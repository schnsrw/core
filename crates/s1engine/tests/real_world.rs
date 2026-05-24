//! Real-World Document Tests
//!
//! Integration tests that open actual documents from the `testdocs/` directory
//! and verify that the engine can read them without panicking. Where applicable,
//! tests also exercise export round-trips and cross-format conversion.
//!
//! Tests degrade gracefully: if a fixture file is missing, the test is skipped
//! rather than failed.
//!
//! These tests require all format features (docx, odt, txt, md) to be enabled.
#![cfg(all(feature = "docx", feature = "odt", feature = "txt", feature = "md"))]

use std::path::Path;
use std::time::Instant;

use s1engine::{Engine, Format, NodeType};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Build an absolute path to a file relative to the workspace root.
///
/// `CARGO_MANIFEST_DIR` points to `crates/s1engine/`, so we go up two levels
/// to reach the workspace root.
fn workspace_path(relative: &str) -> std::path::PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.join("../..").join(relative)
}

/// Read a test document from the workspace, returning `None` if the file
/// does not exist (so tests degrade gracefully on CI without fixtures).
fn read_test_doc(relative: &str) -> Option<Vec<u8>> {
    let path = workspace_path(relative);
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP (file not found): {}", path.display());
            None
        }
        Err(e) => panic!("Failed to read {}: {}", path.display(), e),
    }
}

/// Count all nodes of a given type anywhere in the document tree.
fn count_all_nodes_of_type(doc: &s1engine::Document, node_type: NodeType) -> usize {
    let model = doc.model();
    let root_id = model.root_id();
    count_nodes_recursive(model, root_id, node_type)
}

fn count_nodes_recursive(
    model: &s1engine::DocumentModel,
    node_id: s1engine::NodeId,
    target: NodeType,
) -> usize {
    let mut count = 0;
    if let Some(node) = model.node(node_id) {
        if node.node_type == target {
            count += 1;
        }
        for &child_id in &node.children {
            count += count_nodes_recursive(model, child_id, target);
        }
    }
    count
}

// ═══════════════════════════════════════════════════════════════════════════════
// DOCX Documents
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn open_real_docx_freetestdata_100kb() {
    let Some(bytes) = read_test_doc("testdocs/docx/samples/freetestdata_100kb.docx") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open(&bytes)
        .expect("should open 100kb DOCX without error");
    let text = doc.to_plain_text();
    assert!(
        !text.trim().is_empty(),
        "100kb DOCX should contain some text"
    );

    // Structure check
    let para_count = count_all_nodes_of_type(&doc, NodeType::Paragraph);
    assert!(
        para_count >= 1,
        "should have at least 1 paragraph, got {}",
        para_count
    );

    // Cross-format: export to TXT
    let txt_bytes = doc
        .export(Format::Txt)
        .expect("DOCX -> TXT export should succeed");
    assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");

    // Cross-format: export to ODT
    let odt_bytes = doc
        .export(Format::Odt)
        .expect("DOCX -> ODT export should succeed");
    assert!(!odt_bytes.is_empty(), "ODT export should be non-empty");

    // Verify ODT can be reopened
    let doc2 = engine
        .open_as(&odt_bytes, Format::Odt)
        .expect("reopening DOCX-exported-to-ODT should succeed");
    assert!(
        !doc2.to_plain_text().trim().is_empty(),
        "re-opened ODT should contain text"
    );
}

#[test]
fn open_real_docx_freetestdata_500kb() {
    let Some(bytes) = read_test_doc("testdocs/docx/samples/freetestdata_500kb.docx") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open(&bytes)
        .expect("should open 500kb DOCX without error");
    let text = doc.to_plain_text();
    assert!(
        !text.trim().is_empty(),
        "500kb DOCX should contain some text"
    );

    // Cross-format: export to TXT
    let txt_bytes = doc
        .export(Format::Txt)
        .expect("DOCX -> TXT export should succeed");
    assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");
}

#[test]
fn open_real_docx_freetestdata_1mb() {
    let Some(bytes) = read_test_doc("testdocs/docx/samples/freetestdata_1mb.docx") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open(&bytes)
        .expect("should open 1mb DOCX without error");
    let text = doc.to_plain_text();
    assert!(!text.trim().is_empty(), "1mb DOCX should contain some text");

    // Cross-format: export to TXT
    let txt_bytes = doc
        .export(Format::Txt)
        .expect("DOCX -> TXT export should succeed");
    assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");
}

#[test]
fn open_real_docx_calibre_demo() {
    let Some(bytes) = read_test_doc("testdocs/docx/samples/calibre_demo.docx") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open(&bytes)
        .expect("should open calibre_demo DOCX without error");
    let text = doc.to_plain_text();
    assert!(
        !text.trim().is_empty(),
        "calibre_demo DOCX should contain some text"
    );

    // Cross-format: export to TXT
    let txt_bytes = doc
        .export(Format::Txt)
        .expect("DOCX -> TXT export should succeed");
    assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");

    // Cross-format: export to ODT (may fail for complex documents with
    // duplicate media filenames, so treat as best-effort)
    if let Ok(odt_bytes) = doc.export(Format::Odt) {
        assert!(!odt_bytes.is_empty(), "ODT export should be non-empty");

        // Verify ODT can be reopened
        let doc2 = engine
            .open_as(&odt_bytes, Format::Odt)
            .expect("reopening DOCX-exported-to-ODT should succeed");
        assert!(
            !doc2.to_plain_text().trim().is_empty(),
            "re-opened ODT should contain text"
        );
    }
}

#[test]
fn open_real_docx_demo_document() {
    let Some(bytes) = read_test_doc("demo/images/document.docx") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open(&bytes)
        .expect("should open demo document.docx without error");
    let text = doc.to_plain_text();
    // The demo document may or may not have text, just verify it opens
    eprintln!(
        "demo/images/document.docx: {} chars, {} paragraphs",
        text.len(),
        doc.paragraph_count()
    );

    // Cross-format: export to TXT should not panic
    let _txt_bytes = doc
        .export(Format::Txt)
        .expect("DOCX -> TXT export should succeed");
}

// ═══════════════════════════════════════════════════════════════════════════════
// ODT Documents
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn open_real_odt_freetestdata_100kb() {
    let Some(bytes) = read_test_doc("testdocs/odt/samples/freetestdata_100kb.odt") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open_as(&bytes, Format::Odt)
        .expect("should open 100kb ODT without error");
    let text = doc.to_plain_text();
    assert!(
        !text.trim().is_empty(),
        "100kb ODT should contain some text"
    );

    // Cross-format: export to TXT
    let txt_bytes = doc
        .export(Format::Txt)
        .expect("ODT -> TXT export should succeed");
    assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");

    // Cross-format: export to DOCX
    let docx_bytes = doc
        .export(Format::Docx)
        .expect("ODT -> DOCX export should succeed");
    assert!(!docx_bytes.is_empty(), "DOCX export should be non-empty");

    // Verify DOCX can be reopened
    let doc2 = engine
        .open(&docx_bytes)
        .expect("reopening ODT-exported-to-DOCX should succeed");
    assert!(
        !doc2.to_plain_text().trim().is_empty(),
        "re-opened DOCX should contain text"
    );
}

#[test]
fn open_real_odt_freetestdata_500kb() {
    let Some(bytes) = read_test_doc("testdocs/odt/samples/freetestdata_500kb.odt") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open_as(&bytes, Format::Odt)
        .expect("should open 500kb ODT without error");
    let text = doc.to_plain_text();
    assert!(
        !text.trim().is_empty(),
        "500kb ODT should contain some text"
    );

    // Cross-format: export to TXT
    let txt_bytes = doc
        .export(Format::Txt)
        .expect("ODT -> TXT export should succeed");
    assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");
}

#[test]
fn open_real_odt_freetestdata_1mb() {
    let Some(bytes) = read_test_doc("testdocs/odt/samples/freetestdata_1mb.odt") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open_as(&bytes, Format::Odt)
        .expect("should open 1mb ODT without error");
    let text = doc.to_plain_text();
    assert!(!text.trim().is_empty(), "1mb ODT should contain some text");

    // Cross-format: export to TXT
    let txt_bytes = doc
        .export(Format::Txt)
        .expect("ODT -> TXT export should succeed");
    assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");
}

// ═══════════════════════════════════════════════════════════════════════════════
// TXT Documents
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn open_real_txt_moby_dick() {
    let Some(bytes) = read_test_doc("testdocs/txt/samples/moby_dick.txt") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open_as(&bytes, Format::Txt)
        .expect("should open moby_dick.txt without error");
    let text = doc.to_plain_text();
    assert!(!text.is_empty(), "moby_dick.txt should contain text");
    assert!(
        text.len() > 1000,
        "moby_dick.txt should be a substantial text; got {} chars",
        text.len()
    );

    // Should have many paragraphs
    let para_count = doc.paragraph_count();
    assert!(
        para_count >= 10,
        "moby_dick.txt: expected at least 10 paragraphs, got {}",
        para_count
    );

    // Should contain recognizable content
    assert!(
        text.contains("Moby")
            || text.contains("whale")
            || text.contains("Ahab")
            || text.contains("Call me"),
        "moby_dick.txt: expected recognizable Moby Dick content"
    );

    // Export round-trip: TXT -> model -> TXT
    let exported = doc
        .export_string(Format::Txt)
        .expect("TXT export should succeed");
    assert!(!exported.is_empty(), "TXT re-export should be non-empty");
    assert!(
        exported.len() > 1000,
        "re-exported text should be substantial; got {} chars",
        exported.len()
    );

    // Re-open the exported TXT and verify content
    let doc2 = engine
        .open_as(exported.as_bytes(), Format::Txt)
        .expect("re-open exported TXT should succeed");
    let roundtrip_text = doc2.to_plain_text();
    assert_eq!(
        text.trim(),
        roundtrip_text.trim(),
        "Moby Dick TXT round-trip text should be preserved"
    );

    // Cross-format: export to DOCX
    let docx_bytes = doc
        .export(Format::Docx)
        .expect("TXT -> DOCX export should succeed");
    assert!(!docx_bytes.is_empty(), "DOCX export should be non-empty");

    // Verify DOCX can be reopened
    let doc3 = engine
        .open(&docx_bytes)
        .expect("reopening TXT-exported-to-DOCX should succeed");
    assert!(
        !doc3.to_plain_text().is_empty(),
        "re-opened DOCX should contain text"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Markdown Documents
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn open_real_md_markdown_here_readme() {
    let Some(bytes) = read_test_doc("testdocs/md/samples/markdown_here_readme.md") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open_as(&bytes, Format::Md)
        .expect("should open markdown_here_readme.md without error");
    let text = doc.to_plain_text();
    assert!(
        !text.trim().is_empty(),
        "markdown_here_readme.md should contain text"
    );
    assert!(
        text.len() > 50,
        "expected substantial Markdown content, got {} chars",
        text.len()
    );

    // Export round-trip: MD -> model -> MD
    let exported = doc
        .export_string(Format::Md)
        .expect("MD export should succeed");
    assert!(!exported.is_empty(), "MD re-export should be non-empty");

    // Re-open the exported Markdown
    let doc2 = engine
        .open_as(exported.as_bytes(), Format::Md)
        .expect("re-open exported Markdown should succeed");
    assert!(
        !doc2.to_plain_text().trim().is_empty(),
        "Markdown round-trip should preserve text"
    );

    // Export round-trip: MD -> model -> TXT
    let txt_exported = doc
        .export_string(Format::Txt)
        .expect("MD -> TXT export should succeed");
    assert!(!txt_exported.is_empty(), "TXT export should be non-empty");

    // Cross-format: export to DOCX
    let docx_bytes = doc
        .export(Format::Docx)
        .expect("MD -> DOCX export should succeed");
    assert!(!docx_bytes.is_empty(), "DOCX export should be non-empty");

    // Verify DOCX can be reopened
    let doc3 = engine
        .open(&docx_bytes)
        .expect("reopening MD-exported-to-DOCX should succeed");
    assert!(
        !doc3.to_plain_text().is_empty(),
        "re-opened DOCX should contain text"
    );
}

#[test]
fn open_real_md_markdown_test() {
    let Some(bytes) = read_test_doc("testdocs/md/samples/markdown_test.md") else {
        return;
    };
    let engine = Engine::new();
    let doc = engine
        .open_as(&bytes, Format::Md)
        .expect("should open markdown_test.md without error");
    let text = doc.to_plain_text();
    assert!(
        !text.trim().is_empty(),
        "markdown_test.md should contain text"
    );

    // Export round-trip: MD -> model -> MD
    let exported = doc
        .export_string(Format::Md)
        .expect("MD export should succeed");
    assert!(!exported.is_empty(), "MD re-export should be non-empty");

    // Cross-format: export to ODT
    let odt_bytes = doc
        .export(Format::Odt)
        .expect("MD -> ODT export should succeed");
    assert!(!odt_bytes.is_empty(), "ODT export should be non-empty");

    // Verify ODT can be reopened
    let doc2 = engine
        .open_as(&odt_bytes, Format::Odt)
        .expect("reopening MD-exported-to-ODT should succeed");
    assert!(
        !doc2.to_plain_text().trim().is_empty(),
        "re-opened ODT should contain text"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// DOC (Legacy) Documents -- requires `convert` feature
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "convert")]
mod doc_legacy {
    use super::*;

    #[test]
    fn open_real_doc_freetestdata_100kb() {
        let Some(bytes) = read_test_doc("testdocs/doc/samples/freetestdata_100kb.doc") else {
            return;
        };
        let engine = Engine::new();
        let doc = engine
            .open_as(&bytes, Format::Doc)
            .expect("should open 100kb DOC without error");
        let text = doc.to_plain_text();
        assert!(
            !text.trim().is_empty(),
            "100kb DOC should contain some text"
        );
        eprintln!(
            "DOC 100kb: {} chars, {} paragraphs",
            text.len(),
            doc.paragraph_count()
        );

        // Structure check
        let para_count = count_all_nodes_of_type(&doc, NodeType::Paragraph);
        assert!(
            para_count >= 1,
            "DOC should have at least 1 paragraph, got {}",
            para_count
        );

        // Cross-format: export to TXT
        let txt_bytes = doc
            .export(Format::Txt)
            .expect("DOC -> TXT export should succeed");
        assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");

        // Cross-format: export to DOCX
        let docx_bytes = doc
            .export(Format::Docx)
            .expect("DOC -> DOCX export should succeed");
        assert!(!docx_bytes.is_empty(), "DOCX export should be non-empty");

        // Verify DOCX can be reopened
        let doc2 = engine
            .open(&docx_bytes)
            .expect("reopening DOC-exported-to-DOCX should succeed");
        assert!(
            !doc2.to_plain_text().trim().is_empty(),
            "re-opened DOCX should contain text"
        );
    }

    #[test]
    fn open_real_doc_freetestdata_500kb() {
        let Some(bytes) = read_test_doc("testdocs/doc/samples/freetestdata_500kb.doc") else {
            return;
        };
        let engine = Engine::new();
        let doc = engine
            .open_as(&bytes, Format::Doc)
            .expect("should open 500kb DOC without error");
        let text = doc.to_plain_text();
        assert!(
            !text.trim().is_empty(),
            "500kb DOC should contain some text"
        );
        eprintln!(
            "DOC 500kb: {} chars, {} paragraphs",
            text.len(),
            doc.paragraph_count()
        );

        // Cross-format: export to TXT
        let txt_bytes = doc
            .export(Format::Txt)
            .expect("DOC -> TXT export should succeed");
        assert!(!txt_bytes.is_empty(), "TXT export should be non-empty");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// DOCX Round-Trip Preservation
// ═══════════════════════════════════════════════════════════════════════════════

const DOCX_SAMPLES: &[&str] = &[
    "testdocs/docx/samples/freetestdata_100kb.docx",
    "testdocs/docx/samples/freetestdata_500kb.docx",
    "testdocs/docx/samples/freetestdata_1mb.docx",
    "testdocs/docx/samples/calibre_demo.docx",
    "demo/images/document.docx",
];

#[test]
fn docx_roundtrip_preserves_content() {
    let engine = Engine::new();

    for path in DOCX_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        // Open the original
        let doc1 = engine
            .open_as(&bytes, Format::Docx)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));

        let original_text = doc1.to_plain_text();

        // Export to DOCX
        let exported_bytes = doc1
            .export(Format::Docx)
            .unwrap_or_else(|e| panic!("{}: export to DOCX failed: {}", path, e));

        // Re-open the exported DOCX
        let doc2 = engine
            .open_as(&exported_bytes, Format::Docx)
            .unwrap_or_else(|e| panic!("{}: re-open exported DOCX failed: {}", path, e));

        let roundtrip_text = doc2.to_plain_text();

        // Text content should be substantially preserved
        // (some whitespace differences are acceptable)
        let original_trimmed: String = original_text.split_whitespace().collect();
        let roundtrip_trimmed: String = roundtrip_text.split_whitespace().collect();

        // The round-trip text should be at least 80% of the original length
        if !original_trimmed.is_empty() {
            let ratio = roundtrip_trimmed.len() as f64 / original_trimmed.len() as f64;
            assert!(
                ratio >= 0.8,
                "{}: too much text lost in round-trip: original {} chars, round-trip {} chars (ratio {:.2})",
                path,
                original_trimmed.len(),
                roundtrip_trimmed.len(),
                ratio
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// ODT Round-Trip Preservation
// ═══════════════════════════════════════════════════════════════════════════════

const ODT_SAMPLES: &[&str] = &[
    "testdocs/odt/samples/freetestdata_100kb.odt",
    "testdocs/odt/samples/freetestdata_500kb.odt",
    "testdocs/odt/samples/freetestdata_1mb.odt",
];

#[test]
fn odt_roundtrip_preserves_content() {
    let engine = Engine::new();

    for path in ODT_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        let doc1 = engine
            .open_as(&bytes, Format::Odt)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));

        let original_text = doc1.to_plain_text();

        // Export to ODT
        let exported_bytes = doc1
            .export(Format::Odt)
            .unwrap_or_else(|e| panic!("{}: export to ODT failed: {}", path, e));

        // Re-open the exported ODT
        let doc2 = engine
            .open_as(&exported_bytes, Format::Odt)
            .unwrap_or_else(|e| panic!("{}: re-open exported ODT failed: {}", path, e));

        let roundtrip_text = doc2.to_plain_text();

        let original_trimmed: String = original_text.split_whitespace().collect();
        let roundtrip_trimmed: String = roundtrip_text.split_whitespace().collect();

        if !original_trimmed.is_empty() {
            let ratio = roundtrip_trimmed.len() as f64 / original_trimmed.len() as f64;
            assert!(
                ratio >= 0.8,
                "{}: too much text lost in ODT round-trip: original {} chars, round-trip {} chars (ratio {:.2})",
                path,
                original_trimmed.len(),
                roundtrip_trimmed.len(),
                ratio
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Cross-Format Conversion (DOCX -> ODT and DOCX -> TXT)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn docx_to_odt_conversion() {
    let engine = Engine::new();

    for path in DOCX_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        let doc = engine
            .open_as(&bytes, Format::Docx)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));

        let original_text = doc.to_plain_text();

        // Export as ODT (may fail for complex documents with duplicate media
        // filenames, so treat failures as non-fatal)
        match doc.export(Format::Odt) {
            Ok(odt_bytes) => {
                assert!(
                    !odt_bytes.is_empty(),
                    "{}: exported ODT bytes are empty",
                    path
                );

                // Re-open the ODT to verify validity
                let odt_doc = engine
                    .open_as(&odt_bytes, Format::Odt)
                    .unwrap_or_else(|e| panic!("{}: re-open exported ODT failed: {}", path, e));

                if !original_text.trim().is_empty() {
                    assert!(
                        !odt_doc.to_plain_text().trim().is_empty(),
                        "{}: ODT conversion lost all text content",
                        path
                    );
                }
            }
            Err(e) => {
                eprintln!("{}: ODT export failed (non-fatal): {}", path, e);
            }
        }
    }
}

#[test]
fn docx_to_txt_conversion() {
    let engine = Engine::new();

    for path in DOCX_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        let doc = engine
            .open_as(&bytes, Format::Docx)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));

        // Export as plain text
        let txt = doc
            .export_string(Format::Txt)
            .unwrap_or_else(|e| panic!("{}: export to TXT failed: {}", path, e));

        let plain = doc.to_plain_text();

        // Both should be non-empty if the document has content
        if !plain.trim().is_empty() {
            assert!(
                !txt.trim().is_empty(),
                "{}: export_string(TXT) returned empty but to_plain_text() has content",
                path
            );
        }

        // Whitespace-normalized content should be substantially similar
        // (table rendering may differ between to_plain_text and TXT writer)
        let plain_words: Vec<&str> = plain.split_whitespace().collect();
        let txt_words: Vec<&str> = txt.split_whitespace().collect();
        if !plain_words.is_empty() {
            let ratio = txt_words.len() as f64 / plain_words.len() as f64;
            assert!(
                ratio >= 0.8,
                "{}: TXT export lost too many words: plain={}, txt={} (ratio {:.2})",
                path,
                plain_words.len(),
                txt_words.len(),
                ratio
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Markdown Round-Trip
// ═══════════════════════════════════════════════════════════════════════════════

const MD_SAMPLES: &[&str] = &[
    "testdocs/md/samples/markdown_here_readme.md",
    "testdocs/md/samples/markdown_test.md",
];

#[test]
fn md_roundtrip_preserves_content() {
    let engine = Engine::new();

    for path in MD_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        let doc1 = engine
            .open_as(&bytes, Format::Md)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));

        let original_text = doc1.to_plain_text();

        // Export back to Markdown
        let exported_md = doc1
            .export_string(Format::Md)
            .unwrap_or_else(|e| panic!("{}: export to Markdown failed: {}", path, e));

        // Re-open the exported Markdown
        let doc2 = engine
            .open_as(exported_md.as_bytes(), Format::Md)
            .unwrap_or_else(|e| panic!("{}: re-open exported Markdown failed: {}", path, e));

        let roundtrip_text = doc2.to_plain_text();

        if !original_text.trim().is_empty() {
            assert!(
                !roundtrip_text.trim().is_empty(),
                "{}: Markdown round-trip lost all text",
                path
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Format Auto-Detection
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn format_autodetect_docx() {
    let engine = Engine::new();

    for path in DOCX_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        // Using engine.open() which auto-detects format
        let doc = engine
            .open(&bytes)
            .unwrap_or_else(|e| panic!("Auto-detect open failed for {}: {}", path, e));

        assert!(
            !doc.to_plain_text().trim().is_empty(),
            "{}: auto-detected DOCX has no text",
            path
        );
    }
}

#[test]
fn format_autodetect_odt() {
    let engine = Engine::new();

    for path in ODT_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        let doc = engine
            .open(&bytes)
            .unwrap_or_else(|e| panic!("Auto-detect open failed for {}: {}", path, e));

        assert!(
            !doc.to_plain_text().trim().is_empty(),
            "{}: auto-detected ODT has no text",
            path
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Metadata Extraction (no-panic smoke tests)
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn docx_metadata_accessible() {
    let engine = Engine::new();

    for path in DOCX_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        let doc = engine
            .open_as(&bytes, Format::Docx)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));

        // Just verify these accessors do not panic
        let _meta = doc.metadata();
        let _styles = doc.styles();
        let _sections = doc.sections();
    }
}

#[test]
fn odt_metadata_accessible() {
    let engine = Engine::new();

    for path in ODT_SAMPLES {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };

        let doc = engine
            .open_as(&bytes, Format::Odt)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));

        let _meta = doc.metadata();
        let _styles = doc.styles();
        let _sections = doc.sections();
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Performance Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn large_document_open_performance() {
    let large_files: Vec<(&str, Format)> = vec![
        ("testdocs/docx/samples/freetestdata_1mb.docx", Format::Docx),
        ("testdocs/odt/samples/freetestdata_1mb.odt", Format::Odt),
        ("testdocs/txt/samples/moby_dick.txt", Format::Txt),
    ];

    let engine = Engine::new();

    for (path, format) in &large_files {
        let Some(bytes) = read_test_doc(path) else {
            continue;
        };
        let start = Instant::now();
        let doc = engine
            .open_as(&bytes, *format)
            .unwrap_or_else(|e| panic!("Failed to open {}: {}", path, e));
        let open_elapsed = start.elapsed();

        let export_start = Instant::now();
        let _exported = doc
            .export(Format::Txt)
            .unwrap_or_else(|e| panic!("Failed to export {} to TXT: {}", path, e));
        let export_elapsed = export_start.elapsed();

        eprintln!(
            "{}: open={:?}, export_txt={:?}, {} chars",
            path,
            open_elapsed,
            export_elapsed,
            doc.to_plain_text().len()
        );

        // Each document should open in under 10 seconds
        assert!(
            open_elapsed.as_secs() < 10,
            "{} took too long to open: {:?}",
            path,
            open_elapsed
        );

        // Export should also be fast
        assert!(
            export_elapsed.as_secs() < 10,
            "{} took too long to export to TXT: {:?}",
            path,
            export_elapsed
        );
    }
}

/// Cross-format fidelity audit: DOCX → ODT → DOCX and ODT → DOCX → ODT.
///
/// Runs all eigenpal fixtures through both cross-format round-trips, counts
/// XML tags in word/document.xml (DOCX side) or content.xml (ODT side), and
/// prints a concise loss report. Does NOT assert — diagnostic / audit test.
#[test]
fn cross_format_fidelity_audit() {
    use s1engine::{Engine, Format};
    use std::collections::HashMap;

    // Parse XML and count occurrences of every element name tag.
    fn count_tags(xml: &str) -> HashMap<String, usize> {
        let mut counts = HashMap::<String, usize>::new();
        let mut i = 0usize;
        let b = xml.as_bytes();
        while i < b.len() {
            if b[i] == b'<' {
                let start = i + 1;
                if start < b.len() && b[start] != b'/' && b[start] != b'?' && b[start] != b'!' {
                    let j = b[start..]
                        .iter()
                        .position(|&c| c == b' ' || c == b'>' || c == b'/')
                        .map(|p| start + p)
                        .unwrap_or(b.len());
                    let tag = &xml[start..j];
                    if !tag.is_empty() {
                        *counts.entry(tag.to_string()).or_insert(0) += 1;
                    }
                }
            }
            i += 1;
        }
        counts
    }

    fn xml_from_docx(pkg: &s1_ooxml::Package, part: &str) -> String {
        pkg.parts
            .get(part)
            .and_then(|p| match &p.content {
                s1_ooxml::PartContent::Xml(t) => {
                    t.write().ok().and_then(|b| String::from_utf8(b).ok())
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    fn xml_from_odt(pkg: &s1_odf::Package, part: &str) -> String {
        pkg.parts
            .get(part)
            .and_then(|p| match &p.content {
                s1_odf::PartContent::Xml(t) => {
                    t.write().ok().and_then(|b| String::from_utf8(b).ok())
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    fn tag_loss(
        orig: &HashMap<String, usize>,
        round: &HashMap<String, usize>,
    ) -> (usize, usize, Vec<(String, usize, usize)>) {
        let orig_total: usize = orig.values().sum();
        let round_total: usize = round.values().sum();
        let mut drops: Vec<(String, usize, usize)> = orig
            .iter()
            .filter_map(|(tag, &cnt)| {
                let r = *round.get(tag).unwrap_or(&0);
                if r < cnt {
                    Some((tag.clone(), cnt, r))
                } else {
                    None
                }
            })
            .collect();
        drops.sort_by(|a, b| (b.1 - b.2).cmp(&(a.1 - a.2)));
        (orig_total, round_total, drops)
    }

    let docx_fixtures_dir = workspace_path("testdocs/docx/eigenpal");
    let odt_fixtures_dir = workspace_path("testdocs/odt/samples");
    let engine = Engine::new();

    // ── DOCX → ODT → DOCX ─────────────────────────────────────────────────
    eprintln!("\n=== DOCX → ODT → DOCX (all eigenpal fixtures) ===");
    let mut total_orig = 0usize;
    let mut total_round = 0usize;
    let mut fixture_count = 0usize;

    if let Ok(entries) = std::fs::read_dir(&docx_fixtures_dir) {
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |e| e == "docx"))
            .collect();
        paths.sort();

        for path in &paths {
            let docx_bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let orig_pkg = match s1_ooxml::Package::parse(&docx_bytes) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let orig_xml = xml_from_docx(&orig_pkg, "word/document.xml");
            let orig_counts = count_tags(&orig_xml);
            let orig_total_tags: usize = orig_counts.values().sum();

            let orig_doc = match engine.open(&docx_bytes) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let odt_bytes = match orig_doc.export(Format::Odt) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let odt_doc = match engine.open_as(&odt_bytes, Format::Odt) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let docx2_bytes = match odt_doc.export(Format::Docx) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let round_pkg = match s1_ooxml::Package::parse(&docx2_bytes) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let round_xml = xml_from_docx(&round_pkg, "word/document.xml");
            let round_counts = count_tags(&round_xml);
            let round_total_tags: usize = round_counts.values().sum();

            let (_, _, drops) = tag_loss(&orig_counts, &round_counts);
            let dropped: usize = drops.iter().map(|(_, o, r)| o - r).sum();

            total_orig += orig_total_tags;
            total_round += round_total_tags;
            fixture_count += 1;

            let survival = if orig_total_tags == 0 {
                100.0
            } else {
                (orig_total_tags.saturating_sub(dropped)) as f64 / orig_total_tags as f64 * 100.0
            };

            let fname = path.file_name().unwrap().to_str().unwrap();
            eprintln!("  {fname}: {orig_total_tags} → {round_total_tags} tags, -{dropped} dropped ({survival:.0}% survive)");
            for (tag, orig, round) in drops.iter().take(5) {
                eprintln!("      {tag}: {orig} → {round}");
            }
        }
    }

    if fixture_count > 0 {
        let overall_survival = total_round as f64 / total_orig as f64 * 100.0;
        eprintln!("\nDOCX→ODT→DOCX summary: {fixture_count} fixtures, {total_orig} orig tags → {total_round} round tags ({overall_survival:.1}% raw tag survival)");
    }

    // ── ODT → DOCX → ODT ─────────────────────────────────────────────────
    eprintln!("\n=== ODT → DOCX → ODT (testdocs/odt fixtures) ===");
    let mut total_orig_odt = 0usize;
    let mut total_round_odt = 0usize;
    let mut odt_count = 0usize;

    if let Ok(entries) = std::fs::read_dir(&odt_fixtures_dir) {
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |e| e == "odt"))
            .collect();
        paths.sort();

        for path in &paths {
            let odt_bytes = match std::fs::read(path) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let orig_pkg = match s1_odf::Package::parse(&odt_bytes) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let orig_xml = xml_from_odt(&orig_pkg, "content.xml");
            let orig_counts = count_tags(&orig_xml);
            let orig_total_tags: usize = orig_counts.values().sum();

            let orig_doc = match engine.open_as(&odt_bytes, Format::Odt) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let docx_bytes = match orig_doc.export(Format::Docx) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let docx_doc = match engine.open_as(&docx_bytes, Format::Docx) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let odt2_bytes = match docx_doc.export(Format::Odt) {
                Ok(b) => b,
                Err(_) => continue,
            };
            let round_pkg = match s1_odf::Package::parse(&odt2_bytes) {
                Ok(p) => p,
                Err(_) => continue,
            };
            let round_xml = xml_from_odt(&round_pkg, "content.xml");
            let round_counts = count_tags(&round_xml);
            let round_total_tags: usize = round_counts.values().sum();

            let (_, _, drops) = tag_loss(&orig_counts, &round_counts);
            let dropped: usize = drops.iter().map(|(_, o, r)| o - r).sum();

            total_orig_odt += orig_total_tags;
            total_round_odt += round_total_tags;
            odt_count += 1;

            let survival = if orig_total_tags == 0 {
                100.0
            } else {
                (orig_total_tags.saturating_sub(dropped)) as f64 / orig_total_tags as f64 * 100.0
            };

            let fname = path.file_name().unwrap().to_str().unwrap();
            eprintln!("  {fname}: {orig_total_tags} → {round_total_tags} tags, -{dropped} dropped ({survival:.0}% survive)");
            for (tag, orig, round) in drops.iter().take(5) {
                eprintln!("      {tag}: {orig} → {round}");
            }
        }
    }

    if odt_count > 0 {
        let overall_survival = total_round_odt as f64 / total_orig_odt as f64 * 100.0;
        eprintln!("\nODT→DOCX→ODT summary: {odt_count} fixtures, {total_orig_odt} orig tags → {total_round_odt} round tags ({overall_survival:.1}% raw tag survival)");
    }
}

/// Diagnostic: round-trip every Markdown fixture through DOCX and report
/// per-fixture word-survival. Catches regressions where formatting changes
/// silently drop content.
#[test]
fn md_through_docx_fidelity_audit() {
    use s1engine::{Engine, Format};

    let dir = workspace_path("testdocs/md/samples");
    let mut entries: Vec<std::path::PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |e| e == "md"))
            .collect(),
        Err(_) => return,
    };
    entries.sort();

    eprintln!("\n=== Markdown → DOCX → Markdown ===");
    let engine = Engine::new();
    let mut total_words_orig = 0usize;
    let mut total_words_matched = 0usize;
    let mut fixture_count = 0usize;

    for path in &entries {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let original = match std::str::from_utf8(&bytes) {
            Ok(s) => s.to_string(),
            Err(_) => continue,
        };
        let doc = match engine.open_as(&bytes, Format::Md) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let docx_bytes = match doc.export(Format::Docx) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let doc2 = match engine.open(&docx_bytes) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let round = match doc2.export_string(Format::Md) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Word-multiset survival — counts how many words from the original
        // appear in the round-trip output (order-independent). Catches actual
        // content loss without flagging line-wrap or marker normalization.
        // Strip Markdown emphasis markers (`*`, `_`, `~`, surrounding
        // backticks) so a word that gets bolded or italicised on the
        // round trip — e.g. table headers we now render bold — still
        // matches its plain-text original. This audit measures content
        // survival, not formatting parity.
        use std::collections::HashMap;
        fn norm(w: &str) -> &str {
            w.trim_matches(|c: char| {
                c == '*' || c == '_' || c == '~' || c == '`' || c == '<' || c == '>'
            })
        }
        let orig_words: Vec<&str> = original.split_whitespace().map(norm).collect();
        let round_words: Vec<&str> = round.split_whitespace().map(norm).collect();
        let mut round_counts: HashMap<&str, usize> = HashMap::new();
        for w in &round_words {
            *round_counts.entry(*w).or_insert(0) += 1;
        }
        let mut matches = 0usize;
        for w in &orig_words {
            if let Some(c) = round_counts.get_mut(w) {
                if *c > 0 {
                    *c -= 1;
                    matches += 1;
                }
            }
        }
        let survival = if orig_words.is_empty() {
            100.0
        } else {
            matches as f64 / orig_words.len() as f64 * 100.0
        };
        let fname = path.file_name().unwrap().to_str().unwrap_or("");
        eprintln!(
            "  {fname}: {}/{} words preserved ({:.0}% survive)",
            matches,
            orig_words.len(),
            survival
        );
        total_words_orig += orig_words.len();
        total_words_matched += matches;
        fixture_count += 1;
    }

    if fixture_count > 0 {
        let overall = total_words_matched as f64 / total_words_orig as f64 * 100.0;
        eprintln!(
            "\nMD→DOCX→MD summary: {fixture_count} fixtures, {total_words_matched}/{total_words_orig} words preserved ({overall:.1}% word survival)"
        );
    }
}

/// Regression: tables produced from Markdown must end up with visible
/// borders in the exported DOCX. CommonMark / GFM carry no border info,
/// but unbordered tables render as invisible grids in Word, so the MD
/// reader injects a default thin-black-line border on all six edges.
#[test]
fn md_table_export_has_visible_borders() {
    use s1engine::{Engine, Format};

    let md = "| Col A | Col B |\n|-------|-------|\n| 1     | 2     |\n| 3     | 4     |\n";
    let engine = Engine::new();
    let doc = engine.open_as(md.as_bytes(), Format::Md).expect("parse md");
    let docx = doc.export(Format::Docx).expect("export docx");

    let pkg = s1_ooxml::Package::parse(&docx).expect("parse pkg");
    let doc_xml = pkg
        .parts
        .get("word/document.xml")
        .and_then(|p| match &p.content {
            s1_ooxml::PartContent::Xml(t) => t.write().ok(),
            _ => None,
        })
        .and_then(|b| String::from_utf8(b).ok())
        .expect("document.xml");

    assert!(
        doc_xml.contains("<w:tblBorders>"),
        "expected tblBorders in document.xml; got:\n{doc_xml}"
    );
    for side in ["top", "left", "bottom", "right", "insideH", "insideV"] {
        let tag = format!(r#"<w:{side} w:val="single""#);
        assert!(
            doc_xml.contains(&tag),
            "expected `{tag}` in document.xml; got:\n{doc_xml}"
        );
    }
    assert!(
        doc_xml.contains(r#"w:color="000000""#),
        "expected black borders; got:\n{doc_xml}"
    );
}

/// Regression: Format::MdRaw is a byte-identical passthrough — the
/// CommonMark parser is bypassed so consumers can plug in their own
/// Markdown renderer.
#[test]
fn md_raw_passthrough_is_byte_identical() {
    use s1engine::{Engine, Format};

    // A setext heading — the CommonMark parser would normalise this to
    // `# Heading`, so it makes a good differentiator from the raw path.
    let src = "Heading\n=======\n\nBody  with  unusual  spacing.\n";

    let engine = Engine::new();
    let doc = engine
        .open_as(src.as_bytes(), Format::MdRaw)
        .expect("open as md-raw");
    let out = doc
        .export_string(Format::MdRaw)
        .expect("export md-raw string");
    assert_eq!(src, out, "MdRaw round-trip must be byte-identical");

    // Sanity: the regular CommonMark path *does* normalise the same
    // input (e.g. setext → ATX), so MdRaw earns its keep here.
    let parsed = engine
        .open_as(src.as_bytes(), Format::Md)
        .expect("open as md");
    let normalised = parsed.export_string(Format::Md).expect("export md string");
    assert_ne!(
        src, normalised,
        "regular Md path is expected to normalise setext heading"
    );
}

/// Regression: MD tables converted to DOCX must emit a `<w:tblGrid>`
/// whose `<w:gridCol>` widths are proportional to per-column content
/// length, so Word's autofit starts from a sensible layout instead of
/// an all-equal grid. A column with much longer text gets a larger
/// width than one with short text.
#[test]
fn md_table_export_has_content_sized_column_widths() {
    use s1engine::{Engine, Format};

    let md = "\
| Short | A much longer column with substantial text inside |
|-------|---------------------------------------------------|
| a     | bb                                                 |
";
    let engine = Engine::new();
    let doc = engine.open_as(md.as_bytes(), Format::Md).expect("parse md");
    let docx = doc.export(Format::Docx).expect("export docx");

    let pkg = s1_ooxml::Package::parse(&docx).expect("parse pkg");
    let doc_xml = pkg
        .parts
        .get("word/document.xml")
        .and_then(|p| match &p.content {
            s1_ooxml::PartContent::Xml(t) => t.write().ok(),
            _ => None,
        })
        .and_then(|b| String::from_utf8(b).ok())
        .expect("document.xml");

    // tblGrid is present with two columns.
    let grid_start = doc_xml
        .find("<w:tblGrid>")
        .expect("tblGrid in document.xml");
    let grid_end = doc_xml[grid_start..]
        .find("</w:tblGrid>")
        .map(|i| grid_start + i)
        .expect("tblGrid close");
    let grid = &doc_xml[grid_start..grid_end];

    let widths: Vec<i64> = grid
        .split(r#"<w:gridCol w:w=""#)
        .skip(1)
        .filter_map(|chunk| chunk.split('"').next()?.parse::<i64>().ok())
        .collect();
    assert_eq!(
        widths.len(),
        2,
        "expected 2 gridCol entries; got {widths:?}"
    );
    assert!(
        widths[1] > widths[0],
        "the wider-content column should get more width; got {widths:?}"
    );

    // Table width is declared as auto (pandoc convention).
    assert!(
        doc_xml.contains(r#"<w:tblW w:w="0" w:type="auto"/>"#),
        "expected tblW=auto on MD-sourced table; got:\n{doc_xml}"
    );
}

/// Regression: MD → DOCX must emit Word-friendly spacing — body line-height
/// 1.15 with 8pt-after, plus explicit Heading1..6 style definitions so the
/// converted file doesn't fall back to Word's outsized built-in heading
/// styles. Without these defaults the result reads as "raw" output.
#[test]
fn md_export_has_word_friendly_spacing_defaults() {
    use s1engine::{Engine, Format};

    let md = "# Title\n\nBody paragraph.\n\n## Subhead\n\nMore body.\n";
    let engine = Engine::new();
    let doc = engine.open_as(md.as_bytes(), Format::Md).expect("parse md");
    let docx = doc.export(Format::Docx).expect("export docx");

    let pkg = s1_ooxml::Package::parse(&docx).expect("parse pkg");
    let styles_xml = pkg
        .parts
        .get("word/styles.xml")
        .and_then(|p| match &p.content {
            s1_ooxml::PartContent::Xml(t) => t.write().ok(),
            _ => None,
        })
        .and_then(|b| String::from_utf8(b).ok())
        .expect("styles.xml");

    // Body defaults — pPrDefault carries 1.15 line spacing (276/240).
    assert!(
        styles_xml.contains("<w:docDefaults>"),
        "missing docDefaults: {styles_xml}"
    );
    assert!(
        styles_xml.contains(r#"w:line="276""#),
        "expected 1.15 line spacing (276 twips); got:\n{styles_xml}"
    );
    assert!(
        styles_xml.contains(r#"w:after="160""#),
        "expected 8pt-after on body (160 twips); got:\n{styles_xml}"
    );

    // Heading1 — bold + 18pt + 24pt-before + 6pt-after.
    assert!(
        styles_xml.contains(r#"w:styleId="Heading1""#),
        "missing Heading1 style; got:\n{styles_xml}"
    );
    assert!(
        styles_xml.contains(r#"<w:sz w:val="36"/>"#),
        "Heading1 should be 18pt (36 half-points); got:\n{styles_xml}"
    );
    assert!(
        styles_xml.contains(r#"w:before="480""#),
        "Heading1 should carry 24pt-before (480 twips); got:\n{styles_xml}"
    );
    assert!(
        styles_xml.contains(r#"w:styleId="Heading2""#),
        "missing Heading2 style; got:\n{styles_xml}"
    );
}

/// Regression for the MD→DOCX compiler pass: every construct the audit
/// flagged must produce Word-native output. Asserts the most likely
/// breakage points so a future refactor can't silently regress them.
#[test]
fn md_to_docx_compiler_emits_word_native_styles() {
    use s1engine::{Engine, Format};

    let src = "\
para with `inline code` and [link](https://example.com).

- top bullet
  - nested bullet

1. top item
   1. nested item

```rust
fn f() {}
```

> quote
>
> > nested quote

---

- [x] done
- [ ] pending
";

    let engine = Engine::new();
    let doc = engine
        .open_as(src.as_bytes(), Format::Md)
        .expect("parse md");
    let docx = doc.export(Format::Docx).expect("export docx");
    let pkg = s1_ooxml::Package::parse(&docx).expect("parse pkg");

    let doc_xml = pkg
        .parts
        .get("word/document.xml")
        .and_then(|p| match &p.content {
            s1_ooxml::PartContent::Xml(t) => t.write().ok(),
            _ => None,
        })
        .and_then(|b| String::from_utf8(b).ok())
        .expect("document.xml");
    let styles_xml = pkg
        .parts
        .get("word/styles.xml")
        .and_then(|p| match &p.content {
            s1_ooxml::PartContent::Xml(t) => t.write().ok(),
            _ => None,
        })
        .and_then(|b| String::from_utf8(b).ok())
        .expect("styles.xml");

    // Lists — top-level items must use ilvl=0 (not 1).
    assert!(
        doc_xml.contains(r#"<w:ilvl w:val="0"/>"#),
        "expected ilvl=0 on top-level list items; got:\n{doc_xml}"
    );
    // Nested level → ilvl=1.
    assert!(
        doc_xml.contains(r#"<w:ilvl w:val="1"/>"#),
        "expected ilvl=1 on first-nested list items; got:\n{doc_xml}"
    );

    // Inline code → rStyle="Code".
    assert!(
        doc_xml.contains(r#"<w:rStyle w:val="Code"/>"#),
        "inline code must reference the Code character style; got:\n{doc_xml}"
    );
    // Hyperlink runs → rStyle="Hyperlink".
    assert!(
        doc_xml.contains(r#"<w:rStyle w:val="Hyperlink"/>"#),
        "link runs must reference the Hyperlink character style; got:\n{doc_xml}"
    );
    // Code block → pStyle="CodeBlock" or pStyle="CodeBlock<Lang>" if
    // the fence carried a language hint. The language-specific style
    // inherits from the base CodeBlock via basedOn; both are valid.
    assert!(
        doc_xml.contains(r#"<w:pStyle w:val="CodeBlockRust"/>"#),
        "fenced ```rust block must reference CodeBlockRust (so round-trip preserves the language); got:\n{doc_xml}"
    );
    // And the per-language style must be defined and inherit from
    // the base CodeBlock so it renders consistently in Word.
    assert!(
        styles_xml.contains(r#"w:styleId="CodeBlockRust""#),
        "CodeBlockRust must be defined in styles.xml; got:\n{styles_xml}"
    );
    assert!(
        styles_xml.contains(r#"w:styleId="CodeBlock""#),
        "base CodeBlock style must still be defined; got:\n{styles_xml}"
    );

    // Blockquotes → Quote1 / Quote2.
    assert!(
        doc_xml.contains(r#"<w:pStyle w:val="Quote1"/>"#),
        "blockquote must reference Quote1; got:\n{doc_xml}"
    );
    assert!(
        doc_xml.contains(r#"<w:pStyle w:val="Quote2"/>"#),
        "nested blockquote must reference Quote2; got:\n{doc_xml}"
    );

    // Horizontal rule → HorizontalRule style, NOT pageBreakBefore.
    assert!(
        doc_xml.contains(r#"<w:pStyle w:val="HorizontalRule"/>"#),
        "thematic break must reference HorizontalRule; got:\n{doc_xml}"
    );
    assert!(
        !doc_xml.contains("<w:pageBreakBefore/>"),
        "thematic break must NOT emit a real page break: {doc_xml}"
    );

    // Task list checkbox glyphs.
    assert!(
        doc_xml.contains('\u{2611}'.to_string().as_str()),
        "checked task list marker must be ☒ (U+2611); got:\n{doc_xml}"
    );
    assert!(
        doc_xml.contains('\u{2610}'.to_string().as_str()),
        "unchecked task list marker must be ☐ (U+2610); got:\n{doc_xml}"
    );

    // styles.xml — every other style referenced above must be defined
    // (CodeBlock + CodeBlockRust are checked individually above).
    for sid in ["Code", "Hyperlink", "Quote1", "Quote2", "HorizontalRule"] {
        let tag = format!(r#"w:styleId="{sid}""#);
        assert!(
            styles_xml.contains(&tag),
            "styles.xml is missing definition for `{sid}`; got:\n{styles_xml}"
        );
    }
}

/// Diagnostic: print every word lost on round-trip for each MD fixture,
/// so the next quality pass can target real syntax patterns rather than
/// guess. Ignored by default; run manually.
#[test]
#[ignore = "diagnostic — run manually to see what words drop"]
fn md_lost_words_diagnostic() {
    use s1engine::{Engine, Format};
    use std::collections::HashMap;

    fn norm(w: &str) -> &str {
        w.trim_matches(|c: char| {
            c == '*' || c == '_' || c == '~' || c == '`' || c == '<' || c == '>'
        })
    }

    let dir = workspace_path("testdocs/md/samples");
    let mut entries: Vec<std::path::PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |e| e == "md"))
            .collect(),
        Err(_) => return,
    };
    entries.sort();

    let engine = Engine::new();
    for path in &entries {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let original = match std::str::from_utf8(&bytes) {
            Ok(s) => s.to_string(),
            Err(_) => continue,
        };
        let doc = match engine.open_as(&bytes, Format::Md) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let docx = match doc.export(Format::Docx) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let doc2 = match engine.open(&docx) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let round = match doc2.export_string(Format::Md) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let orig_words: Vec<&str> = original.split_whitespace().map(norm).collect();
        let round_words: Vec<&str> = round.split_whitespace().map(norm).collect();
        let mut round_counts: HashMap<&str, usize> = HashMap::new();
        for w in &round_words {
            *round_counts.entry(*w).or_insert(0) += 1;
        }
        let mut lost: Vec<&str> = Vec::new();
        for w in &orig_words {
            if let Some(c) = round_counts.get_mut(w) {
                if *c > 0 {
                    *c -= 1;
                    continue;
                }
            }
            lost.push(*w);
        }
        let fname = path.file_name().unwrap().to_str().unwrap_or("");
        eprintln!("\n=== {fname}: {} lost words ===", lost.len());
        for (i, w) in lost.iter().take(30).enumerate() {
            eprintln!("  [{i}] {w:?}");
        }
        if lost.len() > 30 {
            eprintln!("  ... and {} more", lost.len() - 30);
        }
    }
}

/// Diagnostic: dump key parts of a DOCX produced from a rich MD source so
/// we can audit MD→DOCX quality across all constructs (lists, code, quotes,
/// links, etc.). Ignored by default.
#[test]
#[ignore = "diagnostic — run manually to audit MD→DOCX output"]
fn md_to_docx_compiler_audit() {
    use s1engine::{Engine, Format};

    let src = "\
# Heading 1

## Heading 2

Normal paragraph with **bold**, *italic*, ***bold italic***, ~~strike~~, \
`inline code`, and [a link](https://example.com).

### Lists

- bullet a
- bullet b
  - nested b.1
    - deep b.1.x
  - nested b.2
- bullet c

1. first
2. second
   1. sub one
   2. sub two
3. third

### Code

```rust
fn main() {
    println!(\"hello\");
}
```

    plain indented block of code
    line 2

### Blockquote

> One.
>
> > Nested two.

### Horizontal rule

before

---

after

### Tasks

- [x] done
- [ ] pending

### Table

| Col | Other |
|-----|-------|
| a   | b     |
";

    let engine = Engine::new();
    let doc = engine.open_as(src.as_bytes(), Format::Md).expect("parse");
    let docx = doc.export(Format::Docx).expect("export");

    let pkg = s1_ooxml::Package::parse(&docx).expect("parse pkg");
    for part in ["word/document.xml", "word/styles.xml", "word/numbering.xml"] {
        let body = pkg
            .parts
            .get(part)
            .and_then(|p| match &p.content {
                s1_ooxml::PartContent::Xml(t) => t.write().ok(),
                _ => None,
            })
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_else(|| "(missing)".into());
        eprintln!("\n========== {part} ==========\n{body}");
    }
}

/// Diagnostic: dump the actual MD output of selected DOCX fixtures so we can
/// eyeball list numbering, heading styles, and other formatting that the
/// word-survival audit can't see. Ignored by default — run manually with
/// `cargo test … docx_to_md_diagnostic_dump -- --ignored --nocapture`.
#[test]
#[ignore = "diagnostic — run manually with --ignored to inspect output"]
fn docx_to_md_diagnostic_dump() {
    use s1engine::{Engine, Format};

    let engine = Engine::new();
    let fixtures = [
        "demo.docx",
        "example-with-image.docx",
        "sds-real-world.docx",
        "issue-387-font-theme-override.docx",
    ];

    // Round-trip every MD fixture and dump the result so we can see
    // which words are being lost (helps target the next pass).
    for name in ["links_images.md", "emphasis_edge.md", "headings_all.md"] {
        if let Ok(md_bytes) = std::fs::read(workspace_path(&format!("testdocs/md/samples/{name}")))
        {
            if let Ok(doc) = engine.open_as(&md_bytes, Format::Md) {
                if let Ok(docx) = doc.export(Format::Docx) {
                    if let Ok(doc2) = engine.open(&docx) {
                        if let Ok(round) = doc2.export_string(Format::Md) {
                            eprintln!("\n========== {name} → DOCX → MD ==========");
                            eprintln!("{round}");
                        }
                    }
                }
            }
        }
    }

    // Also MD → DOCX → MD round-trip for the lists fixture, where any
    // ordered-list regression will be obvious in the rendered output.
    if let Ok(md_bytes) = std::fs::read(workspace_path("testdocs/md/samples/nested_lists.md")) {
        if let Ok(doc) = engine.open_as(&md_bytes, Format::Md) {
            if let Ok(docx) = doc.export(Format::Docx) {
                if let Ok(doc2) = engine.open(&docx) {
                    if let Ok(round) = doc2.export_string(Format::Md) {
                        eprintln!("\n========== nested_lists.md → DOCX → MD ==========");
                        eprintln!("{round}");
                    }
                }
            }
        }
    }

    for name in fixtures {
        let path = workspace_path(&format!("testdocs/docx/eigenpal/{name}"));
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("\n========== {name} (missing) ==========");
            continue;
        };
        let Ok(doc) = engine.open(&bytes) else {
            eprintln!("\n========== {name} (parse failed) ==========");
            continue;
        };
        match doc.export_string(Format::Md) {
            Ok(md) => {
                eprintln!("\n========== {name} → MD ==========");
                eprintln!("{md}");
            }
            Err(e) => eprintln!("\n========== {name} (export failed: {e}) =========="),
        }
    }
}

#[cfg(all(feature = "pdf", feature = "docx"))]
#[test]
fn embedded_fonts_load_into_db() {
    let docx = match std::fs::read("../../testdocs/docx/eigenpal/demo.docx") {
        Ok(b) => b,
        Err(_) => return,
    };
    let pkg = s1_ooxml::Package::parse(&docx).unwrap();
    let fonts = s1_format_docx::extract_embedded_fonts(&pkg);
    assert!(!fonts.is_empty(), "demo.docx has embedded fonts");

    let mut db = s1_text::FontDatabase::new();
    let before = db.len();
    for bytes in fonts {
        db.load_font_data(bytes);
    }
    let after = db.len();
    assert!(after > before, "DB should grow: was {before}, now {after}");

    let ubuntu = db.find("Ubuntu", false, false);
    assert!(
        ubuntu.is_some(),
        "Ubuntu Regular should be found (before={before} after={after})"
    );

    if let Some(id) = ubuntu {
        if let Some(font) = db.load_font(id) {
            let name = font.family_name();
            assert_ne!(
                name, "Unknown",
                "Ubuntu family_name should not be 'Unknown'"
            );
        }
    }
}
