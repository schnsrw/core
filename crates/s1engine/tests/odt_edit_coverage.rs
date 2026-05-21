//! ODT edit-path coverage audit.
//!
//! Counterpart to `docx_edit_coverage.rs`. For every `.odt` fixture
//! under `testdocs/odt/`, opens it through the preservation-aware
//! engine path, fires a trivial mutation to flip `model_dirty`, exports
//! again, and measures:
//!
//! 1. **Non-body part preservation (ODT Phase 2a)** — every package
//!    part other than `content.xml` should be byte-identical between
//!    input and output (or tag-census identical for XML parts, since
//!    the OOXML/ODF writers normalise whitespace and attribute order).
//!    `styles.xml`, `meta.xml`, `settings.xml`,
//!    `META-INF/manifest.xml`, `Pictures/*`, `Configurations2/*`,
//!    `Thumbnails/*` must all ride through.
//!
//! 2. **Body tag census (ODT Phase 2b target)** — what survives
//!    inside `content.xml` once it's been regenerated from the model.
//!    This is Phase 2a's known limitation: body unknowns are lost on
//!    edit. The forthcoming `BodyOrigin` for ODT closes this.
//!
//! Run: `cargo test --package s1engine --test odt_edit_coverage -- --nocapture`

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use s1_model::AttributeMap;
use s1_ops::Operation;
use s1engine::{Engine, Format};

fn fixture_dirs() -> Vec<PathBuf> {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdocs")
        .join("odt");
    let mut dirs = Vec::new();
    for sub in ["samples", "realworld"] {
        let p = base.join(sub);
        if p.exists() {
            dirs.push(p);
        }
    }
    dirs
}

fn read_all_parts(odt: &[u8]) -> Option<BTreeMap<String, Vec<u8>>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(odt)).ok()?;
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

fn is_xml_extension(name: &str) -> bool {
    name.ends_with(".xml") || name.ends_with(".rdf")
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

/// Compare two parts for "the same content". Binary parts must be
/// byte-equal. XML parts may differ in whitespace / attribute order
/// after the writer round-trip; only the start-tag census needs to
/// match for the preservation contract.
fn part_structural_eq(name: &str, a: &[u8], b: &[u8]) -> bool {
    if a == b {
        return true;
    }
    if !is_xml_extension(name) {
        return false;
    }
    let (Ok(a_str), Ok(b_str)) = (std::str::from_utf8(a), std::str::from_utf8(b)) else {
        return false;
    };
    tag_census(a_str) == tag_census(b_str)
}

fn extract_content_xml(odt: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(odt)).ok()?;
    let mut entry = zip.by_name("content.xml").ok()?;
    let mut s = String::new();
    entry.read_to_string(&mut s).ok()?;
    Some(s)
}

const BODY_PART: &str = "content.xml";

#[derive(Debug, Default)]
struct EditReport {
    name: String,
    parsed: bool,
    written: bool,
    /// Parts that drifted, excluding `content.xml`.
    non_body_drift: Vec<String>,
    /// Body XML tags dropped — expected under Phase 2a.
    body_dropped_tags: BTreeMap<String, (u32, u32)>,
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
    let mut doc = match engine.open_as(&bytes_in, Format::Odt) {
        Ok(d) => d,
        Err(_) => return report,
    };
    report.parsed = true;

    // Trivial mutation: a no-op `SetAttributes` on the body root.
    // Sets an empty attribute map, which is semantically a no-op but
    // routes through `apply_transaction` and flips `model_dirty` so
    // `export(Odt)` takes the Phase 2a splice path. Targets the body
    // because every ODT has one — fixtures without TOCs or paragraphs
    // still trigger the dirty bit.
    let body_id = match doc.body_id() {
        Some(id) => id,
        None => return report,
    };
    let op = Operation::SetAttributes {
        target_id: body_id,
        attributes: AttributeMap::new(),
        previous: None,
    };
    if doc.apply(op).is_err() {
        return report;
    }
    if !doc.is_dirty() {
        return report;
    }

    let bytes_out = match doc.export(Format::Odt) {
        Ok(b) => b,
        Err(_) => return report,
    };
    report.written = true;

    let in_parts = match read_all_parts(&bytes_in) {
        Some(p) => p,
        None => return report,
    };
    let out_parts = match read_all_parts(&bytes_out) {
        Some(p) => p,
        None => return report,
    };

    for (name, in_bytes) in &in_parts {
        if name == BODY_PART {
            continue;
        }
        match out_parts.get(name) {
            Some(out_bytes) if part_structural_eq(name, in_bytes, out_bytes) => {}
            Some(_) => report.non_body_drift.push(format!("modified: {name}")),
            None => report.non_body_drift.push(format!("missing: {name}")),
        }
    }
    for name in out_parts.keys() {
        if name == BODY_PART {
            continue;
        }
        if !in_parts.contains_key(name) {
            report.non_body_drift.push(format!("added: {name}"));
        }
    }

    if let (Some(in_xml), Some(out_xml)) = (
        extract_content_xml(&bytes_in),
        extract_content_xml(&bytes_out),
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
fn odt_edit_coverage_audit() {
    let dirs = fixture_dirs();
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
        eprintln!("no .odt fixtures — skipping");
        return;
    }

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
    eprintln!("═══ ODT edit-path coverage report ═══");
    eprintln!("Fixtures total                       : {total}");
    eprintln!("  parsed                             : {parsed}");
    eprintln!("  re-written after edit              : {written}");
    eprintln!("  non-body parts preserved (Phase 2a) : {non_body_clean} / {written}");
    eprintln!("  body zero-drop (Phase 2b target)    : {body_zero_drop} / {written}");

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

    let mut body_drops_total: BTreeMap<String, u32> = BTreeMap::new();
    for r in &reports {
        for (tag, (n, _)) in &r.body_dropped_tags {
            *body_drops_total.entry(tag.clone()).or_insert(0) += n;
        }
    }
    if !body_drops_total.is_empty() {
        eprintln!();
        eprintln!(
            "Body tags dropped on edit ({} unique — Phase 2b backlog):",
            body_drops_total.len()
        );
        for (tag, n) in body_drops_total.iter().take(30) {
            eprintln!("  {tag:36} {n}x");
        }
        if body_drops_total.len() > 30 {
            eprintln!("  … ({} more)", body_drops_total.len() - 30);
        }
    }

    // Phase 2a contract: for fixtures that did get re-written, every
    // non-body part must ride through. Fixtures that didn't trigger
    // a mutation (`written == false`) are skipped from this gate.
    assert_eq!(
        non_body_clean,
        written,
        "{} of {} re-written fixtures lost non-body parts under edit — ODT splice is broken",
        written - non_body_clean,
        written
    );
}
