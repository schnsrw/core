//! `s1-odf` — ODF (OpenDocument Format) packaging + lossless XML
//! preservation layer for Casual Core.
//!
//! Counterpart to `s1-ooxml`. ODF packages (`.odt`, `.ods`, `.odp`) are
//! ZIP archives, but they differ from OOXML in three structural ways:
//!
//! 1. **`mimetype`** must be the *first* entry, stored uncompressed,
//!    with no extra field. A reader can identify the format by reading
//!    bytes 38..n of the archive without unpacking. Re-emitting an ODF
//!    package that violates this rule may produce a file LibreOffice
//!    refuses to open.
//! 2. **No `[Content_Types].xml`.** Part types are listed in
//!    `META-INF/manifest.xml` instead, keyed by media type.
//!    `manifest.rs` parses + writes it.
//! 3. **No centralised relationships file.** ODF cross-references are
//!    embedded in the body XML directly (`xlink:href` etc.).
//!
//! Like `s1-ooxml`, this crate has **zero `s1-model` dependencies** —
//! it operates purely on the OPC tier so the format crates above can
//! bridge to `DocumentModel`.
//!
//! # Example
//!
//! ```ignore
//! use s1_odf::Package;
//!
//! let odt_bytes = std::fs::read("input.odt")?;
//! let package = Package::parse(&odt_bytes)?;
//!
//! // Inspect, project to DocumentModel, edit, …
//!
//! let out_bytes = package.write()?;
//! std::fs::write("output.odt", out_bytes)?;
//! ```

mod error;
mod manifest;
mod package;
mod xml;

pub use error::{OdfError, Result};
pub use manifest::{Manifest, ManifestEntry};
pub use package::{Compression, Package, Part, PartContent, PartName};
pub use xml::{NamespaceDecl, QName, XmlAttribute, XmlDeclaration, XmlElement, XmlNode, XmlTree};
