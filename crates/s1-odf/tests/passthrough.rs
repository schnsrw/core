//! Passthrough audit — runs every fixture under `testdocs/odt/` through
//! `Package::parse → Package::write → re-parse` and verifies that every
//! XML tag in `content.xml` survives the round trip.
//!
//! Counterpart of `s1-ooxml::tests::passthrough`. If this is green we
//! know the preservation layer doesn't drop anything, which is the
//! prerequisite for using it as the foundation of `s1-format-odt`'s
//! Phase 2 fidelity pass.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use s1_odf::Package;

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

fn extract_content_xml(odt: &[u8]) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(odt)).ok()?;
    let mut entry = zip.by_name("content.xml").ok()?;
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

#[test]
fn passthrough_audit() {
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

    let mut total = 0usize;
    let mut zero_drop = 0usize;
    let mut parse_failures: Vec<String> = Vec::new();
    let mut write_failures: Vec<String> = Vec::new();
    let mut tag_loss: Vec<(String, BTreeMap<String, (u32, u32)>)> = Vec::new();
    let mut dropped_overall: BTreeMap<String, u32> = BTreeMap::new();

    for path in &paths {
        let fname = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        total += 1;
        let bytes_in = match fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let pkg = match Package::parse(&bytes_in) {
            Ok(p) => p,
            Err(e) => {
                parse_failures.push(format!("{fname}: {e}"));
                continue;
            }
        };
        let bytes_out = match pkg.write() {
            Ok(b) => b,
            Err(e) => {
                write_failures.push(format!("{fname}: {e}"));
                continue;
            }
        };

        let in_xml = match extract_content_xml(&bytes_in) {
            Some(s) => s,
            None => continue,
        };
        let out_xml = match extract_content_xml(&bytes_out) {
            Some(s) => s,
            None => {
                write_failures.push(format!("{fname}: no content.xml in output"));
                continue;
            }
        };
        let in_tags = tag_census(&in_xml);
        let out_tags = tag_census(&out_xml);
        let mut drops = BTreeMap::new();
        for (tag, &in_count) in &in_tags {
            let out_count = out_tags.get(tag).copied().unwrap_or(0);
            if out_count == 0 && in_count > 0 {
                drops.insert(tag.clone(), (in_count, out_count));
            }
        }
        if drops.is_empty() {
            zero_drop += 1;
        } else {
            for (tag, (n, _)) in &drops {
                *dropped_overall.entry(tag.clone()).or_insert(0) += n;
            }
            tag_loss.push((fname, drops));
        }
    }

    eprintln!();
    eprintln!("═══ s1-odf passthrough audit ═══");
    eprintln!("Fixtures total : {total}");
    eprintln!("  zero-drop    : {zero_drop} / {total}");
    eprintln!("  parse fails  : {}", parse_failures.len());
    eprintln!("  write fails  : {}", write_failures.len());
    if !parse_failures.is_empty() {
        eprintln!("Parse failures:");
        for f in &parse_failures {
            eprintln!("  {f}");
        }
    }
    if !write_failures.is_empty() {
        eprintln!("Write failures:");
        for f in &write_failures {
            eprintln!("  {f}");
        }
    }
    if !dropped_overall.is_empty() {
        eprintln!("Total dropped tags ({} unique):", dropped_overall.len());
        for (tag, n) in &dropped_overall {
            eprintln!("  {tag:36} {n}x");
        }
        eprintln!("Per-fixture drops:");
        for (name, drops) in &tag_loss {
            eprintln!("  {name}");
            for (tag, (in_c, out_c)) in drops {
                eprintln!("    {tag:32} in={in_c} out={out_c}");
            }
        }
    }

    assert!(
        parse_failures.is_empty(),
        "{} fixtures failed to parse",
        parse_failures.len()
    );
    assert!(
        write_failures.is_empty(),
        "{} fixtures failed to write",
        write_failures.len()
    );
    assert_eq!(
        zero_drop,
        total,
        "{} of {} fixtures drop tags on round-trip — preservation broken",
        total - zero_drop,
        total
    );
}
