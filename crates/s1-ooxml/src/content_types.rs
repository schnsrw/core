//! `[Content_Types].xml` — the OPC content-type map.
//!
//! Maps file extensions (via `<Default>`) and explicit part names (via
//! `<Override>`) to their MIME-style content types. We preserve the file
//! verbatim on round-trip; the parsed form is used to decide whether a
//! given part is XML (and should be parsed losslessly) or binary (and
//! should be kept as raw bytes).

use crate::xml::XmlTree;

const NS_CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

/// Parsed `[Content_Types].xml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentTypes {
    /// `<Default Extension="xml" ContentType="..."/>` entries.
    pub defaults: Vec<DefaultMapping>,
    /// `<Override PartName="/word/document.xml" ContentType="..."/>` entries.
    pub overrides: Vec<OverrideMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultMapping {
    pub extension: String,
    pub content_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideMapping {
    pub part_name: String,
    pub content_type: String,
}

/// A resolved content type for a single part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentType(pub String);

impl ContentType {
    /// Is this a content type that maps to XML?
    ///
    /// Uses the standard MIME suffix rule (`+xml`) plus the two exact
    /// XML content types. We deliberately do **not** match `contains("xml")`
    /// — some binary content types (e.g. obfuscated fonts in some
    /// generators) contain the substring without being XML.
    pub fn is_xml(&self) -> bool {
        let s = &self.0;
        s.ends_with("+xml") || s == "application/xml" || s == "text/xml"
    }
}

impl ContentTypes {
    /// Parse `[Content_Types].xml` content.
    pub fn parse(xml: &str) -> crate::Result<Self> {
        let tree = XmlTree::parse(xml).map_err(|e| {
            crate::OoxmlError::Malformed(format!("could not parse [Content_Types].xml: {e}"))
        })?;
        let _ = NS_CT; // namespace constant kept for reference / future use

        let mut defaults = Vec::new();
        let mut overrides = Vec::new();

        for child in &tree.root.children {
            if let crate::XmlNode::Element(el) = child {
                let local = &el.name.local_name;
                match local.as_str() {
                    "Default" => {
                        let mut ext = String::new();
                        let mut ct = String::new();
                        for a in &el.attributes {
                            match a.name.local_name.as_str() {
                                "Extension" => ext = a.value.clone(),
                                "ContentType" => ct = a.value.clone(),
                                _ => {}
                            }
                        }
                        defaults.push(DefaultMapping {
                            extension: ext,
                            content_type: ct,
                        });
                    }
                    "Override" => {
                        let mut pn = String::new();
                        let mut ct = String::new();
                        for a in &el.attributes {
                            match a.name.local_name.as_str() {
                                "PartName" => pn = a.value.clone(),
                                "ContentType" => ct = a.value.clone(),
                                _ => {}
                            }
                        }
                        overrides.push(OverrideMapping {
                            part_name: pn,
                            content_type: ct,
                        });
                    }
                    _ => {}
                }
            }
        }

        Ok(ContentTypes {
            defaults,
            overrides,
        })
    }

    /// Resolve the content type for a given part path.
    ///
    /// Checks overrides first, then falls back to extension defaults.
    pub fn for_part(&self, part_name: &str) -> Option<ContentType> {
        let normalised = if part_name.starts_with('/') {
            part_name.to_owned()
        } else {
            format!("/{part_name}")
        };
        for o in &self.overrides {
            if o.part_name == normalised {
                return Some(ContentType(o.content_type.clone()));
            }
        }
        if let Some(ext) = part_name.rsplit('.').next() {
            let ext_lower = ext.to_ascii_lowercase();
            for d in &self.defaults {
                if d.extension.eq_ignore_ascii_case(&ext_lower) {
                    return Some(ContentType(d.content_type.clone()));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_and_override() {
        let xml = r#"<?xml version="1.0"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="xml" ContentType="application/xml"/>
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;
        let ct = ContentTypes::parse(xml).unwrap();
        assert_eq!(ct.defaults.len(), 2);
        assert_eq!(ct.overrides.len(), 1);

        let resolved = ct.for_part("word/document.xml").unwrap();
        assert!(resolved.is_xml());
        assert!(resolved.0.contains("wordprocessingml"));

        let rel_ct = ct.for_part("_rels/.rels").unwrap();
        assert!(rel_ct.is_xml());
    }

    #[test]
    fn xml_detection() {
        assert!(ContentType("application/xml".to_owned()).is_xml());
        assert!(ContentType("application/foo+xml".to_owned()).is_xml());
        assert!(!ContentType("image/png".to_owned()).is_xml());
    }
}
