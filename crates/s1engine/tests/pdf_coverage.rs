//! PDF coverage matrix — quantify what reaches the rendered PDF.
//!
//! For every DOCX + ODT fixture, render through
//! `Document::export(Format::Pdf)` and count the PDF objects the
//! resulting bytes actually contain:
//!
//! - **pages** — the `/Type /Page` count from the page tree.
//! - **text operators** — `BT … Tj/TJ` runs inside content streams
//!   (decompressed). Zero on a fixture with non-trivial body text =
//!   text-rendering path is broken.
//! - **image XObjects** — `/Subtype /Image` entries in the resources.
//!   Zero on a fixture with embedded pictures = image pipeline drops
//!   them.
//! - **font objects** — `/Type /Font` entries, plus how many have
//!   `FontFile2` / `FontFile3` embedded glyph data.
//! - **vector ops** — `re` (rectangle), `m`/`l`/`c` (path), `S`/`B`/`f`
//!   (stroke/fill) counts. Hints whether tables / borders / drawn
//!   shapes are emitting any geometry.
//!
//! Output:
//!
//! - Per-fixture table on stderr (only the columns where input
//!   constructs exist — keeps the report skimmable).
//! - JSON dump to `target/pdf-coverage.json`.
//! - Markdown rollup to `docs/pdf-coverage.md`.
//!
//! This is a reporter, not a gate. It's the user-facing answer to
//! "what reaches the PDF" — the counterpart to the structural
//! `fidelity_score` test on the other side.
//!
//! Run: `cargo test --package s1engine --test pdf_coverage --features pdf -- --nocapture`

#![cfg(feature = "pdf")]

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use lopdf::{Document as PdfDoc, Object};
use s1engine::{Engine, Format};

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

// ─── Source-side counts (so per-fixture report is interpretable) ────────────

fn extract_zip_entry(bytes: &[u8], name: &str) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut entry = zip.by_name(name).ok()?;
    let mut s = String::new();
    entry.read_to_string(&mut s).ok()?;
    Some(s)
}

#[derive(Debug, Default, Clone)]
struct SourceCounts {
    /// `<w:p>` / `<text:p>` / `<text:h>` instances in the body.
    paragraphs: u32,
    /// `<w:drawing>` / `<draw:frame>` / `<draw:image>` instances.
    drawings: u32,
    /// `<w:tbl>` / `<table:table>` instances.
    tables: u32,
    /// `<w:headerReference>` / similar.
    header_refs: u32,
    /// Image binary parts present in the package (`word/media/*`, `Pictures/*`).
    image_parts: u32,
}

fn source_counts(bytes: &[u8], format: Format) -> SourceCounts {
    let body_part = match format {
        Format::Docx => "word/document.xml",
        Format::Odt => "content.xml",
        _ => return SourceCounts::default(),
    };
    let body_xml = match extract_zip_entry(bytes, body_part) {
        Some(s) => s,
        None => return SourceCounts::default(),
    };

    let mut sc = SourceCounts::default();
    // Lightweight regex-free counting — these tag patterns are
    // unambiguous in OOXML / ODF wire format.
    for pat in &[
        "<w:p ",
        "<w:p>",
        "<w:p/>",
        "<text:p ",
        "<text:p>",
        "<text:p/>",
        "<text:h ",
    ] {
        sc.paragraphs += body_xml.matches(pat).count() as u32;
    }
    for pat in &["<w:drawing", "<draw:frame", "<draw:image"] {
        sc.drawings += body_xml.matches(pat).count() as u32;
    }
    for pat in &["<w:tbl ", "<w:tbl>", "<table:table "] {
        sc.tables += body_xml.matches(pat).count() as u32;
    }
    for pat in &["<w:headerReference", "<w:footerReference"] {
        sc.header_refs += body_xml.matches(pat).count() as u32;
    }

    // Count image binary parts in the ZIP.
    if let Ok(mut zip) = zip::ZipArchive::new(Cursor::new(bytes)) {
        for i in 0..zip.len() {
            if let Ok(entry) = zip.by_index(i) {
                let n = entry.name();
                if n.starts_with("word/media/") || n.starts_with("Pictures/") {
                    if n.ends_with(".png")
                        || n.ends_with(".jpg")
                        || n.ends_with(".jpeg")
                        || n.ends_with(".gif")
                        || n.ends_with(".tif")
                        || n.ends_with(".tiff")
                        || n.ends_with(".bmp")
                        || n.ends_with(".svg")
                    {
                        sc.image_parts += 1;
                    }
                }
            }
        }
    }

    sc
}

// ─── PDF inspection ─────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
struct PdfStats {
    bytes: usize,
    pages: u32,
    text_runs: u32,
    show_text_ops: u32,
    image_xobjects: u32,
    font_objects: u32,
    embedded_fonts: u32,
    vector_path_ops: u32,
}

fn inspect_pdf(pdf_bytes: &[u8]) -> Option<PdfStats> {
    let pdf = PdfDoc::load_mem(pdf_bytes).ok()?;
    let mut stats = PdfStats {
        bytes: pdf_bytes.len(),
        ..Default::default()
    };

    stats.pages = pdf.get_pages().len() as u32;

    // Walk every object, categorise.
    for (_id, obj) in pdf.objects.iter() {
        match obj {
            Object::Dictionary(d) => {
                let type_name = d.get(b"Type").ok().and_then(name_of);
                let subtype = d.get(b"Subtype").ok().and_then(name_of);
                match (type_name.as_deref(), subtype.as_deref()) {
                    (Some("Font"), _) => {
                        stats.font_objects += 1;
                        // Look at the descriptor to see if a FontFile is embedded.
                        if let Ok(desc_obj) = d.get(b"FontDescriptor") {
                            if let Object::Reference(rid) = desc_obj {
                                if let Ok(Object::Dictionary(dd)) = pdf.get_object(*rid) {
                                    for k in [
                                        b"FontFile".as_ref(),
                                        b"FontFile2".as_ref(),
                                        b"FontFile3".as_ref(),
                                    ] {
                                        if dd.get(k).is_ok() {
                                            stats.embedded_fonts += 1;
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    (Some("XObject"), Some("Image")) => {
                        stats.image_xobjects += 1;
                    }
                    _ => {}
                }
            }
            Object::Stream(s) => {
                // Streams carry their dict in `.dict` — image XObjects
                // are streams with /Subtype /Image, NOT bare dicts. Need
                // to check the stream's dict before counting.
                let type_name = s.dict.get(b"Type").ok().and_then(name_of);
                let subtype = s.dict.get(b"Subtype").ok().and_then(name_of);
                if matches!(type_name.as_deref(), Some("XObject"))
                    && matches!(subtype.as_deref(), Some("Image"))
                {
                    stats.image_xobjects += 1;
                }
                // Decode and scan for content-stream operators.
                let decoded = s
                    .decompressed_content()
                    .unwrap_or_else(|_| s.content.clone());
                count_content_ops(&decoded, &mut stats);
            }
            _ => {}
        }
    }

    Some(stats)
}

fn name_of(o: &Object) -> Option<String> {
    if let Object::Name(bytes) = o {
        Some(String::from_utf8_lossy(bytes).into_owned())
    } else {
        None
    }
}

fn count_content_ops(stream: &[u8], stats: &mut PdfStats) {
    // We need word-boundary scanning so "BT" doesn't match inside
    // string literals — PDF content streams have a known token shape:
    // operands precede an operator, all whitespace-delimited.
    // For our purposes (rough fidelity counts) a simple boundary scan
    // is sufficient. We walk byte by byte, treating ()<>[]{}/ %, and
    // whitespace as token boundaries.
    let mut i = 0;
    let mut in_string: i32 = 0; // depth of () string literals
    let mut in_hex_string = false;
    let mut token_start: Option<usize> = None;
    while i < stream.len() {
        let b = stream[i];
        // Track when we're inside string literals — operators don't
        // count there.
        if in_hex_string {
            if b == b'>' {
                in_hex_string = false;
            }
            i += 1;
            continue;
        }
        if in_string > 0 {
            match b {
                b'\\' => {
                    i += 2;
                    continue;
                }
                b'(' => in_string += 1,
                b')' => in_string -= 1,
                _ => {}
            }
            i += 1;
            continue;
        }
        if b == b'(' {
            in_string = 1;
            i += 1;
            continue;
        }
        if b == b'<' {
            in_hex_string = true;
            i += 1;
            continue;
        }
        let is_token =
            b.is_ascii_alphanumeric() || b == b'_' || b == b'*' || b == b'\'' || b == b'"';
        if is_token {
            if token_start.is_none() {
                token_start = Some(i);
            }
        } else if let Some(start) = token_start.take() {
            let tok = &stream[start..i];
            classify_op(tok, stats);
        }
        i += 1;
    }
    if let Some(start) = token_start {
        classify_op(&stream[start..], stats);
    }
}

fn classify_op(tok: &[u8], stats: &mut PdfStats) {
    match tok {
        b"BT" => stats.text_runs += 1,
        b"Tj" | b"TJ" | b"'" | b"\"" => stats.show_text_ops += 1,
        b"m" | b"l" | b"c" | b"v" | b"y" | b"re" | b"S" | b"s" | b"f" | b"F" | b"f*" | b"B"
        | b"B*" | b"b" | b"b*" => {
            stats.vector_path_ops += 1;
        }
        _ => {}
    }
}

// ─── Run ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct FixtureReport {
    name: String,
    format: &'static str,
    src: SourceCounts,
    pdf: Option<PdfStats>,
    error: Option<String>,
}

fn run_one(path: &Path, format: Format) -> FixtureReport {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut rep = FixtureReport {
        name,
        format: match format {
            Format::Docx => "docx",
            Format::Odt => "odt",
            _ => "?",
        },
        src: SourceCounts::default(),
        pdf: None,
        error: None,
    };

    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            rep.error = Some(format!("read: {e}"));
            return rep;
        }
    };
    rep.src = source_counts(&bytes, format);

    let engine = Engine::new();
    let doc = match engine.open_as(&bytes, format) {
        Ok(d) => d,
        Err(e) => {
            rep.error = Some(format!("parse: {e}"));
            return rep;
        }
    };
    let pdf_bytes = match doc.export(Format::Pdf) {
        Ok(b) => b,
        Err(e) => {
            rep.error = Some(format!("export pdf: {e}"));
            return rep;
        }
    };
    rep.pdf = inspect_pdf(&pdf_bytes);
    rep
}

fn print_report(label: &str, reports: &[FixtureReport]) {
    eprintln!();
    eprintln!("── {label} (n={}) ──", reports.len());
    eprintln!(
        "  {:<36} {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5}",
        "fixture", "paras", "draws", "tbls", "imgs", "pages", "Tj", "Do", "Fonts", "Vec",
    );
    for r in reports {
        let p = r.pdf.as_ref();
        eprintln!(
            "  {:<36} {:>5} {:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5}",
            truncate(&r.name, 36),
            r.src.paragraphs,
            r.src.drawings,
            r.src.tables,
            r.src.image_parts,
            p.map(|s| s.pages).unwrap_or(0),
            p.map(|s| s.show_text_ops).unwrap_or(0),
            p.map(|s| s.image_xobjects).unwrap_or(0),
            p.map(|s| s.font_objects).unwrap_or(0),
            p.map(|s| s.vector_path_ops).unwrap_or(0),
        );
        if let Some(err) = &r.error {
            eprintln!("    ERROR: {err}");
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_string();
    }
    format!("{}…", &s[..n - 1])
}

/// Aggregate gaps across a set of fixtures — answer "how many fixtures
/// have N input paragraphs / drawings / images but zero PDF text /
/// images / draws".
fn aggregate_gaps(reports: &[FixtureReport]) -> BTreeMap<String, u32> {
    let mut g: BTreeMap<String, u32> = BTreeMap::new();
    for r in reports {
        let pdf = match &r.pdf {
            Some(p) => p,
            None => continue,
        };
        if r.src.paragraphs > 0 && pdf.show_text_ops == 0 {
            *g.entry("text vanishes (input had paragraphs, PDF has no Tj)".into())
                .or_default() += 1;
        }
        if r.src.image_parts > 0 && pdf.image_xobjects == 0 {
            *g.entry("images vanish (input had image parts, PDF has no Image XObjects)".into())
                .or_default() += 1;
        }
        if r.src.drawings > 0 && pdf.image_xobjects == 0 && pdf.vector_path_ops < 4 {
            *g.entry(
                "drawings vanish (input had w:drawing/draw:frame, PDF has no image and no vectors)"
                    .into(),
            )
            .or_default() += 1;
        }
        if r.src.tables > 0 && pdf.vector_path_ops < 4 {
            *g.entry("table borders missing (input had tables, PDF has few vector ops)".into())
                .or_default() += 1;
        }
        if r.src.header_refs > 0 && pdf.show_text_ops == 0 {
            *g.entry("headers/footers vanish".into()).or_default() += 1;
        }
    }
    g
}

fn write_outputs(docx: &[FixtureReport], odt: &[FixtureReport]) {
    use std::fmt::Write;
    let mut md = String::from(
        "# PDF coverage scorecard\n\n\
        Auto-generated by `crates/s1engine/tests/pdf_coverage.rs`. \
        For every fixture, renders through `Document::export(Format::Pdf)` \
        and counts what actually reaches the PDF (pages, text-show ops, \
        image XObjects, embedded fonts, vector path ops). The point is \
        to see *which constructs vanish* when the upstream model had \
        them, so the layout + PDF pipeline gaps can be prioritised.\n\n",
    );

    for (label, reports) in [("DOCX", docx), ("ODT", odt)] {
        let _ = writeln!(md, "## {label} ({} fixtures)\n", reports.len());
        let _ = writeln!(
            md,
            "| Fixture | Paras | Draws | Tables | Imgs | PDF pages | PDF Tj | PDF Imgs | PDF Fonts | PDF Vec |"
        );
        let _ = writeln!(
            md,
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
        );
        for r in reports {
            let p = r.pdf.as_ref();
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                r.name,
                r.src.paragraphs,
                r.src.drawings,
                r.src.tables,
                r.src.image_parts,
                p.map(|s| s.pages).unwrap_or(0),
                p.map(|s| s.show_text_ops).unwrap_or(0),
                p.map(|s| s.image_xobjects).unwrap_or(0),
                p.map(|s| s.font_objects).unwrap_or(0),
                p.map(|s| s.vector_path_ops).unwrap_or(0),
            );
        }
        let _ = writeln!(md);
        let gaps = aggregate_gaps(reports);
        if !gaps.is_empty() {
            let _ = writeln!(md, "**Aggregate gaps ({label}):**\n");
            for (k, n) in &gaps {
                let _ = writeln!(md, "- {n}× — {k}");
            }
            let _ = writeln!(md);
        }
    }

    let path = manifest_root()
        .join("..")
        .join("..")
        .join("docs")
        .join("pdf-coverage.md");
    let _ = fs::write(&path, md);
}

#[test]
fn pdf_coverage_audit() {
    let docx: Vec<FixtureReport> = docx_fixtures()
        .iter()
        .map(|p| run_one(p, Format::Docx))
        .collect();
    let odt: Vec<FixtureReport> = odt_fixtures()
        .iter()
        .map(|p| run_one(p, Format::Odt))
        .collect();

    print_report("DOCX → PDF", &docx);
    print_report("ODT → PDF", &odt);

    eprintln!();
    eprintln!("Aggregate gaps (DOCX):");
    for (k, n) in &aggregate_gaps(&docx) {
        eprintln!("  {n:3}× — {k}");
    }
    eprintln!();
    eprintln!("Aggregate gaps (ODT):");
    for (k, n) in &aggregate_gaps(&odt) {
        eprintln!("  {n:3}× — {k}");
    }

    write_outputs(&docx, &odt);
}
