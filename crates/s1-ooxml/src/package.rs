//! Package — the top-level OOXML container.
//!
//! Reads a `.docx` / `.xlsx` / `.pptx` byte stream into a typed container
//! that preserves every part (XML or binary). Writes back to bytes via
//! the same code path so the round-trip is lossless.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use crate::content_types::ContentTypes;
use crate::error::{OoxmlError, Result};
use crate::relationships::Relationships;
use crate::xml::XmlTree;

/// Path of a part inside the package (forward-slash separated, no leading
/// slash). Example: `"word/document.xml"`.
pub type PartName = String;

/// A complete OOXML package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// All parts keyed by their package-relative path. `BTreeMap` so write
    /// order is deterministic for byte-equal round trips when possible.
    pub parts: BTreeMap<PartName, Part>,
    /// Parsed `[Content_Types].xml`. Re-emitted on write from the raw part.
    pub content_types: ContentTypes,
    /// Parsed `*.rels` files keyed by the rels file path (e.g.
    /// `"_rels/.rels"` or `"word/_rels/document.xml.rels"`).
    /// Re-emitted on write from the raw parts.
    pub relationships: BTreeMap<PartName, Relationships>,
}

/// One part inside the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub name: PartName,
    pub content: PartContent,
    /// Compression method used in the ZIP — preserved so write doesn't
    /// silently change images from STORE → DEFLATE or vice versa.
    pub compression: Compression,
}

/// What's inside a part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartContent {
    /// XML payload parsed as a lossless tree.
    Xml(XmlTree),
    /// Binary payload preserved verbatim (images, fonts, embedded
    /// objects, …).
    Binary(Vec<u8>),
}

/// ZIP compression for one part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    Stored,
    Deflated,
}

impl From<CompressionMethod> for Compression {
    fn from(m: CompressionMethod) -> Self {
        match m {
            CompressionMethod::Stored => Compression::Stored,
            _ => Compression::Deflated,
        }
    }
}

impl From<Compression> for CompressionMethod {
    fn from(c: Compression) -> Self {
        match c {
            Compression::Stored => CompressionMethod::Stored,
            Compression::Deflated => CompressionMethod::Deflated,
        }
    }
}

impl Package {
    /// Parse a complete OOXML package from bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let reader = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader)?;

        // First pass: read every entry as raw bytes + compression method.
        let mut raw: Vec<(String, Vec<u8>, Compression)> = Vec::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_owned();
            let compression: Compression = entry.compression().into();
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data)?;
            raw.push((name, data, compression));
        }

        // Find and parse `[Content_Types].xml` (case-sensitive per spec).
        let ct_idx = raw
            .iter()
            .position(|(n, _, _)| n == "[Content_Types].xml")
            .ok_or_else(|| OoxmlError::MissingPart("[Content_Types].xml".to_owned()))?;
        let ct_xml = String::from_utf8(raw[ct_idx].1.clone()).map_err(|e| OoxmlError::Utf8 {
            part: "[Content_Types].xml".to_owned(),
            source: e,
        })?;
        let content_types = ContentTypes::parse(&ct_xml)?;

        // Walk the parts; classify each as XML / binary / rels.
        let mut parts = BTreeMap::new();
        let mut relationships = BTreeMap::new();

        for (name, data, compression) in raw {
            if name == "[Content_Types].xml" {
                // Preserve as XML so it re-serialises identically on write.
                let tree = XmlTree::parse(std::str::from_utf8(&data).map_err(|_| {
                    OoxmlError::Malformed("[Content_Types].xml: invalid UTF-8".to_owned())
                })?)
                .map_err(|e| OoxmlError::Xml {
                    part: name.clone(),
                    source: into_quick_err(e),
                })?;
                parts.insert(
                    name.clone(),
                    Part {
                        name: name.clone(),
                        content: PartContent::Xml(tree),
                        compression,
                    },
                );
                continue;
            }

            // `.rels` files are XML and get parsed structurally too.
            if is_rels(&name) {
                let xml_str = String::from_utf8(data.clone()).map_err(|e| OoxmlError::Utf8 {
                    part: name.clone(),
                    source: e,
                })?;
                let rels = Relationships::parse(&xml_str)?;
                relationships.insert(name.clone(), rels);
                // Also keep the raw XML tree so we can write it back losslessly.
                let tree = XmlTree::parse(&xml_str).map_err(|e| OoxmlError::Xml {
                    part: name.clone(),
                    source: into_quick_err(e),
                })?;
                parts.insert(
                    name.clone(),
                    Part {
                        name: name.clone(),
                        content: PartContent::Xml(tree),
                        compression,
                    },
                );
                continue;
            }

            // Otherwise classify via [Content_Types].xml.
            let is_xml = content_types
                .for_part(&name)
                .map(|ct| ct.is_xml())
                .unwrap_or(false);

            if is_xml {
                let xml_str = String::from_utf8(data).map_err(|e| OoxmlError::Utf8 {
                    part: name.clone(),
                    source: e,
                })?;
                let tree = XmlTree::parse(&xml_str).map_err(|e| OoxmlError::Xml {
                    part: name.clone(),
                    source: into_quick_err(e),
                })?;
                parts.insert(
                    name.clone(),
                    Part {
                        name: name.clone(),
                        content: PartContent::Xml(tree),
                        compression,
                    },
                );
            } else {
                parts.insert(
                    name.clone(),
                    Part {
                        name: name.clone(),
                        content: PartContent::Binary(data),
                        compression,
                    },
                );
            }
        }

        Ok(Package {
            parts,
            content_types,
            relationships,
        })
    }

    /// Serialise the package back to bytes.
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zip = ZipWriter::new(cursor);

            // Preserve the original ordering by sorting `[Content_Types].xml`
            // first, then `_rels/.rels`, then the rest alphabetically. This
            // is the canonical OPC order most consumers expect.
            let mut entries: Vec<&Part> = self.parts.values().collect();
            entries.sort_by_key(|p| sort_key(&p.name));

            for part in entries {
                let opts = SimpleFileOptions::default()
                    .compression_method(part.compression.into())
                    .unix_permissions(0o644);
                zip.start_file(&part.name, opts)?;
                match &part.content {
                    PartContent::Xml(tree) => {
                        let bytes = tree.write()?;
                        zip.write_all(&bytes)?;
                    }
                    PartContent::Binary(b) => {
                        zip.write_all(b)?;
                    }
                }
            }

            zip.finish()?;
        }
        Ok(buf)
    }

    /// Number of parts in this package.
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// `true` if the package has no parts.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

fn is_rels(name: &str) -> bool {
    name.ends_with(".rels")
}

fn into_quick_err(e: OoxmlError) -> quick_xml::Error {
    // The XML preservation layer only emits quick-xml errors wrapped in
    // OoxmlError::Xml. Unwrap or fall through to a generic IO error.
    match e {
        OoxmlError::Xml { source, .. } => source,
        OoxmlError::Malformed(msg) => quick_xml::Error::Io(std::sync::Arc::new(
            std::io::Error::new(std::io::ErrorKind::InvalidData, msg),
        )),
        other => quick_xml::Error::Io(std::sync::Arc::new(std::io::Error::other(
            other.to_string(),
        ))),
    }
}

/// Sort key that puts `[Content_Types].xml` first, then `_rels/.rels`, then
/// everything else alphabetically. Mirrors the order most OOXML writers use.
fn sort_key(name: &str) -> (u8, String) {
    if name == "[Content_Types].xml" {
        (0, String::new())
    } else if name == "_rels/.rels" {
        (1, String::new())
    } else if name.ends_with(".rels") {
        (2, name.to_owned())
    } else {
        (3, name.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_docx() -> Vec<u8> {
        // Hand-roll a tiny but valid OOXML package: [Content_Types].xml,
        // _rels/.rels, word/document.xml.
        let mut buf: Vec<u8> = Vec::new();
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

            zip.start_file("word/document.xml", opts).unwrap();
            zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:body><w:p><w:r><w:t>hello</w:t></w:r></w:p></w:body>
</w:document>"#,
            )
            .unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn parse_and_write_minimal() {
        let bytes = build_minimal_docx();
        let pkg = Package::parse(&bytes).unwrap();
        assert_eq!(pkg.parts.len(), 3);
        assert!(pkg.parts.contains_key("[Content_Types].xml"));
        assert!(pkg.parts.contains_key("word/document.xml"));
        assert_eq!(pkg.relationships.len(), 1);

        let out = pkg.write().unwrap();
        // Re-parse and assert structural equality.
        let pkg2 = Package::parse(&out).unwrap();
        assert_eq!(pkg.parts.len(), pkg2.parts.len());
        assert_eq!(pkg.content_types, pkg2.content_types);
        // Parts may differ in subtle ways (whitespace), but their XML trees
        // and binaries should compare equal.
        for (name, part) in &pkg.parts {
            let part2 = pkg2.parts.get(name).unwrap();
            assert_eq!(part.content, part2.content, "part {name} content mismatch");
        }
    }
}
