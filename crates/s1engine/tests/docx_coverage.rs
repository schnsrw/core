//! DOCX coverage matrix — three-bucket round-trip audit.
//!
//! For every fixture in `testdocs/docx/eigenpal/`, this test:
//!
//! 1. Reads the input DOCX.
//! 2. Extracts the XML tag census from the input `document.xml`.
//! 3. Opens it with Casual Core's engine, exports back to DOCX.
//! 4. Extracts the XML tag census from the output `document.xml`.
//! 5. Reports per-tag deltas: tags that appear in input but vanish in output.
//!
//! Cross-referenced against the consumer's
//! `docx-editor/roundtrip-audit-report.md` "Global rollup" list, this gives
//! the three-bucket coverage matrix described in `docs/integration-plan.md`:
//!
//! - **Bucket A** — consumer handles, we drop (must close before integration)
//! - **Bucket B** — we handle, consumer drops (wins we bring)
//! - **Bucket C** — neither handles
//!
//! This test never fails. It's a *reporting* test — its job is to produce
//! the data the integration plan's phase gates depend on. The output is
//! written to `target/docx-coverage.json`.
//!
//! Run with: `cargo test --package s1engine --test docx_coverage -- --nocapture`

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use s1engine::{Engine, Format};

/// Tags the consumer (eigenpal docx-editor) is known to parse-but-drop.
/// Pulled from `docx-editor/roundtrip-audit-report.md` global rollup.
const CONSUMER_DROPPED_TAGS: &[&str] = &[
    "w:fldChar",
    "w:rPr",
    "w:pBdr",
    "w:spacing",
    "w:ind",
    "w:start",
    "w:end",
    "w:pgNumType",
    "w:rStyle",
    "w:formProt",
    "w:textDirection",
    "wp14:sizeRelH",
    "wp14:pctWidth",
    "wp14:sizeRelV",
    "wp14:pctHeight",
    "w:highlight",
    "w:bookmarkEnd",
    "w:bdr",
    "w:delInstrText",
    "w:instrText",
    "w:footnotePr",
    "w:endnotePr",
];

fn eigenpal_fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdocs")
        .join("docx")
        .join("eigenpal")
}

fn target_report_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("docx-coverage.json")
}

/// Extract `word/document.xml` from a DOCX byte slice.
fn extract_document_xml(docx_bytes: &[u8]) -> Option<String> {
    let reader = Cursor::new(docx_bytes);
    let mut zip = zip::ZipArchive::new(reader).ok()?;
    let mut entry = zip.by_name("word/document.xml").ok()?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf).ok()?;
    Some(buf)
}

/// Count opening tags (`<w:foo>` and `<w:foo/>`) keyed by `prefix:localname`.
fn tag_census(xml: &str) -> BTreeMap<String, u32> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                *counts.entry(name).or_insert(0) += 1;
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    counts
}

#[derive(Debug)]
struct FixtureReport {
    name: String,
    parsed: bool,
    written: bool,
    dropped_tags: BTreeMap<String, (u32, u32)>, // tag -> (in_count, out_count)
}

fn audit_fixture(path: &Path) -> FixtureReport {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut report = FixtureReport {
        name,
        parsed: false,
        written: false,
        dropped_tags: BTreeMap::new(),
    };

    let bytes_in = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return report,
    };

    let in_xml = match extract_document_xml(&bytes_in) {
        Some(x) => x,
        None => return report,
    };
    let in_tags = tag_census(&in_xml);

    let engine = Engine::new();
    let doc = match engine.open_as(&bytes_in, Format::Docx) {
        Ok(d) => d,
        Err(_) => return report,
    };
    report.parsed = true;

    let bytes_out = match doc.export(Format::Docx) {
        Ok(b) => b,
        Err(_) => return report,
    };
    report.written = true;

    let out_xml = match extract_document_xml(&bytes_out) {
        Some(x) => x,
        None => return report,
    };
    let out_tags = tag_census(&out_xml);

    // Methodology: match eigenpal's audit — only flag tags whose output count
    // is exactly 0 ("truly parse-but-drop"). Reduced-count noise from writer
    // consolidation (collapsing adjacent runs, merging rPr) is excluded.
    for (tag, &in_count) in &in_tags {
        let out_count = out_tags.get(tag).copied().unwrap_or(0);
        if out_count == 0 && in_count > 0 {
            report
                .dropped_tags
                .insert(tag.clone(), (in_count, out_count));
        }
    }

    report
}

#[derive(Debug, Default)]
struct Buckets {
    /// Consumer supports, we drop. Critical — these block integration.
    bucket_a: BTreeMap<String, u32>,
    /// We support, consumer drops. Wins we bring to integration.
    bucket_b: Vec<String>,
    /// Neither supports.
    bucket_c: BTreeMap<String, u32>,
    /// Both support.
    matched: u64,
    /// Per-fixture summary.
    fixtures: Vec<(String, bool, bool, usize)>,
}

fn classify(reports: &[FixtureReport]) -> Buckets {
    let mut b = Buckets::default();
    let consumer_drops: std::collections::HashSet<&str> =
        CONSUMER_DROPPED_TAGS.iter().copied().collect();

    let mut our_drops_aggregate: BTreeMap<String, u32> = BTreeMap::new();
    for r in reports {
        b.fixtures
            .push((r.name.clone(), r.parsed, r.written, r.dropped_tags.len()));
        for (tag, (in_count, _)) in &r.dropped_tags {
            *our_drops_aggregate.entry(tag.clone()).or_insert(0) += *in_count;
        }
    }

    for (tag, total) in &our_drops_aggregate {
        if consumer_drops.contains(tag.as_str()) {
            *b.bucket_c.entry(tag.clone()).or_insert(0) += *total;
        } else {
            *b.bucket_a.entry(tag.clone()).or_insert(0) += *total;
        }
    }

    for tag in CONSUMER_DROPPED_TAGS {
        if !our_drops_aggregate.contains_key(*tag) {
            b.bucket_b.push((*tag).to_string());
        }
    }

    b
}

fn render_json(reports: &[FixtureReport], buckets: &Buckets) -> String {
    let mut s = String::from("{\n");
    s.push_str("  \"fixtures\": [\n");
    for (i, r) in reports.iter().enumerate() {
        s.push_str("    {");
        s.push_str(&format!("\"name\":\"{}\",", r.name.replace('"', "\\\"")));
        s.push_str(&format!("\"parsed\":{},", r.parsed));
        s.push_str(&format!("\"written\":{},", r.written));
        s.push_str("\"dropped_tags\":{");
        for (j, (tag, (in_c, out_c))) in r.dropped_tags.iter().enumerate() {
            if j > 0 {
                s.push(',');
            }
            s.push_str(&format!("\"{tag}\":[{in_c},{out_c}]"));
        }
        s.push_str("}}");
        if i + 1 < reports.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("  ],\n");

    s.push_str("  \"buckets\": {\n");
    s.push_str("    \"a_consumer_handles_we_drop\": {");
    for (i, (tag, total)) in buckets.bucket_a.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("\"{tag}\":{total}"));
    }
    s.push_str("},\n");

    s.push_str("    \"b_we_handle_consumer_drops\": [");
    for (i, tag) in buckets.bucket_b.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("\"{tag}\""));
    }
    s.push_str("],\n");

    s.push_str("    \"c_neither_handles\": {");
    for (i, (tag, total)) in buckets.bucket_c.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("\"{tag}\":{total}"));
    }
    s.push_str("}\n");

    s.push_str("  }\n}\n");
    s
}

fn print_summary(reports: &[FixtureReport], buckets: &Buckets) {
    let total = reports.len();
    let parsed = reports.iter().filter(|r| r.parsed).count();
    let written = reports.iter().filter(|r| r.written).count();
    let zero_drop = reports
        .iter()
        .filter(|r| r.parsed && r.written && r.dropped_tags.is_empty())
        .count();

    eprintln!();
    eprintln!("═══ DOCX coverage report ═══");
    eprintln!("Fixtures total : {total}");
    eprintln!("  parsed       : {parsed}");
    eprintln!("  re-written   : {written}");
    eprintln!("  zero-drop    : {zero_drop} / {total}");
    eprintln!();
    eprintln!(
        "Bucket A — consumer supports, WE DROP  ({} tags)",
        buckets.bucket_a.len()
    );
    for (tag, total) in &buckets.bucket_a {
        eprintln!("  {tag:32} dropped {total}x");
    }
    eprintln!();
    eprintln!(
        "Bucket B — WE support, consumer drops  ({} tags)",
        buckets.bucket_b.len()
    );
    for tag in &buckets.bucket_b {
        eprintln!("  {tag}");
    }
    eprintln!();
    eprintln!(
        "Bucket C — neither supports             ({} tags)",
        buckets.bucket_c.len()
    );
    for (tag, total) in &buckets.bucket_c {
        eprintln!("  {tag:32} dropped {total}x");
    }
    eprintln!();
}

#[test]
fn docx_coverage_audit() {
    let dir = eigenpal_fixtures_dir();
    if !dir.exists() {
        eprintln!(
            "Eigenpal fixtures not present at {} — skipping coverage audit.",
            dir.display()
        );
        return;
    }

    let mut paths: Vec<_> = fs::read_dir(&dir)
        .expect("read eigenpal fixtures dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("docx"))
        .collect();
    paths.sort();

    let reports: Vec<FixtureReport> = paths.iter().map(|p| audit_fixture(p)).collect();
    let buckets = classify(&reports);
    print_summary(&reports, &buckets);

    let json = render_json(&reports, &buckets);
    let report_path = target_report_path();
    if let Some(parent) = report_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&report_path, json).expect("write coverage report");
    eprintln!("Coverage JSON: {}", report_path.display());

    // Reporting test — never fails. Phase gates check the JSON output.
    let parsed = reports.iter().filter(|r| r.parsed).count();
    assert!(
        parsed > 0,
        "no eigenpal fixtures parsed; check {}",
        dir.display()
    );
}
