//! `META-INF/manifest.xml` — the ODF analog of `[Content_Types].xml`.
//!
//! The manifest lists every part in the package along with its media
//! type. Unlike OOXML's content-types, the manifest also lists the
//! root document itself (`/`). The order of entries matters for some
//! consumers; we preserve it on read and write.

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::error::{OdfError, Result};

/// Parsed `META-INF/manifest.xml`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub entries: Vec<ManifestEntry>,
}

/// One `<manifest:file-entry>` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// `manifest:full-path` — package-relative path. `"/"` is the root
    /// document. Directories end with `/`.
    pub full_path: String,
    /// `manifest:media-type` — MIME type. Empty string for directories.
    pub media_type: String,
    /// `manifest:version` — ODF version of the root document. Only
    /// present on the root entry.
    pub version: Option<String>,
}

impl Manifest {
    /// Parse `META-INF/manifest.xml` from its XML bytes.
    ///
    /// Lenient: unknown attributes are kept inside the parts but ignored
    /// here. Only the data needed to round-trip the part list survives.
    pub fn parse(xml: &str) -> Result<Self> {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut entries = Vec::new();
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) | Ok(Event::Empty(e))
                    if local_name_eq(e.name().as_ref(), b"file-entry") =>
                {
                    let mut full_path = String::new();
                    let mut media_type = String::new();
                    let mut version: Option<String> = None;
                    for attr in e.attributes().with_checks(false).flatten() {
                        let local = strip_prefix(attr.key.as_ref());
                        match local {
                            b"full-path" => {
                                full_path = attr
                                    .unescape_value()
                                    .map(|c| c.into_owned())
                                    .unwrap_or_default();
                            }
                            b"media-type" => {
                                media_type = attr
                                    .unescape_value()
                                    .map(|c| c.into_owned())
                                    .unwrap_or_default();
                            }
                            b"version" => {
                                version = Some(
                                    attr.unescape_value()
                                        .map(|c| c.into_owned())
                                        .unwrap_or_default(),
                                );
                            }
                            _ => {}
                        }
                    }
                    if !full_path.is_empty() {
                        entries.push(ManifestEntry {
                            full_path,
                            media_type,
                            version,
                        });
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return Err(OdfError::Xml {
                        part: String::from("META-INF/manifest.xml"),
                        source: e,
                    });
                }
                _ => {}
            }
            buf.clear();
        }
        Ok(Self { entries })
    }

    /// Return the media-type for a given full-path, if listed.
    pub fn media_type_for(&self, full_path: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.full_path == full_path)
            .map(|e| e.media_type.as_str())
    }

    /// Look up just the root document's media type (`manifest:full-path="/"`).
    pub fn root_media_type(&self) -> Option<&str> {
        self.media_type_for("/")
    }
}

fn local_name_eq(name: &[u8], local: &[u8]) -> bool {
    strip_prefix(name) == local
}

fn strip_prefix(name: &[u8]) -> &[u8] {
    match name.iter().position(|b| *b == b':') {
        Some(i) => &name[i + 1..],
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_manifest() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
 <manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="application/vnd.oasis.opendocument.text"/>
 <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
 <manifest:file-entry manifest:full-path="Pictures/" manifest:media-type=""/>
</manifest:manifest>"#;
        let m = Manifest::parse(xml).unwrap();
        assert_eq!(m.entries.len(), 3);
        assert_eq!(
            m.root_media_type(),
            Some("application/vnd.oasis.opendocument.text")
        );
        assert_eq!(m.media_type_for("content.xml"), Some("text/xml"));
        assert_eq!(m.entries[0].version.as_deref(), Some("1.3"));
    }
}
