//! Unified per-construct fidelity scorecard for DOCX + ODT.
//!
//! Walks every fixture under `testdocs/docx/eigenpal/` and
//! `testdocs/odt/{samples,realworld}/`, then for each `(format, path)`
//! tuple — no-edit and with-edit — counts the instances of every
//! tracked **construct family** (paragraphs, runs, tables, drawings,
//! text boxes, math, vector primitives, lists, footnotes, comments,
//! hyperlinks, bookmarks, fields, tracked changes, TOCs,
//! header/footer references) in the input and output bodies and
//! reports per-construct survival as a percentage.
//!
//! Output:
//!
//! - Per-format / per-path / per-construct table on stderr
//! - JSON dump to `target/fidelity-score.json`
//! - Markdown rollup to `docs/fidelity-scorecard.md`
//!
//! This test is a *reporter*, not a gate. The contract gates live in
//! `docx_coverage` / `docx_edit_coverage` / `odt_coverage` /
//! `odt_edit_coverage`. This is the user-facing single-number answer
//! to "what's our fidelity score across every construct".
//!
//! Run: `cargo test --package s1engine --test fidelity_score -- --nocapture`

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use s1_model::AttributeMap;
use s1_ops::Operation;
use s1engine::{Engine, Format};

// ─── Construct taxonomy ─────────────────────────────────────────────────────

/// Bucket → list of `(prefix:local-name)` patterns. A tag matches a
/// bucket when its full name equals one of the patterns *or* its
/// prefix matches a `prefix:*` wildcard pattern.
const CONSTRUCTS: &[(&str, &[&str])] = &[
    ("Paragraphs", &["w:p", "text:p", "text:h"]),
    ("Runs / spans", &["w:r", "text:span"]),
    (
        "Tables",
        &[
            "w:tbl",
            "w:tr",
            "w:tc",
            "table:table",
            "table:table-row",
            "table:table-cell",
            "table:table-column",
            "table:table-columns",
        ],
    ),
    (
        "Drawings (DrawingML)",
        &[
            "w:drawing",
            "a:*",
            "pic:*",
            "wp:*",
            "wps:*",
            "wpg:*",
            "wpc:*",
        ],
    ),
    ("Drawings (VML / legacy)", &["w:pict", "v:*", "o:*"]),
    (
        "Drawings (ODT)",
        &[
            "draw:frame",
            "draw:image",
            "draw:object",
            "draw:custom-shape",
            "draw:rect",
            "draw:circle",
            "draw:ellipse",
            "draw:line",
            "draw:polygon",
            "draw:polyline",
            "draw:path",
            "draw:g",
            "draw:connector",
            "draw:contour-polygon",
            "draw:enhanced-geometry",
        ],
    ),
    (
        "Text boxes",
        &["w:txbxContent", "wps:txbx", "draw:text-box"],
    ),
    ("Math", &["m:*", "math:*"]),
    ("Vectors / SVG primitives", &["svg:*"]),
    (
        "Lists",
        &[
            "w:numPr",
            "w:numId",
            "w:ilvl",
            "text:list",
            "text:list-item",
            "text:list-header",
        ],
    ),
    (
        "Footnotes / endnotes",
        &[
            "w:footnoteReference",
            "w:endnoteReference",
            "text:note",
            "text:note-citation",
            "text:note-body",
            "text:note-ref",
        ],
    ),
    (
        "Comments",
        &[
            "w:commentRangeStart",
            "w:commentRangeEnd",
            "w:commentReference",
            "office:annotation",
            "office:annotation-end",
        ],
    ),
    ("Hyperlinks", &["w:hyperlink", "text:a"]),
    (
        "Bookmarks",
        &[
            "w:bookmarkStart",
            "w:bookmarkEnd",
            "text:bookmark",
            "text:bookmark-start",
            "text:bookmark-end",
            "text:bookmark-ref",
        ],
    ),
    (
        "Fields",
        &[
            "w:fldChar",
            "w:instrText",
            "w:fldSimple",
            "text:variable-set",
            "text:variable-get",
            "text:variable-decl",
            "text:variable-decls",
            "text:sequence",
            "text:sequence-decl",
            "text:sequence-decls",
            "text:user-field-get",
            "text:user-field-decl",
            "text:user-field-decls",
        ],
    ),
    (
        "Tracked changes",
        &[
            "w:ins",
            "w:del",
            "w:moveTo",
            "w:moveFrom",
            "text:tracked-changes",
            "text:changed-region",
            "text:change",
            "text:change-start",
            "text:change-end",
            "text:deletion",
            "text:insertion",
            "text:format-change",
        ],
    ),
    (
        "TOCs",
        &[
            "w:sdt",
            "text:table-of-content",
            "text:table-of-content-source",
        ],
    ),
    (
        "Header / footer references",
        &[
            "w:headerReference",
            "w:footerReference",
            "style:header",
            "style:footer",
        ],
    ),
    (
        "Section / page geometry",
        &[
            "w:sectPr",
            "w:pgSz",
            "w:pgMar",
            "w:cols",
            "style:page-layout",
            "style:page-layout-properties",
        ],
    ),
    (
        "Soft formatting & whitespace",
        &[
            "w:br",
            "w:tab",
            "w:cr",
            "text:line-break",
            "text:tab",
            "text:s",
            "text:soft-page-break",
        ],
    ),
];

fn pattern_matches(tag: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix(":*") {
        if let Some(idx) = tag.find(':') {
            return &tag[..idx] == prefix;
        }
        return false;
    }
    tag == pattern
}

fn bucket_for(tag: &str) -> Option<&'static str> {
    for (name, patterns) in CONSTRUCTS {
        for p in *patterns {
            if pattern_matches(tag, p) {
                return Some(*name);
            }
        }
    }
    None
}

// ─── Fixture walk ───────────────────────────────────────────────────────────

fn manifest_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn docx_fixtures() -> Vec<PathBuf> {
    let dir = manifest_root()
        .join("..")
        .join("..")
        .join("testdocs")
        .join("docx")
        .join("eigenpal");
    list_files(&dir, "docx")
}

fn odt_fixtures() -> Vec<PathBuf> {
    let base = manifest_root()
        .join("..")
        .join("..")
        .join("testdocs")
        .join("odt");
    let mut v: Vec<PathBuf> = ["samples", "realworld"]
        .iter()
        .flat_map(|sub| list_files(&base.join(sub), "odt"))
        .collect();
    v.sort();
    v
}

fn list_files(dir: &Path, ext: &str) -> Vec<PathBuf> {
    if !dir.exists() {
        return Vec::new();
    }
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some(ext))
        .collect();
    v.sort();
    v
}

fn extract_body_xml(bytes: &[u8], format: Format) -> Option<String> {
    let part_name = match format {
        Format::Docx => "word/document.xml",
        Format::Odt => "content.xml",
        _ => return None,
    };
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut entry = zip.by_name(part_name).ok()?;
    let mut s = String::new();
    entry.read_to_string(&mut s).ok()?;
    Some(s)
}

fn tag_census(xml: &str) -> BTreeMap<String, u32> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
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

#[derive(Default, Debug, Clone)]
struct Bucket {
    in_count: u64,
    out_count: u64,
}

fn aggregate(in_xml: &str, out_xml: &str) -> BTreeMap<String, Bucket> {
    let in_tags = tag_census(in_xml);
    let out_tags = tag_census(out_xml);
    let mut by_bucket: BTreeMap<String, Bucket> = BTreeMap::new();

    for (tag, n) in &in_tags {
        if let Some(b) = bucket_for(tag) {
            by_bucket.entry(b.to_string()).or_default().in_count += *n as u64;
        }
    }
    for (tag, n) in &out_tags {
        if let Some(b) = bucket_for(tag) {
            by_bucket.entry(b.to_string()).or_default().out_count += *n as u64;
        }
    }
    by_bucket
}

fn pct(out: u64, input: u64) -> f64 {
    if input == 0 {
        return 100.0;
    }
    (out.min(input) as f64 / input as f64) * 100.0
}

// ─── Runs ───────────────────────────────────────────────────────────────────

enum EditMode {
    None,
    Edit,
}

fn run_one_fixture(
    path: &Path,
    format: Format,
    mode: &EditMode,
) -> Option<BTreeMap<String, Bucket>> {
    let bytes_in = fs::read(path).ok()?;
    let engine = Engine::new();
    let mut doc = engine.open_as(&bytes_in, format).ok()?;

    if matches!(mode, EditMode::Edit) {
        let body_id = doc.body_id()?;
        let _ = doc.apply(Operation::SetAttributes {
            target_id: body_id,
            attributes: AttributeMap::new(),
            previous: None,
        });
    }

    let bytes_out = doc.export(format).ok()?;
    let in_xml = extract_body_xml(&bytes_in, format)?;
    let out_xml = extract_body_xml(&bytes_out, format)?;
    Some(aggregate(&in_xml, &out_xml))
}

fn aggregate_lane(
    fixtures: &[PathBuf],
    format: Format,
    mode: EditMode,
) -> BTreeMap<String, Bucket> {
    let mut total: BTreeMap<String, Bucket> = BTreeMap::new();
    for fx in fixtures {
        if let Some(buckets) = run_one_fixture(fx, format, &mode) {
            for (name, b) in buckets {
                let e = total.entry(name).or_default();
                e.in_count += b.in_count;
                e.out_count += b.out_count;
            }
        }
    }
    total
}

#[derive(Debug)]
struct LaneReport {
    label: String,
    fixtures: usize,
    buckets: BTreeMap<String, Bucket>,
}

impl LaneReport {
    fn overall_pct(&self) -> f64 {
        let (i, o) = self
            .buckets
            .values()
            .fold((0u64, 0u64), |(i, o), b| (i + b.in_count, o + b.out_count));
        pct(o, i)
    }
}

// ─── Output ─────────────────────────────────────────────────────────────────

fn print_lane(lane: &LaneReport) {
    eprintln!();
    eprintln!(
        "── {} (n={}, overall {:.2}%) ──",
        lane.label,
        lane.fixtures,
        lane.overall_pct()
    );
    eprintln!(
        "  {:<36} {:>9} {:>9} {:>7}",
        "Construct", "input", "output", "%"
    );
    for (name, b) in &lane.buckets {
        if b.in_count == 0 && b.out_count == 0 {
            continue;
        }
        eprintln!(
            "  {:<36} {:>9} {:>9} {:>6.1}%",
            name,
            b.in_count,
            b.out_count,
            pct(b.out_count, b.in_count)
        );
    }
}

fn lane_to_markdown(lane: &LaneReport, out: &mut String) {
    use std::fmt::Write;
    let _ = writeln!(
        out,
        "\n### {} — overall **{:.2}%** ({} fixtures)\n",
        lane.label,
        lane.overall_pct(),
        lane.fixtures
    );
    let _ = writeln!(out, "| Construct | Input | Output | % |");
    let _ = writeln!(out, "| --- | ---: | ---: | ---: |");
    for (name, b) in &lane.buckets {
        if b.in_count == 0 && b.out_count == 0 {
            continue;
        }
        let _ = writeln!(
            out,
            "| {} | {} | {} | {:.1}% |",
            name,
            b.in_count,
            b.out_count,
            pct(b.out_count, b.in_count)
        );
    }
}

fn write_scorecard(lanes: &[LaneReport]) {
    let target = manifest_root()
        .join("..")
        .join("..")
        .join("target")
        .join("fidelity-score.json");
    if let Some(p) = target.parent() {
        let _ = fs::create_dir_all(p);
    }
    let mut json = String::from("[\n");
    for (i, lane) in lanes.iter().enumerate() {
        if i > 0 {
            json.push_str(",\n");
        }
        json.push_str(&format!(
            "  {{\"lane\": \"{}\", \"fixtures\": {}, \"overall_pct\": {:.4}, \"buckets\": {{",
            lane.label,
            lane.fixtures,
            lane.overall_pct()
        ));
        let mut first = true;
        for (name, b) in &lane.buckets {
            if b.in_count == 0 && b.out_count == 0 {
                continue;
            }
            if !first {
                json.push(',');
            }
            first = false;
            json.push_str(&format!(
                "\"{}\":{{\"in\":{},\"out\":{},\"pct\":{:.4}}}",
                name,
                b.in_count,
                b.out_count,
                pct(b.out_count, b.in_count)
            ));
        }
        json.push_str("}}");
    }
    json.push_str("\n]\n");
    let _ = fs::write(&target, json);

    let mut md = String::from(
        "# Fidelity scorecard\n\n\
        Auto-generated by `crates/s1engine/tests/fidelity_score.rs`. \
        Counts every tag inside the body part (`word/document.xml` for DOCX, \
        `content.xml` for ODT) of every fixture, groups by construct family, \
        and reports `(output instances / input instances)` per family. \
        100% means every input instance of that construct survives the \
        round-trip on that lane.\n\n\
        Lanes:\n\n\
        - **no-edit** — `Engine::open(format) → Document::export(format)` \
          with no mutation. Tests the preservation re-emit path.\n\
        - **with-edit** — same plus a no-op `SetAttributes` on the body \
          root to flip `model_dirty` and force the splice path. Tests \
          Phase 2a / 2b body splicing.\n",
    );
    for lane in lanes {
        lane_to_markdown(lane, &mut md);
    }

    let md_path = manifest_root()
        .join("..")
        .join("..")
        .join("docs")
        .join("fidelity-scorecard.md");
    let _ = fs::write(&md_path, md);
}

#[test]
fn fidelity_scorecard() {
    let docx = docx_fixtures();
    let odt = odt_fixtures();

    let lanes = vec![
        LaneReport {
            label: "DOCX · no-edit".to_string(),
            fixtures: docx.len(),
            buckets: aggregate_lane(&docx, Format::Docx, EditMode::None),
        },
        LaneReport {
            label: "DOCX · with-edit".to_string(),
            fixtures: docx.len(),
            buckets: aggregate_lane(&docx, Format::Docx, EditMode::Edit),
        },
        LaneReport {
            label: "ODT · no-edit".to_string(),
            fixtures: odt.len(),
            buckets: aggregate_lane(&odt, Format::Odt, EditMode::None),
        },
        LaneReport {
            label: "ODT · with-edit".to_string(),
            fixtures: odt.len(),
            buckets: aggregate_lane(&odt, Format::Odt, EditMode::Edit),
        },
    ];

    eprintln!();
    eprintln!("═══════════════════════════════════════════════════════════════════════");
    eprintln!("                Casual Core — per-construct fidelity scorecard");
    eprintln!("═══════════════════════════════════════════════════════════════════════");

    for lane in &lanes {
        print_lane(lane);
    }

    eprintln!();
    eprintln!("Overall by lane:");
    for lane in &lanes {
        eprintln!("  {:<24} {:.2}%", lane.label, lane.overall_pct());
    }

    write_scorecard(&lanes);
}
