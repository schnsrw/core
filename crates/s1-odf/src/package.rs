//! Package — the top-level ODF container.
//!
//! Reads a `.odt` / `.ods` / `.odp` byte stream into a typed container
//! that preserves every part (XML or binary). Writes back to bytes via
//! the same code path so the round-trip is lossless.
//!
//! ODF differs from OOXML in three load-bearing ways:
//!
//! 1. **`mimetype` must be the first ZIP entry, STORED (uncompressed),
//!    with no extra-field bytes.** This lets a consumer identify the
//!    package format by reading bytes 38..n of the archive without
//!    unpacking. We honour this on write — `mimetype` goes out first,
//!    Stored, regardless of how it came in.
//! 2. **No `[Content_Types].xml`.** Part media types live in
//!    `META-INF/manifest.xml` (parsed into [`crate::Manifest`]). Parts
//!    whose media type begins with `text/xml`, `application/xml`, or
//!    ends with `+xml` are treated as XML; the rest stay binary.
//! 3. **Stable entry order**: we preserve original-archive ordering for
//!    every entry except `mimetype` (which we always force first). This
//!    keeps `Package::parse(bytes).write()` byte-equal where the ZIP
//!    writer's CRC layer allows.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use zip::write::{ExtendedFileOptions, FileOptions, SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use crate::error::{OdfError, Result};
use crate::manifest::Manifest;
use crate::xml::XmlTree;

/// Package-relative part path. Forward-slash-separated, no leading slash.
pub type PartName = String;

/// A complete ODF package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// All parts keyed by package-relative path. `BTreeMap` so iteration
    /// order is deterministic and writes are predictable.
    pub parts: BTreeMap<PartName, Part>,
    /// Original archive entry order (e.g. `["mimetype", "content.xml",
    /// "styles.xml", ...]`). Preserved on write — `mimetype` is always
    /// forced first regardless of where it appears here, and any new
    /// entries added since open are appended at the end alphabetically.
    pub entry_order: Vec<PartName>,
    /// Parsed `META-INF/manifest.xml`, if present. Re-emitted on write
    /// from the underlying part.
    pub manifest: Option<Manifest>,
    /// `mimetype` payload (e.g. `application/vnd.oasis.opendocument.text`).
    /// Stored separately because it has special write rules — first
    /// entry, no compression, no extra field.
    pub mimetype: Option<String>,
}

/// One part inside the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub name: PartName,
    pub content: PartContent,
    /// ZIP compression method this part used on read. Preserved on write
    /// for the non-mimetype parts.
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

const MIMETYPE: &str = "mimetype";
const MANIFEST_PART: &str = "META-INF/manifest.xml";

impl Package {
    /// Parse a complete ODF package from bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let reader = Cursor::new(bytes);
        let mut archive = zip::ZipArchive::new(reader)?;

        // Pass 1 — read raw bytes + compression for every entry, in
        // archive order. Track order separately for write-time replay.
        let mut raw: Vec<(String, Vec<u8>, Compression)> = Vec::with_capacity(archive.len());
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

        let entry_order: Vec<String> = raw.iter().map(|(n, _, _)| n.clone()).collect();

        // Extract mimetype eagerly so the rest of the loop doesn't have
        // to special-case it.
        let mut mimetype: Option<String> = None;
        if let Some(idx) = raw.iter().position(|(n, _, _)| n == MIMETYPE) {
            let bytes = raw[idx].1.clone();
            mimetype = Some(
                String::from_utf8(bytes)
                    .map_err(|e| OdfError::Utf8 {
                        part: MIMETYPE.to_owned(),
                        source: e,
                    })?
                    .trim()
                    .to_owned(),
            );
        }

        // Parse manifest first so we know how to classify other parts.
        let manifest = if let Some((_, data, _)) = raw.iter().find(|(n, _, _)| n == MANIFEST_PART) {
            let xml_str = std::str::from_utf8(data)
                .map_err(|_| OdfError::Malformed(format!("{MANIFEST_PART}: invalid UTF-8")))?;
            Some(Manifest::parse(xml_str)?)
        } else {
            None
        };

        // Pass 2 — classify each entry as XML or binary, preserve as a Part.
        let mut parts = BTreeMap::new();
        for (name, data, compression) in raw {
            // mimetype is held separately; don't double-store as a Part.
            if name == MIMETYPE {
                continue;
            }
            let media = manifest
                .as_ref()
                .and_then(|m| m.media_type_for(&name))
                .unwrap_or("");
            let is_xml = is_xml_media(media) || looks_like_xml_path(&name);

            // Try XML when the part claims XML. Fall back to Binary on
            // any parse failure (empty `.xml` files like
            // `Configurations2/accelerator/current.xml`, broken stubs in
            // older LibreOffice writes, etc.) — lenient on read so we can
            // still round-trip the package even when one part is junk.
            let part = if is_xml {
                let xml_parsed = std::str::from_utf8(&data)
                    .ok()
                    .and_then(|s| XmlTree::parse(s).ok());
                match xml_parsed {
                    Some(tree) => Part {
                        name: name.clone(),
                        content: PartContent::Xml(tree),
                        compression,
                    },
                    None => Part {
                        name: name.clone(),
                        content: PartContent::Binary(data),
                        compression,
                    },
                }
            } else {
                Part {
                    name: name.clone(),
                    content: PartContent::Binary(data),
                    compression,
                }
            };
            parts.insert(name, part);
        }

        Ok(Package {
            parts,
            entry_order,
            manifest,
            mimetype,
        })
    }

    /// Serialise the package back to bytes.
    ///
    /// Write order:
    /// 1. `mimetype` first, Stored (uncompressed), no extra field.
    /// 2. Every other entry in the recorded original order. Parts
    ///    added since open (not in `entry_order`) trail alphabetically.
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zip = ZipWriter::new(cursor);

            // 1. mimetype — must be first, Stored, no extra field.
            if let Some(mt) = &self.mimetype {
                let opts: FileOptions<'_, ExtendedFileOptions> = FileOptions::default()
                    .compression_method(CompressionMethod::Stored)
                    .unix_permissions(0o644);
                zip.start_file(MIMETYPE, opts)?;
                zip.write_all(mt.as_bytes())?;
            }

            // 2. Walk the recorded order, emitting each existing part.
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            seen.insert(MIMETYPE);
            for name in &self.entry_order {
                if name == MIMETYPE {
                    continue;
                }
                if let Some(part) = self.parts.get(name) {
                    write_part(&mut zip, part)?;
                    seen.insert(name.as_str());
                }
            }

            // 3. Parts added since open — emit alphabetically.
            for (name, part) in &self.parts {
                if seen.contains(name.as_str()) {
                    continue;
                }
                write_part(&mut zip, part)?;
            }

            zip.finish()?;
        }
        Ok(buf)
    }

    /// Number of parts in the package, mimetype excluded.
    pub fn len(&self) -> usize {
        self.parts.len()
    }

    /// `true` if the package has no parts beyond mimetype.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

fn write_part<W: Write + std::io::Seek>(zip: &mut ZipWriter<W>, part: &Part) -> Result<()> {
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
    Ok(())
}

fn is_xml_media(media: &str) -> bool {
    media.starts_with("text/xml") || media.starts_with("application/xml") || media.ends_with("+xml")
}

fn looks_like_xml_path(name: &str) -> bool {
    name.ends_with(".xml") || name.ends_with(".rels")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-roll a tiny but valid ODT package and round-trip it.
    fn build_minimal_odt() -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let cursor = Cursor::new(&mut buf);
            let mut zip = ZipWriter::new(cursor);

            // mimetype — first, Stored.
            let mt: FileOptions<'_, ExtendedFileOptions> =
                FileOptions::default().compression_method(CompressionMethod::Stored);
            zip.start_file(MIMETYPE, mt).unwrap();
            zip.write_all(b"application/vnd.oasis.opendocument.text")
                .unwrap();

            let deflated =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

            // META-INF/manifest.xml
            zip.start_file("META-INF/manifest.xml", deflated).unwrap();
            zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">
 <manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text" manifest:version="1.3"/>
 <manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#).unwrap();

            // content.xml
            zip.start_file("content.xml", deflated).unwrap();
            zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0">
 <office:body>
  <office:text>
   <text:p>hello</text:p>
  </office:text>
 </office:body>
</office:document-content>"#,
            )
            .unwrap();

            zip.finish().unwrap();
        }
        buf
    }

    #[test]
    fn parse_and_write_minimal_odt() {
        let bytes = build_minimal_odt();
        let pkg = Package::parse(&bytes).unwrap();
        assert_eq!(
            pkg.mimetype.as_deref(),
            Some("application/vnd.oasis.opendocument.text")
        );
        assert_eq!(pkg.parts.len(), 2);
        assert!(pkg.parts.contains_key("content.xml"));
        assert!(pkg.parts.contains_key("META-INF/manifest.xml"));
        assert!(pkg.manifest.is_some());

        let out = pkg.write().unwrap();
        let pkg2 = Package::parse(&out).unwrap();
        assert_eq!(pkg.mimetype, pkg2.mimetype);
        assert_eq!(pkg.parts.len(), pkg2.parts.len());
        // Mimetype must remain the first entry on rewrite.
        let mut archive = zip::ZipArchive::new(Cursor::new(&out)).unwrap();
        let first = archive.by_index(0).unwrap();
        assert_eq!(first.name(), MIMETYPE);
        assert_eq!(first.compression(), CompressionMethod::Stored);
    }
}
