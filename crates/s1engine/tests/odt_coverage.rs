//! ODT coverage matrix — round-trip tag census against every ODT fixture.
//!
//! Counterpart to `docx_coverage.rs`. For every `.odt` under
//! `testdocs/odt/`, parses it through Casual Core, exports back to ODT,
//! and diffs the `content.xml` tag census in vs out. The output is the
//! baseline scorecard for the v0.2.x ODT preservation milestone — same
//! playbook that drove DOCX from 10/39 to 39/39 zero-drop.
//!
//! This test is a *reporter*. It prints the scorecard and writes
//! `target/odt-coverage.json`. It does not fail on dropped tags yet —
//! the preservation layer (`s1-odf`, mirror of `s1-ooxml`) is the
//! follow-up work that will close any drops. Once it lands, this test
//! will ratchet to assert zero drop.
//!
//! Run: `cargo test --package s1engine --test odt_coverage -- --nocapture`

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use s1engine::{Engine, Format};

fn fixture_dirs() -> Vec<PathBuf> {
    let testdocs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdocs")
        .join("odt");
    let mut dirs = Vec::new();
    for sub in ["samples", "realworld"] {
        let p = testdocs.join(sub);
        if p.exists() {
            dirs.push(p);
        }
    }
    dirs
}

fn report_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("odt-coverage.json")
}

/// Pull `content.xml` out of an ODT ZIP archive.
fn extract_content_xml(odt: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(odt)).ok()?;
    let mut entry = zip.by_name("content.xml").ok()?;
    let mut s = String::new();
    entry.read_to_string(&mut s).ok()?;
    Some(s)
}

/// `(prefix:local-name → count)` of every start-tag in the XML.
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

#[derive(Debug, Default)]
struct FixtureReport {
    name: String,
    bytes_in: usize,
    bytes_out: usize,
    parsed: bool,
    written: bool,
    /// `(tag, (in_count, out_count))` for tags whose output count dropped
    /// to zero relative to input.
    dropped: BTreeMap<String, (u32, u32)>,
    /// Tags new in output (writer added them).
    added: BTreeMap<String, u32>,
}

fn audit_one(path: &Path) -> FixtureReport {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut report = FixtureReport {
        name,
        ..Default::default()
    };

    let bytes_in = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return report,
    };
    report.bytes_in = bytes_in.len();

    let engine = Engine::new();
    let doc = match engine.open_as(&bytes_in, Format::Odt) {
        Ok(d) => d,
        Err(_) => return report,
    };
    report.parsed = true;

    let bytes_out = match doc.export(Format::Odt) {
        Ok(b) => b,
        Err(_) => return report,
    };
    report.bytes_out = bytes_out.len();
    report.written = true;

    let (Some(in_xml), Some(out_xml)) = (
        extract_content_xml(&bytes_in),
        extract_content_xml(&bytes_out),
    ) else {
        return report;
    };
    let in_tags = tag_census(&in_xml);
    let out_tags = tag_census(&out_xml);

    for (tag, &in_count) in &in_tags {
        let out_count = out_tags.get(tag).copied().unwrap_or(0);
        if out_count == 0 && in_count > 0 {
            report.dropped.insert(tag.clone(), (in_count, out_count));
        }
    }
    for (tag, &out_count) in &out_tags {
        if !in_tags.contains_key(tag) {
            report.added.insert(tag.clone(), out_count);
        }
    }
    report
}

#[test]
fn odt_coverage_audit() {
    let dirs = fixture_dirs();
    if dirs.is_empty() {
        eprintln!("no testdocs/odt/ subdirs — skipping");
        return;
    }

    let mut paths: Vec<PathBuf> = dirs
        .into_iter()
        .flat_map(|d| {
            fs::read_dir(&d)
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("odt"))
                .collect::<Vec<_>>()
        })
        .collect();
    paths.sort();

    if paths.is_empty() {
        eprintln!("no .odt fixtures found — skipping");
        return;
    }

    let reports: Vec<FixtureReport> = paths.iter().map(|p| audit_one(p)).collect();
    let total = reports.len();
    let parsed = reports.iter().filter(|r| r.parsed).count();
    let written = reports.iter().filter(|r| r.written).count();
    let zero_drop = reports
        .iter()
        .filter(|r| r.parsed && r.written && r.dropped.is_empty())
        .count();

    eprintln!();
    eprintln!("═══ ODT coverage report ═══");
    eprintln!("Fixtures total : {total}");
    eprintln!("  parsed       : {parsed}");
    eprintln!("  written      : {written}");
    eprintln!("  zero-drop    : {zero_drop} / {total}");

    let mut drops_total: BTreeMap<String, u32> = BTreeMap::new();
    for r in &reports {
        for (tag, (n, _)) in &r.dropped {
            *drops_total.entry(tag.clone()).or_insert(0) += n;
        }
    }
    if !drops_total.is_empty() {
        eprintln!();
        eprintln!(
            "Dropped tags ({} unique — preservation backlog):",
            drops_total.len()
        );
        for (tag, n) in drops_total.iter().take(30) {
            eprintln!("  {tag:36} {n}x");
        }
        if drops_total.len() > 30 {
            eprintln!("  … ({} more)", drops_total.len() - 30);
        }
    }

    eprintln!();
    eprintln!("Per-fixture:");
    for r in &reports {
        let status = match (r.parsed, r.written, r.dropped.is_empty()) {
            (true, true, true) => "zero-drop",
            (true, true, false) => "drops",
            (true, false, _) => "write-failed",
            _ => "parse-failed",
        };
        eprintln!(
            "  {:<35} {:>9} ({} drop tag(s), bytes {} → {})",
            r.name,
            status,
            r.dropped.len(),
            r.bytes_in,
            r.bytes_out
        );
    }

    // JSON report for the integration scorecard.
    let mut json = String::from("{\n");
    json.push_str(&format!("  \"total\": {total},\n"));
    json.push_str(&format!("  \"parsed\": {parsed},\n"));
    json.push_str(&format!("  \"written\": {written},\n"));
    json.push_str(&format!("  \"zero_drop\": {zero_drop},\n"));
    json.push_str("  \"dropped_tags\": {\n");
    let mut first = true;
    for (tag, n) in &drops_total {
        if !first {
            json.push_str(",\n");
        }
        first = false;
        json.push_str(&format!("    \"{tag}\": {n}"));
    }
    json.push_str("\n  }\n}\n");
    let path = report_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, json);
}
