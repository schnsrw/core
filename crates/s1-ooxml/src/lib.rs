//! `s1-ooxml` — OOXML packaging + lossless XML preservation layer for Casual Core.
//!
//! See [`docs/ooxml-design.md`](../../../docs/ooxml-design.md) for the rationale
//! and roadmap. The short version: this crate parses an OOXML package
//! (`.docx` / `.xlsx` / `.pptx`) into a tree that preserves **every** element,
//! attribute, namespace, text run, and binary blob, then can write it back out.
//! Unknown content rides through untouched, which is what makes
//! `s1-format-docx` able to achieve eigenpal-class fidelity without
//! hand-mapping every OOXML tag into `s1-model`.
//!
//! This crate is **format-agnostic at the OOXML tier**. It does not know what
//! `w:p` means. WordprocessingML interpretation lives in `s1-format-docx`.
//!
//! # Example
//!
//! ```ignore
//! use s1_ooxml::Package;
//!
//! let docx_bytes = std::fs::read("input.docx")?;
//! let package = Package::parse(&docx_bytes)?;
//!
//! // Inspect, projection, edits …
//!
//! let out_bytes = package.write()?;
//! std::fs::write("output.docx", out_bytes)?;
//! ```

mod content_types;
mod error;
mod package;
mod relationships;
mod xml;

pub use content_types::{ContentType, ContentTypes};
pub use error::{OoxmlError, Result};
pub use package::{Package, Part, PartContent, PartName};
pub use relationships::{Relationship, Relationships};
pub use xml::{NamespaceDecl, QName, XmlAttribute, XmlDeclaration, XmlElement, XmlNode, XmlTree};
