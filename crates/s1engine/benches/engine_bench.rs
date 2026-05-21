//! Performance benchmarks for s1engine core operations.
//!
//! Run with: cargo bench -p s1engine

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use s1engine::{DocumentBuilder, Engine, Format};

// ── Helpers ────────────────────────────────────────────────────────────

fn build_small_doc() -> s1engine::Document {
    DocumentBuilder::new()
        .title("Benchmark Doc")
        .heading(1, "Introduction")
        .text("This is a test paragraph with some content.")
        .paragraph(|p| {
            p.text("Normal ")
                .bold("bold")
                .text(" and ")
                .italic("italic")
        })
        .build()
}

fn build_medium_doc() -> s1engine::Document {
    let mut builder = DocumentBuilder::new().title("Medium Document");
    for i in 0..50 {
        builder = builder
            .heading(2, &format!("Section {}", i + 1))
            .text(&format!(
                "Paragraph {} with enough content to be realistic. \
                 Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
                i + 1
            ));
    }
    builder.build()
}

fn build_large_doc() -> s1engine::Document {
    let mut builder = DocumentBuilder::new().title("Large Document (~100 pages)");
    for i in 0..500 {
        builder = builder
            .heading(2, &format!("Section {}", i + 1))
            .text(&format!(
                "Paragraph {} of the large benchmark document. \
                 Lorem ipsum dolor sit amet, consectetur adipiscing elit, \
                 sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
                 Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.",
                i + 1
            ));
    }
    builder.build()
}

/// Roughly 500 pages of body content (2500 sections × heading + paragraph
/// of ~400 chars). The roadmap's `v0.3.x` perf target benches DOCX → PDF
/// against this scale; bumping the section count keeps the layout +
/// shaping pipeline under realistic stress without inflating individual
/// paragraph weight.
#[cfg(feature = "pdf")]
fn build_huge_doc() -> s1engine::Document {
    let mut builder = DocumentBuilder::new().title("Huge Document (~500 pages)");
    for i in 0..2500 {
        builder = builder
            .heading(2, &format!("Section {}", i + 1))
            .text(&format!(
                "Paragraph {} of the 500-page benchmark document. \
                 Lorem ipsum dolor sit amet, consectetur adipiscing elit, \
                 sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
                 Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.",
                i + 1
            ));
    }
    builder.build()
}

fn build_table_doc() -> s1engine::Document {
    DocumentBuilder::new()
        .heading(1, "Report")
        .table(|t| {
            let mut t = t.row(|r| r.cell("Name").cell("Value").cell("Status"));
            for i in 0..20 {
                let name = format!("Item {}", i + 1);
                let value = format!("{}", (i + 1) * 100);
                t = t.row(move |r| r.cell(&name).cell(&value).cell("OK"));
            }
            t
        })
        .build()
}

// ── Benchmarks ─────────────────────────────────────────────────────────

fn bench_create_empty(c: &mut Criterion) {
    let engine = Engine::new();
    c.bench_function("create_empty_document", |b| {
        b.iter(|| {
            let doc = engine.create();
            black_box(doc);
        });
    });
}

fn bench_builder_small(c: &mut Criterion) {
    c.bench_function("builder_small_doc", |b| {
        b.iter(|| {
            let doc = build_small_doc();
            black_box(doc);
        });
    });
}

fn bench_builder_medium(c: &mut Criterion) {
    c.bench_function("builder_medium_50_sections", |b| {
        b.iter(|| {
            let doc = build_medium_doc();
            black_box(doc);
        });
    });
}

fn bench_builder_table(c: &mut Criterion) {
    c.bench_function("builder_table_20_rows", |b| {
        b.iter(|| {
            let doc = build_table_doc();
            black_box(doc);
        });
    });
}

fn bench_to_plain_text(c: &mut Criterion) {
    let doc = build_medium_doc();
    c.bench_function("to_plain_text_50_sections", |b| {
        b.iter(|| {
            let text = doc.to_plain_text();
            black_box(text);
        });
    });
}

fn bench_export_docx_small(c: &mut Criterion) {
    let doc = build_small_doc();
    c.bench_function("export_docx_small", |b| {
        b.iter(|| {
            let bytes = doc.export(Format::Docx).unwrap();
            black_box(bytes);
        });
    });
}

fn bench_export_docx_medium(c: &mut Criterion) {
    let doc = build_medium_doc();
    c.bench_function("export_docx_50_sections", |b| {
        b.iter(|| {
            let bytes = doc.export(Format::Docx).unwrap();
            black_box(bytes);
        });
    });
}

fn bench_export_odt_small(c: &mut Criterion) {
    let doc = build_small_doc();
    c.bench_function("export_odt_small", |b| {
        b.iter(|| {
            let bytes = doc.export(Format::Odt).unwrap();
            black_box(bytes);
        });
    });
}

fn bench_export_txt(c: &mut Criterion) {
    let doc = build_medium_doc();
    c.bench_function("export_txt_50_sections", |b| {
        b.iter(|| {
            let bytes = doc.export(Format::Txt).unwrap();
            black_box(bytes);
        });
    });
}

fn bench_open_docx(c: &mut Criterion) {
    let doc = build_small_doc();
    let bytes = doc.export(Format::Docx).unwrap();
    let engine = Engine::new();

    c.bench_function("open_docx_small", |b| {
        b.iter(|| {
            let doc = engine.open(black_box(&bytes)).unwrap();
            black_box(doc);
        });
    });
}

fn bench_open_docx_medium(c: &mut Criterion) {
    let doc = build_medium_doc();
    let bytes = doc.export(Format::Docx).unwrap();
    let engine = Engine::new();

    c.bench_function("open_docx_50_sections", |b| {
        b.iter(|| {
            let doc = engine.open(black_box(&bytes)).unwrap();
            black_box(doc);
        });
    });
}

fn bench_open_odt(c: &mut Criterion) {
    let doc = build_small_doc();
    let bytes = doc.export(Format::Odt).unwrap();
    let engine = Engine::new();

    c.bench_function("open_odt_small", |b| {
        b.iter(|| {
            let doc = engine.open(black_box(&bytes)).unwrap();
            black_box(doc);
        });
    });
}

fn bench_roundtrip_docx(c: &mut Criterion) {
    let doc = build_small_doc();
    let bytes = doc.export(Format::Docx).unwrap();
    let engine = Engine::new();

    c.bench_function("roundtrip_docx_small", |b| {
        b.iter(|| {
            let doc = engine.open(black_box(&bytes)).unwrap();
            let out = doc.export(Format::Docx).unwrap();
            black_box(out);
        });
    });
}

fn bench_undo_redo(c: &mut Criterion) {
    use s1_ops::Operation;

    c.bench_function("undo_redo_10_ops", |b| {
        b.iter(|| {
            let mut doc = build_small_doc();
            let para_ids = doc.paragraph_ids();
            if para_ids.is_empty() {
                return;
            }

            // Find a text node to edit
            let para = doc.node(para_ids[0]).unwrap();
            if para.children.is_empty() {
                return;
            }
            let run_id = para.children[0];
            let run = doc.node(run_id).unwrap();
            if run.children.is_empty() {
                return;
            }
            let text_id = run.children[0];

            // Apply 10 insert operations
            for i in 0..10 {
                let op = Operation::InsertText {
                    target_id: text_id,
                    offset: i,
                    text: "x".to_string(),
                };
                doc.apply(op).unwrap();
            }

            // Undo all 10
            for _ in 0..10 {
                doc.undo().unwrap();
            }

            // Redo all 10
            for _ in 0..10 {
                doc.redo().unwrap();
            }

            black_box(&doc);
        });
    });
}

fn bench_format_detection(c: &mut Criterion) {
    let doc = build_small_doc();
    let docx_bytes = doc.export(Format::Docx).unwrap();
    let odt_bytes = doc.export(Format::Odt).unwrap();
    let txt_bytes = doc.export(Format::Txt).unwrap();

    c.bench_function("format_detection", |b| {
        b.iter(|| {
            black_box(Format::detect(&docx_bytes));
            black_box(Format::detect(&odt_bytes));
            black_box(Format::detect(&txt_bytes));
        });
    });
}

fn bench_builder_large_100_pages(c: &mut Criterion) {
    c.bench_function("builder_large_500_paragraphs", |b| {
        b.iter(|| {
            let doc = build_large_doc();
            black_box(doc);
        });
    });
}

#[cfg(feature = "docx")]
fn bench_export_docx_large(c: &mut Criterion) {
    let doc = build_large_doc();
    c.bench_function("export_docx_large_500_paragraphs", |b| {
        b.iter(|| {
            let bytes = doc.export(Format::Docx).unwrap();
            black_box(bytes);
        });
    });
}

#[cfg(feature = "docx")]
fn bench_open_docx_large(c: &mut Criterion) {
    let doc = build_large_doc();
    let bytes = doc.export(Format::Docx).unwrap();
    let engine = Engine::new();

    c.bench_function("open_docx_large_500_paragraphs", |b| {
        b.iter(|| {
            let doc = engine.open(black_box(&bytes)).unwrap();
            black_box(doc);
        });
    });
}

/// DOCX → PDF conversion at roadmap-scale (~500 pages of body content).
/// This is the `v0.3.x` published-number bench. Sample size is tightened
/// because each iteration runs the full layout + shaping + PDF pipeline.
///
/// Run: `cargo bench -p s1engine --features pdf -- pdf_500_pages`
#[cfg(feature = "pdf")]
fn bench_docx_to_pdf_huge(c: &mut Criterion) {
    let doc = build_huge_doc();
    let font_db = s1_text::FontDatabase::empty();
    let mut group = c.benchmark_group("pdf_500_pages");
    group.sample_size(10);
    group.bench_function("docx_to_pdf_500_pages", |b| {
        b.iter(|| {
            let bytes = doc.export_pdf(&font_db).unwrap();
            black_box(bytes);
        });
    });
    group.finish();
}

#[cfg(feature = "odt")]
fn bench_roundtrip_odt_small(c: &mut Criterion) {
    let doc = DocumentBuilder::new()
        .title("Small ODT Roundtrip")
        .heading(1, "Introduction")
        .text("First paragraph with some content.")
        .text("Second paragraph for the roundtrip benchmark.")
        .paragraph(|p| p.text("Mixed ").bold("bold").text(" and ").italic("italic"))
        .text("Final paragraph to round things out.")
        .build();
    let bytes = doc.export(Format::Odt).unwrap();
    let engine = Engine::new();

    c.bench_function("roundtrip_odt_small_5_paragraphs", |b| {
        b.iter(|| {
            let doc = engine.open(black_box(&bytes)).unwrap();
            let out = doc.export(Format::Odt).unwrap();
            black_box(out);
        });
    });
}

fn bench_to_plain_text_large(c: &mut Criterion) {
    let doc = build_large_doc();
    c.bench_function("to_plain_text_large_500_paragraphs", |b| {
        b.iter(|| {
            let text = doc.to_plain_text();
            black_box(text);
        });
    });
}

criterion_group!(
    benches,
    bench_create_empty,
    bench_builder_small,
    bench_builder_medium,
    bench_builder_table,
    bench_to_plain_text,
    bench_export_docx_small,
    bench_export_docx_medium,
    bench_export_odt_small,
    bench_export_txt,
    bench_open_docx,
    bench_open_docx_medium,
    bench_open_odt,
    bench_roundtrip_docx,
    bench_undo_redo,
    bench_format_detection,
    bench_builder_large_100_pages,
    bench_to_plain_text_large,
);

#[cfg(feature = "docx")]
criterion_group!(
    benches_docx_large,
    bench_export_docx_large,
    bench_open_docx_large,
);

#[cfg(feature = "odt")]
criterion_group!(benches_odt, bench_roundtrip_odt_small,);

#[cfg(feature = "pdf")]
criterion_group!(benches_pdf, bench_docx_to_pdf_huge,);

#[cfg(all(feature = "docx", feature = "odt", feature = "pdf"))]
criterion_main!(benches, benches_docx_large, benches_odt, benches_pdf);

#[cfg(all(feature = "docx", feature = "odt", not(feature = "pdf")))]
criterion_main!(benches, benches_docx_large, benches_odt);

#[cfg(all(feature = "docx", not(feature = "odt"), feature = "pdf"))]
criterion_main!(benches, benches_docx_large, benches_pdf);

#[cfg(all(feature = "docx", not(feature = "odt"), not(feature = "pdf")))]
criterion_main!(benches, benches_docx_large);

#[cfg(all(not(feature = "docx"), feature = "odt", feature = "pdf"))]
criterion_main!(benches, benches_odt, benches_pdf);

#[cfg(all(not(feature = "docx"), feature = "odt", not(feature = "pdf")))]
criterion_main!(benches, benches_odt);

#[cfg(all(not(feature = "docx"), not(feature = "odt"), feature = "pdf"))]
criterion_main!(benches, benches_pdf);

#[cfg(all(not(feature = "docx"), not(feature = "odt"), not(feature = "pdf")))]
criterion_main!(benches);
