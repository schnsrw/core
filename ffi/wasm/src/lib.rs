//! WebAssembly bindings for Casual Core.
//!
//! Minimal, function-style API for converting documents between formats.
//! Designed for one-shot use: bytes in, bytes out. Stateful editing lives
//! upstream in the consumer (Casual Editor and friends).

use std::ffi::OsStr;

use s1engine::{Engine, Format};
use wasm_bindgen::prelude::*;

/// Detect a document's format from its first bytes.
///
/// Returns the canonical extension (`"docx"`, `"odt"`, `"pdf"`, `"md"`,
/// `"txt"`, `"doc"`, …) or `"txt"` as a fallback for unknown plain bytes.
#[wasm_bindgen]
pub fn detect_format(data: &[u8]) -> String {
    Format::detect(data).extension().to_string()
}

/// Convert a document from one format to another.
///
/// `from` may be the canonical extension (e.g. `"docx"`) or an empty string
/// to auto-detect. `to` must be one of the writable formats: `"docx"`,
/// `"odt"`, `"pdf"`, `"md"`, `"txt"`.
#[wasm_bindgen]
pub fn convert(data: &[u8], from: &str, to: &str) -> Result<Vec<u8>, JsError> {
    let doc = open_document(data, from)?;
    let target = parse_format(to)?;
    doc.export(target).map_err(jserr)
}

/// Convert a document and return the result as a UTF-8 string. Useful for
/// text outputs (`"md"`, `"txt"`); binary formats go through [`convert`].
#[wasm_bindgen]
pub fn convert_to_string(data: &[u8], from: &str, to: &str) -> Result<String, JsError> {
    let doc = open_document(data, from)?;
    let target = parse_format(to)?;
    doc.export_string(target).map_err(jserr)
}

/// Extract the plain-text content of a document. Convenience for callers
/// that only need the prose, no formatting.
#[wasm_bindgen]
pub fn extract_text(data: &[u8], from: &str) -> Result<String, JsError> {
    let doc = open_document(data, from)?;
    Ok(doc.to_plain_text())
}

fn open_document(data: &[u8], from: &str) -> Result<s1engine::Document, JsError> {
    let engine = Engine::new();
    if from.is_empty() {
        engine.open(data).map_err(jserr)
    } else {
        let f = parse_format(from)?;
        engine.open_as(data, f).map_err(jserr)
    }
}

fn parse_format(s: &str) -> Result<Format, JsError> {
    Format::from_extension(OsStr::new(s)).map_err(jserr)
}

fn jserr<E: std::fmt::Display>(e: E) -> JsError {
    JsError::new(&e.to_string())
}
