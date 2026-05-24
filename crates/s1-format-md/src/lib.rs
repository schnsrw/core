//! Markdown reader/writer for s1engine.
//!
//! Reads Markdown (CommonMark + GFM extensions) into a [`DocumentModel`] and
//! writes a [`DocumentModel`] back to Markdown text.

mod reader;
mod writer;

pub use reader::read;
pub use writer::write;

use s1_model::{DocumentModel, Node, NodeType};

/// Errors produced by the Markdown format crate.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MdError {
    /// A model insertion error.
    #[error("model error: {0}")]
    Model(String),
}

/// Read Markdown bytes into a [`DocumentModel`].
///
/// The input is interpreted as UTF-8.
///
/// # Errors
///
/// Returns `MdError` if the document model cannot be constructed.
pub fn read_bytes(input: &[u8]) -> Result<DocumentModel, MdError> {
    let text = String::from_utf8_lossy(input);
    read(&text)
}

/// Write a [`DocumentModel`] to Markdown bytes (UTF-8).
pub fn write_bytes(doc: &DocumentModel) -> Vec<u8> {
    write(doc).into_bytes()
}

/// Write a [`DocumentModel`] to a Markdown string.
pub fn write_string(doc: &DocumentModel) -> String {
    write(doc)
}

// ─── Raw passthrough (Format::MdRaw) ────────────────────────────────────────
//
// Bypasses the CommonMark parser entirely so consumers can plug in their own
// Markdown renderer. The whole input lands in a single text node; the writer
// flattens the body's text content back out byte-for-byte. A round-trip
// `MdRaw → DocumentModel → MdRaw` returns identical bytes.

/// Read Markdown bytes as raw text — no CommonMark parsing applied.
///
/// The entire input becomes a single `Paragraph → Run → Text` node so the
/// original bytes can be recovered unchanged by [`write_raw_bytes`].
pub fn read_raw(input: &[u8]) -> Result<DocumentModel, MdError> {
    let text = String::from_utf8_lossy(input);
    let mut doc = DocumentModel::new();
    let body_id = doc
        .body_id()
        .ok_or_else(|| MdError::Model("no body".into()))?;

    let para_id = doc.next_id();
    doc.insert_node(body_id, 0, Node::new(para_id, NodeType::Paragraph))
        .map_err(|e| MdError::Model(e.to_string()))?;
    let run_id = doc.next_id();
    doc.insert_node(para_id, 0, Node::new(run_id, NodeType::Run))
        .map_err(|e| MdError::Model(e.to_string()))?;
    let text_id = doc.next_id();
    doc.insert_node(run_id, 0, Node::text(text_id, text.as_ref()))
        .map_err(|e| MdError::Model(e.to_string()))?;

    Ok(doc)
}

/// Concatenate every text-node's content in document order, without
/// applying any Markdown syntax (headings, list markers, fences, etc.).
/// Pairs with [`read_raw`] to give a byte-faithful passthrough.
pub fn write_raw_string(doc: &DocumentModel) -> String {
    let body_id = match doc.body_id() {
        Some(id) => id,
        None => return String::new(),
    };
    let mut out = String::new();
    collect_text(doc, body_id, &mut out);
    out
}

/// Same as [`write_raw_string`] but returns UTF-8 bytes.
pub fn write_raw_bytes(doc: &DocumentModel) -> Vec<u8> {
    write_raw_string(doc).into_bytes()
}

fn collect_text(doc: &DocumentModel, node_id: s1_model::NodeId, out: &mut String) {
    let Some(node) = doc.node(node_id) else {
        return;
    };
    if node.node_type == NodeType::Text {
        if let Some(t) = &node.text_content {
            out.push_str(t);
        }
        return;
    }
    let children: Vec<s1_model::NodeId> = node.children.clone();
    for child_id in children {
        collect_text(doc, child_id, out);
    }
}

#[cfg(test)]
mod raw_tests {
    use super::*;

    #[test]
    fn read_raw_then_write_raw_is_byte_identical() {
        let src = "# heading\n\nbody\n\n* list **a**\n* list b\n";
        let doc = read_raw(src.as_bytes()).unwrap();
        let out = write_raw_string(&doc);
        assert_eq!(src, out, "raw round-trip must be byte-identical");
    }

    #[test]
    fn read_raw_does_not_parse_headings() {
        let src = "# Not a heading\n";
        let doc = read_raw(src.as_bytes()).unwrap();
        // Body should have exactly one paragraph holding the raw text —
        // no StyleId="Heading1" should be set.
        let body = doc.body_id().unwrap();
        let body_node = doc.node(body).unwrap();
        assert_eq!(body_node.children.len(), 1);
        let para = doc.node(body_node.children[0]).unwrap();
        assert!(
            !para
                .attributes
                .get_string(&s1_model::AttributeKey::StyleId)
                .map(|s| s.starts_with("Heading"))
                .unwrap_or(false),
            "MdRaw must not treat # as a heading"
        );
    }
}
