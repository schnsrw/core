//! Passthrough audit — runs every fixture in `testdocs/docx/eigenpal/`
//! through `Package::parse → Package::write → re-parse` and verifies that
//! every XML tag in `word/document.xml` survives the round trip.
//!
//! This is the load-bearing test for `s1-ooxml`. If it's green we know the
//! preservation layer doesn't drop anything, which is the prerequisite for
//! using it as the foundation of `s1-format-docx`'s fidelity pass.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use s1_ooxml::Package;

fn eigenpal_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdocs")
        .join("docx")
        .join("eigenpal")
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

#[test]
fn passthrough_round_trip_zero_drop() {
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

    let mut total = 0usize;
    let mut zero_drop = 0usize;
    let mut dropped_tags_overall: BTreeMap<String, u32> = BTreeMap::new();
    let mut parse_failures: Vec<String> = Vec::new();
    let mut write_failures: Vec<String> = Vec::new();
    let mut tag_loss_fixtures: Vec<(String, BTreeMap<String, (u32, u32)>)> = Vec::new();

    for path in &paths {
        total += 1;
        let fname = path.file_name().unwrap().to_string_lossy().into_owned();
        let bytes_in = fs::read(path).expect("read fixture");

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

        // Tag census comparison on document.xml.
        let in_xml = match extract_document_xml(&bytes_in) {
            Some(s) => s,
            None => continue,
        };
        let out_xml = match extract_document_xml(&bytes_out) {
            Some(s) => s,
            None => {
                write_failures.push(format!("{fname}: no document.xml in output"));
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
                *dropped_tags_overall.entry(tag.clone()).or_insert(0) += n;
            }
            tag_loss_fixtures.push((fname.clone(), drops));
        }
    }

    eprintln!();
    eprintln!("═══ s1-ooxml passthrough audit ═══");
    eprintln!("Fixtures total : {total}");
    eprintln!("  zero-drop    : {zero_drop} / {total}");
    eprintln!("  parse fails  : {}", parse_failures.len());
    eprintln!("  write fails  : {}", write_failures.len());
    eprintln!();
    if !parse_failures.is_empty() {
        eprintln!("Parse failures:");
        for f in &parse_failures {
            eprintln!("  {f}");
        }
        eprintln!();
    }
    if !write_failures.is_empty() {
        eprintln!("Write failures:");
        for f in &write_failures {
            eprintln!("  {f}");
        }
        eprintln!();
    }
    if !dropped_tags_overall.is_empty() {
        eprintln!(
            "Total dropped tags ({} unique):",
            dropped_tags_overall.len()
        );
        for (tag, n) in &dropped_tags_overall {
            eprintln!("  {tag:36} {n}x");
        }
        eprintln!();
        eprintln!("Per-fixture drops:");
        for (name, drops) in &tag_loss_fixtures {
            eprintln!("  {name}");
            for (tag, (in_c, out_c)) in drops {
                eprintln!("    {tag:32} in={in_c} out={out_c}");
            }
        }
    }

    // This is the gate. If `s1-ooxml` is a real preservation layer, this
    // assertion must hold for every fixture.
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
