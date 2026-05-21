//! [`BodyOrigin`] for ODT — counterpart of `s1_format_docx::BodyOrigin`.
//!
//! Maps each top-level body NodeId back to the preserved `XmlElement`
//! inside `content.xml`'s `<office:body><office:text>`. Built at parse
//! time. Drives the Phase 2b per-NodeId splice in
//! `s1engine::Document::export(Odt)`: clean NodeIds keep their preserved
//! XML verbatim (including unknown OOXML inside paragraphs / tables —
//! `draw:frame`, `text:span`, `text:s`, `text:soft-page-break`,
//! `svg:title/desc`, …), dirty NodeIds re-render through the writer.

use std::collections::HashMap;

use s1_model::{DocumentModel, NodeId};
use s1_odf::{Package, PartContent, XmlElement, XmlNode};

/// Per-body-child origin table for ODT.
#[derive(Debug, Clone, Default)]
pub struct BodyOrigin {
    /// `(model NodeId → preserved XmlElement)` for every top-level body
    /// child the model exposes.
    pub by_node_id: HashMap<NodeId, XmlElement>,
    /// Model body NodeIds in their preserved-document order.
    pub node_id_order: Vec<NodeId>,
}

impl BodyOrigin {
    /// Build a [`BodyOrigin`] by aligning the model's body children with
    /// the preserved `content.xml` body's block-level children.
    ///
    /// Returns an empty origin when alignment fails — typically because
    /// the document contains `<text:list>` (which projects to multiple
    /// paragraphs in the model but is a single preserved element) or
    /// the block counts otherwise diverge. Callers treat an empty
    /// origin as "no preservation guarantees" and fall back to the
    /// XmlTree-level body-subtree splice from Phase 2a.
    pub fn build(model: &DocumentModel, package: &Package) -> Self {
        let body_id = match model.body_id() {
            Some(id) => id,
            None => return Self::default(),
        };
        let body_node = match model.node(body_id) {
            Some(n) => n,
            None => return Self::default(),
        };
        let model_body_children: Vec<NodeId> = body_node.children.clone();

        let preserved_text = match preserved_office_text(package) {
            Some(el) => el,
            None => return Self::default(),
        };

        let preserved_blocks: Vec<&XmlElement> = preserved_text
            .children
            .iter()
            .filter_map(|child| match child {
                XmlNode::Element(el) if is_block_level(&el.name.local_name) => Some(el),
                _ => None,
            })
            .collect();

        if preserved_blocks.len() != model_body_children.len() {
            return Self::default();
        }

        let mut by_node_id = HashMap::with_capacity(model_body_children.len());
        for (nid, el) in model_body_children.iter().zip(preserved_blocks.iter()) {
            by_node_id.insert(*nid, (*el).clone());
        }

        Self {
            by_node_id,
            node_id_order: model_body_children,
        }
    }
}

/// Resolve `content.xml` → `<office:document-content>` →
/// `<office:body>` → `<office:text>` in the preserved package.
pub fn preserved_office_text(package: &Package) -> Option<&XmlElement> {
    let part = package.parts.get("content.xml")?;
    let tree = match &part.content {
        PartContent::Xml(t) => t,
        _ => return None,
    };
    office_text_in(&tree.root)
}

/// Find the `<office:text>` descendant inside an `<office:document-content>`
/// root element.
pub fn office_text_in(root: &XmlElement) -> Option<&XmlElement> {
    let body = root.children.iter().find_map(|child| match child {
        XmlNode::Element(el) if el.name.local_name == "body" => Some(el),
        _ => None,
    })?;
    body.children.iter().find_map(|child| match child {
        XmlNode::Element(el) if el.name.local_name == "text" => Some(el),
        _ => None,
    })
}

/// Mutable variant of [`office_text_in`].
pub fn office_text_in_mut(root: &mut XmlElement) -> Option<&mut XmlElement> {
    let body = root.children.iter_mut().find_map(|child| match child {
        XmlNode::Element(el) if el.name.local_name == "body" => Some(el),
        _ => None,
    })?;
    body.children.iter_mut().find_map(|child| match child {
        XmlNode::Element(el) if el.name.local_name == "text" => Some(el),
        _ => None,
    })
}

/// `true` for the `<office:text>` children the model projects as a
/// top-level body child 1:1 (`<text:p>`, `<text:h>`, `<table:table>`,
/// `<text:table-of-content>`).
///
/// Notably **excludes** `<text:list>` because it projects to N model
/// children (one per item), breaking position alignment;
/// `<text:tracked-changes>` and `<text:sequence-decls>` because they
/// don't produce model children at all. Documents using any of these
/// fall back to Phase 2a's XmlTree-level body swap, which already
/// preserves them via the surrounding clone.
pub fn is_block_level(local_name: &str) -> bool {
    matches!(local_name, "p" | "h" | "table" | "table-of-content")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_odt(body: &str) -> Vec<u8> {
        use std::io::{Cursor, Write};
        use zip::write::{ExtendedFileOptions, FileOptions, SimpleFileOptions, ZipWriter};
        use zip::CompressionMethod;

        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zip = ZipWriter::new(cursor);
            let mt: FileOptions<'_, ExtendedFileOptions> =
                FileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file("mimetype", mt).unwrap();
            zip.write_all(b"application/vnd.oasis.opendocument.text")
                .unwrap();

            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            zip.start_file("META-INF/manifest.xml", deflated).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">
 <manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>
 <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#).unwrap();

            zip.start_file("content.xml", deflated).unwrap();
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
 <office:body><office:text>{body}</office:text></office:body>
</office:document-content>"#
            );
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn aligns_paragraphs_one_to_one() {
        let bytes =
            build_minimal_odt("<text:p>one</text:p><text:p>two</text:p><text:p>three</text:p>");
        let (model, package) = crate::reader::read_with_package(&bytes).unwrap();
        let origin = BodyOrigin::build(&model, &package);

        assert_eq!(origin.node_id_order.len(), 3);
        for nid in &origin.node_id_order {
            let el = origin.by_node_id.get(nid).expect("missing origin entry");
            assert_eq!(el.name.local_name, "p");
        }
    }

    #[test]
    fn empty_when_list_breaks_alignment() {
        // A <text:list> projects to N model paragraphs but is 1
        // preserved element → counts diverge → origin is empty.
        let bytes = build_minimal_odt(
            r#"<text:p>before</text:p><text:list><text:list-item><text:p>li</text:p></text:list-item></text:list>"#,
        );
        let (model, package) = crate::reader::read_with_package(&bytes).unwrap();
        let origin = BodyOrigin::build(&model, &package);
        assert!(
            origin.node_id_order.is_empty(),
            "expected empty origin for list-bearing body"
        );
    }
}
