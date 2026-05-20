//! [`BodyOrigin`] — side-table mapping model body NodeIds back to their
//! preserved `XmlElement` from `word/document.xml`.
//!
//! Built at parse time by walking the preserved package body alongside the
//! model body in document order. Used by [`s1engine::Document::export`]
//! (DOCX) to keep untouched body elements byte-for-byte verbatim — including
//! any unknown child elements the model can't project — while only
//! regenerating dirty nodes.
//!
//! Layering: this type lives in `s1-format-docx` because it bridges
//! `s1-model` (`NodeId`) and `s1-ooxml` (`XmlElement`). Putting it inside
//! `s1-ooxml::Package` would require `s1-ooxml → s1-model`, which violates
//! the workspace's zero-dependency rule on the OOXML packaging tier.

use std::collections::HashMap;

use s1_model::{DocumentModel, NodeId};
use s1_ooxml::{Package, PartContent, XmlElement, XmlNode};

/// Per-body-child origin table.
///
/// `node_id_order` records the model NodeIds in the order they appeared in
/// the preserved body. `by_node_id` looks up the preserved `XmlElement`
/// for each. The fast path in `export_docx_spliced` walks model body
/// children, and for any NodeId not in the dirty set it splices the
/// preserved element back verbatim.
#[derive(Debug, Clone, Default)]
pub struct BodyOrigin {
    /// `(model NodeId → preserved XmlElement)` for every top-level body
    /// child the model exposes.
    pub by_node_id: HashMap<NodeId, XmlElement>,
    /// Model body NodeIds in their preserved-document order. Used to
    /// detect structural changes (insert / delete / reorder).
    pub node_id_order: Vec<NodeId>,
}

impl BodyOrigin {
    /// Build a [`BodyOrigin`] by aligning the model's body children with
    /// the preserved `word/document.xml` body's block-level children.
    ///
    /// Returns an empty origin (no NodeIds) if alignment fails — the
    /// preserved body has a different number of block-level children than
    /// the model body, or the package has no parsed `word/document.xml`.
    /// Callers should treat an empty origin as "no preservation
    /// guarantees" and fall back to wholesale regeneration on edit.
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

        let preserved_body = match preserved_body_element(package) {
            Some(el) => el,
            None => return Self::default(),
        };

        let preserved_blocks: Vec<&XmlElement> = preserved_body
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

/// Resolve `word/document.xml`'s root → `<w:body>` element in the preserved
/// package, if present.
pub fn preserved_body_element(package: &Package) -> Option<&XmlElement> {
    let part = package.parts.get("word/document.xml")?;
    let tree = match &part.content {
        PartContent::Xml(t) => t,
        _ => return None,
    };
    // Document root is `<w:document>`; its first `<w:body>` child holds
    // the block-level content we want to align against.
    body_in(&tree.root)
}

/// Find the `<w:body>` child of a `<w:document>` root element.
pub fn body_in(root: &XmlElement) -> Option<&XmlElement> {
    root.children.iter().find_map(|child| match child {
        XmlNode::Element(el) if el.name.local_name == "body" => Some(el),
        _ => None,
    })
}

/// Find the `<w:body>` child of a `<w:document>` root element, mutably.
pub fn body_in_mut(root: &mut XmlElement) -> Option<&mut XmlElement> {
    root.children.iter_mut().find_map(|child| match child {
        XmlNode::Element(el) if el.name.local_name == "body" => Some(el),
        _ => None,
    })
}

/// `true` for the OOXML body-level elements the model projects as a
/// top-level body child (`<w:p>`, `<w:tbl>`, top-level `<w:sdt>`).
///
/// Everything else in the preserved body — `<w:sectPr>` (the final
/// section), non-TOC `<w:sdt>` blocks, range markers — does *not*
/// correspond to a model body child and is preserved by sitting in place
/// during the splice.
pub fn is_block_level(local_name: &str) -> bool {
    matches!(local_name, "p" | "tbl" | "sdt")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_docx_with_paragraphs(paragraphs: &[&str]) -> Vec<u8> {
        use std::io::{Cursor, Write};
        use zip::write::{SimpleFileOptions, ZipWriter};
        use zip::CompressionMethod;

        let mut buf = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zip = ZipWriter::new(cursor);
            let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

            zip.start_file("[Content_Types].xml", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#).unwrap();

            zip.start_file("_rels/.rels", opts).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#).unwrap();

            let body_paras: String = paragraphs
                .iter()
                .map(|t| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>"))
                .collect();
            let doc = format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body>{body_paras}</w:body>
</w:document>"#
            );
            zip.start_file("word/document.xml", opts).unwrap();
            zip.write_all(doc.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn aligns_paragraphs_one_to_one() {
        let docx = build_docx_with_paragraphs(&["one", "two", "three"]);
        let (model, package) = crate::reader::read_with_package(&docx).unwrap();
        let origin = BodyOrigin::build(&model, &package);

        assert_eq!(origin.node_id_order.len(), 3);
        for nid in &origin.node_id_order {
            let el = origin.by_node_id.get(nid).expect("origin entry missing");
            assert_eq!(el.name.local_name, "p");
        }
    }

    #[test]
    fn empty_when_body_counts_mismatch() {
        // Hand-craft a package whose model body and preserved body don't
        // line up — easiest is to drop preserved body's block children.
        let docx = build_docx_with_paragraphs(&["solo"]);
        let (mut model, package) = crate::reader::read_with_package(&docx).unwrap();
        // Forcibly add another paragraph to the model body so the counts
        // diverge from the preserved body's one paragraph.
        let body_id = model.body_id().unwrap();
        let extra = model.next_id();
        model
            .insert_node(
                body_id,
                1,
                s1_model::Node::new(extra, s1_model::NodeType::Paragraph),
            )
            .unwrap();

        let origin = BodyOrigin::build(&model, &package);
        assert!(origin.node_id_order.is_empty());
        assert!(origin.by_node_id.is_empty());
    }
}
