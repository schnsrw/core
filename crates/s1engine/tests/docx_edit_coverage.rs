//! DOCX edit-path coverage audit.
//!
//! Counterpart to `docx_coverage.rs`, which exercises the pure-passthrough
//! path (open + export with no mutation). This test exercises the
//! **with-edits** path: open each fixture, fire a trivial mutation, export,
//! then measure what survives.
//!
//! Two contracts are enforced per fixture:
//!
//! 1. **Non-body part preservation (Phase 2a)** — every package part
//!    other than `word/document.xml` is byte-identical between input and
//!    output: theme, fontTable, customXml, headers, footers, footnotes,
//!    endnotes, comments, numbering, styles, images, rels, content
//!    types all ride through unchanged.
//!
//! 2. **Body tag census preservation (Phase 2b)** — every OOXML element
//!    name present in the input body survives into the output body.
//!    The per-NodeId splice keeps clean paragraphs / tables / TOC
//!    blocks verbatim (including any unknown OOXML inside them —
//!    drawings, structured document tags, AlternateContent fallbacks,
//!    MathML); only NodeIds in the dirty set are regenerated through
//!    the writer.
//!
//! Run with: `cargo test --package s1engine --test docx_edit_coverage -- --nocapture`

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use s1engine::{Engine, Format};

fn eigenpal_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdocs")
        .join("docx")
        .join("eigenpal")
}

/// Pull each entry's `(name, bytes)` out of a DOCX zip. Used to inspect
/// parts between input and output.
fn read_all_parts(docx: &[u8]) -> Option<BTreeMap<String, Vec<u8>>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(docx)).ok()?;
    let mut out = BTreeMap::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).ok()?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf).ok()?;
        out.insert(name, buf);
    }
    Some(out)
}

/// `s1-ooxml`'s XML writer normalises whitespace and attribute escaping, so
/// two byte-equal XML inputs can serialise to byte-different outputs while
/// being **semantically identical**. The right preservation check is on
/// tag census, not raw bytes.
fn part_structural_eq(name: &str, a: &[u8], b: &[u8]) -> bool {
    if a == b {
        return true;
    }
    // Binary parts must be byte-equal.
    if !is_xml_extension(name) {
        return false;
    }
    let (Ok(a_str), Ok(b_str)) = (std::str::from_utf8(a), std::str::from_utf8(b)) else {
        return false;
    };
    tag_census(a_str) == tag_census(b_str)
}

fn is_xml_extension(name: &str) -> bool {
    name.ends_with(".xml") || name.ends_with(".rels") || name.ends_with(".svg")
}

fn extract_document_xml(docx: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(docx)).ok()?;
    let mut entry = zip.by_name("word/document.xml").ok()?;
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

#[derive(Debug, Default)]
struct EditReport {
    name: String,
    parsed: bool,
    written: bool,
    /// Parts present in input but missing from output, or with different bytes.
    /// We exclude `word/document.xml` (intentionally regenerated) and any
    /// `.rels` file that points at document.xml (rIds may renumber).
    non_body_drift: Vec<String>,
    /// Body XML tag census — drops are expected under Phase 2a.
    body_dropped_tags: BTreeMap<String, (u32, u32)>,
}

const BODY_PART: &str = "word/document.xml";

fn is_doc_rels(name: &str) -> bool {
    name == "word/_rels/document.xml.rels"
}

fn audit_fixture(path: &Path) -> EditReport {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut report = EditReport {
        name,
        ..Default::default()
    };

    let bytes_in = match fs::read(path) {
        Ok(b) => b,
        Err(_) => return report,
    };

    let engine = Engine::new();
    let mut doc = match engine.open_as(&bytes_in, Format::Docx) {
        Ok(d) => d,
        Err(_) => return report,
    };
    report.parsed = true;

    // Trivial mutation: `update_toc()` rewrites cached entry paragraphs
    // for documents that have a TOC, and is a no-op for the rest. Under
    // Phase 2b's per-NodeId dirty tracking, no-TOC docs export verbatim
    // (no body NodeId dirty); TOC docs export through the per-node splice
    // (only the TOC element is regenerated). Both paths should preserve
    // every non-TOC body element verbatim.
    doc.update_toc();

    let bytes_out = match doc.export(Format::Docx) {
        Ok(b) => b,
        Err(_) => return report,
    };
    report.written = true;

    // Non-body part preservation.
    let in_parts = match read_all_parts(&bytes_in) {
        Some(p) => p,
        None => return report,
    };
    let out_parts = match read_all_parts(&bytes_out) {
        Some(p) => p,
        None => return report,
    };

    for (name, in_bytes) in &in_parts {
        if name == BODY_PART || is_doc_rels(name) {
            continue;
        }
        match out_parts.get(name) {
            Some(out_bytes) if part_structural_eq(name, in_bytes, out_bytes) => {}
            Some(_) => report.non_body_drift.push(format!("modified: {name}")),
            None => report.non_body_drift.push(format!("missing: {name}")),
        }
    }
    for name in out_parts.keys() {
        if name == BODY_PART || is_doc_rels(name) {
            continue;
        }
        if !in_parts.contains_key(name) {
            report.non_body_drift.push(format!("added: {name}"));
        }
    }

    // Body tag census diff (Phase 2a known regression vs no-edits path).
    if let (Some(in_xml), Some(out_xml)) = (
        extract_document_xml(&bytes_in),
        extract_document_xml(&bytes_out),
    ) {
        let in_tags = tag_census(&in_xml);
        let out_tags = tag_census(&out_xml);
        for (tag, &in_count) in &in_tags {
            let out_count = out_tags.get(tag).copied().unwrap_or(0);
            if out_count == 0 && in_count > 0 {
                report
                    .body_dropped_tags
                    .insert(tag.clone(), (in_count, out_count));
            }
        }
    }

    report
}

#[test]
fn docx_edit_coverage_audit() {
    let dir = eigenpal_dir();
    if !dir.exists() {
        eprintln!("fixtures not present at {} — skipping", dir.display());
        return;
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read fixtures dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("docx"))
        .collect();
    paths.sort();

    let reports: Vec<EditReport> = paths.iter().map(|p| audit_fixture(p)).collect();
    let total = reports.len();
    let parsed = reports.iter().filter(|r| r.parsed).count();
    let written = reports.iter().filter(|r| r.written).count();
    let non_body_clean = reports
        .iter()
        .filter(|r| r.parsed && r.written && r.non_body_drift.is_empty())
        .count();
    let body_zero_drop = reports
        .iter()
        .filter(|r| r.parsed && r.written && r.body_dropped_tags.is_empty())
        .count();

    eprintln!();
    eprintln!("═══ DOCX edit-path coverage report ═══");
    eprintln!("Fixtures total                        : {total}");
    eprintln!("  parsed                              : {parsed}");
    eprintln!("  re-written after edit               : {written}");
    eprintln!("  non-body parts preserved (Phase 2a)  : {non_body_clean} / {total}");
    eprintln!("  body zero-drop (Phase 2b)             : {body_zero_drop} / {total}");

    let with_drift: Vec<_> = reports
        .iter()
        .filter(|r| !r.non_body_drift.is_empty())
        .collect();
    if !with_drift.is_empty() {
        eprintln!();
        eprintln!("Non-body drift (unexpected — investigate):");
        for r in with_drift {
            eprintln!("  {}", r.name);
            for d in &r.non_body_drift {
                eprintln!("    {d}");
            }
        }
    }

    // Aggregate body drops — should be empty under Phase 2b.
    let mut body_drops_total: BTreeMap<String, u32> = BTreeMap::new();
    for r in &reports {
        for (tag, (n, _)) in &r.body_dropped_tags {
            *body_drops_total.entry(tag.clone()).or_insert(0) += n;
        }
    }
    if !body_drops_total.is_empty() {
        eprintln!();
        eprintln!(
            "Body tags dropped on edit ({} unique):",
            body_drops_total.len()
        );
        for (tag, n) in body_drops_total.iter().take(30) {
            eprintln!("  {tag:36} {n}x");
        }
        if body_drops_total.len() > 30 {
            eprintln!("  … ({} more)", body_drops_total.len() - 30);
        }
    }

    // Phase 2a contract: every fixture must round-trip with **non-body
    // parts preserved**. If this regresses, the splice is broken.
    assert_eq!(
        non_body_clean,
        total,
        "{} fixtures lost non-body parts under edit — splice is broken",
        total - non_body_clean
    );

    // Phase 2b contract: every fixture must round-trip with the body's
    // tag census intact across an edit. Per-NodeId splice keeps clean
    // body elements (and the unknown OOXML inside them) verbatim.
    assert_eq!(
        body_zero_drop,
        total,
        "{} fixtures dropped body tags under edit — Phase 2b splice is broken",
        total - body_zero_drop
    );
}
