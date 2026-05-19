//! `_rels/*.rels` files — relationship graph between parts.
//!
//! Each relationship file describes outgoing edges from one source part to
//! other parts (or to external URLs). Like `[Content_Types].xml`, we
//! preserve these verbatim on round-trip — the parsed form is for
//! introspection only.

use crate::xml::XmlTree;

/// Parsed contents of a single `*.rels` file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Relationships {
    pub entries: Vec<Relationship>,
}

/// One `<Relationship>` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    /// Relationship type URI.
    pub rel_type: String,
    /// Target — usually a part path or external URI.
    pub target: String,
    /// `Internal` / `External` mode marker, if present.
    pub target_mode: Option<String>,
}

impl Relationships {
    /// Parse a `.rels` file.
    pub fn parse(xml: &str) -> crate::Result<Self> {
        let tree = XmlTree::parse(xml)
            .map_err(|e| crate::OoxmlError::Malformed(format!("could not parse rels: {e}")))?;

        let mut entries = Vec::new();
        for child in &tree.root.children {
            if let crate::XmlNode::Element(el) = child {
                if el.name.local_name != "Relationship" {
                    continue;
                }
                let mut r = Relationship {
                    id: String::new(),
                    rel_type: String::new(),
                    target: String::new(),
                    target_mode: None,
                };
                for a in &el.attributes {
                    match a.name.local_name.as_str() {
                        "Id" => r.id = a.value.clone(),
                        "Type" => r.rel_type = a.value.clone(),
                        "Target" => r.target = a.value.clone(),
                        "TargetMode" => r.target_mode = Some(a.value.clone()),
                        _ => {}
                    }
                }
                entries.push(r);
            }
        }

        Ok(Relationships { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_rels() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/>
</Relationships>"#;
        let rels = Relationships::parse(xml).unwrap();
        assert_eq!(rels.entries.len(), 2);
        assert_eq!(rels.entries[0].id, "rId1");
        assert_eq!(rels.entries[0].target, "word/document.xml");
        assert_eq!(rels.entries[1].target_mode.as_deref(), Some("External"));
    }
}
