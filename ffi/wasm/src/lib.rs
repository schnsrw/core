//! WebAssembly bindings for Casual Core.
//!
//! Minimal, function-style API for converting documents between formats.
//! Designed for one-shot use: bytes in, bytes out. Stateful editing lives
//! upstream in the consumer (Casual Editor and friends).

use std::ffi::OsStr;

use s1engine::{Engine, Format};
use s1_model::{
    AttributeKey, AttributeValue, DocumentModel, FieldType, LineSpacing, ListFormat,
    NodeId, NodeType, PageOrientation, TableWidth, VerticalAlignment,
};
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

/// Parse a document and return its model as a JSON string.
///
/// The returned string is a UTF-8 JSON object with shape `S1DocumentModel`
/// (see `js/src/types.ts`). Prefer this over `open_to_json` for large
/// documents — string transfer avoids repeated JS/WASM boundary crossings.
#[wasm_bindgen]
pub fn open_to_json_string(data: &[u8], from: &str) -> Result<String, JsError> {
    let doc = open_document(data, from)?;
    let json = model_to_json(doc.model());
    serde_json::to_string(&json).map_err(|e| JsError::new(&e.to_string()))
}

/// Parse a document and return its model as a `JsValue` (parsed JSON object).
///
/// Equivalent to `JSON.parse(open_to_json_string(…))` but done on the Rust
/// side. Use `open_to_json_string` when you want to post the payload to a
/// worker or store it as a string.
#[wasm_bindgen]
pub fn open_to_json(data: &[u8], from: &str) -> Result<JsValue, JsError> {
    let s = open_to_json_string(data, from)?;
    js_sys::JSON::parse(&s).map_err(|e| {
        JsError::new(&format!("JSON.parse failed: {:?}", e))
    })
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

// ── Model → JSON serialization ────────────────────────────────────────────────

fn node_id_str(id: NodeId) -> String {
    format!("{}:{}", id.replica, id.counter)
}

fn node_type_str(nt: NodeType) -> &'static str {
    match nt {
        NodeType::Document => "document",
        NodeType::Body => "body",
        NodeType::Section => "section",
        NodeType::Paragraph => "paragraph",
        NodeType::Table => "table",
        NodeType::TableRow => "tableRow",
        NodeType::TableCell => "tableCell",
        NodeType::Run => "run",
        NodeType::Text => "text",
        NodeType::LineBreak => "lineBreak",
        NodeType::PageBreak => "pageBreak",
        NodeType::ColumnBreak => "columnBreak",
        NodeType::Tab => "tab",
        NodeType::TableOfContents => "tableOfContents",
        NodeType::Equation => "equation",
        NodeType::Image => "image",
        NodeType::Drawing => "drawing",
        NodeType::Header => "header",
        NodeType::Footer => "footer",
        NodeType::Field => "field",
        NodeType::BookmarkStart => "bookmarkStart",
        NodeType::BookmarkEnd => "bookmarkEnd",
        NodeType::CommentStart => "commentStart",
        NodeType::CommentEnd => "commentEnd",
        NodeType::CommentBody => "commentBody",
        NodeType::FootnoteRef => "footnoteRef",
        NodeType::FootnoteBody => "footnoteBody",
        NodeType::EndnoteRef => "endnoteRef",
        NodeType::EndnoteBody => "endnoteBody",
        _ => "unknown",
    }
}

fn attr_key_str(key: &AttributeKey) -> String {
    // Derive camelCase from the PascalCase Debug representation.
    let pascal = format!("{key:?}");
    let mut c = pascal.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_lowercase().to_string() + c.as_str(),
    }
}

fn attr_value_to_json(val: &AttributeValue) -> serde_json::Value {
    use serde_json::json;
    match val {
        AttributeValue::Bool(b) => json!(b),
        AttributeValue::Int(i) => json!(i),
        AttributeValue::Float(f) => json!(f),
        AttributeValue::String(s) => json!(s),
        AttributeValue::Color(c) => json!(format!("#{}", c.to_hex())),
        AttributeValue::Alignment(a) => json!(format!("{a:?}").to_lowercase()),
        AttributeValue::UnderlineStyle(u) => json!(format!("{u:?}").to_lowercase()),
        AttributeValue::LineSpacing(ls) => match ls {
            LineSpacing::Single => json!({"type": "single"}),
            LineSpacing::OnePointFive => json!({"type": "onePointFive"}),
            LineSpacing::Double => json!({"type": "double"}),
            LineSpacing::Exact(v) => json!({"type": "exact", "value": v}),
            LineSpacing::AtLeast(v) => json!({"type": "atLeast", "value": v}),
            LineSpacing::Multiple(v) => json!({"type": "multiple", "value": v}),
            _ => json!(null),
        },
        AttributeValue::Borders(b) => {
            json!({
                "top": b.top.as_ref().map(|s| json!({
                    "style": format!("{:?}", s.style).to_lowercase(),
                    "width": s.width,
                    "color": format!("#{}", s.color.to_hex()),
                })),
                "bottom": b.bottom.as_ref().map(|s| json!({
                    "style": format!("{:?}", s.style).to_lowercase(),
                    "width": s.width,
                    "color": format!("#{}", s.color.to_hex()),
                })),
                "left": b.left.as_ref().map(|s| json!({
                    "style": format!("{:?}", s.style).to_lowercase(),
                    "width": s.width,
                    "color": format!("#{}", s.color.to_hex()),
                })),
                "right": b.right.as_ref().map(|s| json!({
                    "style": format!("{:?}", s.style).to_lowercase(),
                    "width": s.width,
                    "color": format!("#{}", s.color.to_hex()),
                })),
            })
        }
        AttributeValue::TabStops(stops) => {
            let arr: Vec<_> = stops
                .iter()
                .map(|s| {
                    json!({
                        "position": s.position,
                        "alignment": format!("{:?}", s.alignment).to_lowercase(),
                        "leader": format!("{:?}", s.leader).to_lowercase(),
                    })
                })
                .collect();
            json!(arr)
        }
        AttributeValue::ListInfo(li) => {
            json!({
                "numId": li.num_id,
                "level": li.level,
                "numFormat": list_format_str(li.num_format),
                "start": li.start,
            })
        }
        AttributeValue::PageOrientation(o) => match o {
            PageOrientation::Portrait => json!("portrait"),
            PageOrientation::Landscape => json!("landscape"),
            _ => json!("portrait"),
        },
        AttributeValue::TableWidth(tw) => match tw {
            TableWidth::Auto => json!({"type": "auto"}),
            TableWidth::Fixed(v) => json!({"type": "fixed", "value": v}),
            TableWidth::Percent(v) => json!({"type": "percent", "value": v}),
            _ => json!({"type": "auto"}),
        },
        AttributeValue::VerticalAlignment(va) => match va {
            VerticalAlignment::Top => json!("top"),
            VerticalAlignment::Center => json!("center"),
            VerticalAlignment::Bottom => json!("bottom"),
            _ => json!("top"),
        },
        AttributeValue::MediaId(id) => json!(id.0),
        AttributeValue::FieldType(ft) => json!(field_type_str(*ft)),
        _ => serde_json::Value::Null,
    }
}

fn list_format_str(f: ListFormat) -> &'static str {
    match f {
        ListFormat::Bullet => "bullet",
        ListFormat::Decimal => "decimal",
        ListFormat::LowerAlpha => "lowerAlpha",
        ListFormat::UpperAlpha => "upperAlpha",
        ListFormat::LowerRoman => "lowerRoman",
        ListFormat::UpperRoman => "upperRoman",
        _ => "bullet",
    }
}

fn field_type_str(f: FieldType) -> &'static str {
    match f {
        FieldType::PageNumber => "pageNumber",
        FieldType::PageCount => "pageCount",
        FieldType::Date => "date",
        FieldType::Time => "time",
        FieldType::FileName => "fileName",
        FieldType::Author => "author",
        FieldType::TableOfContents => "tableOfContents",
        FieldType::Custom => "custom",
        _ => "custom",
    }
}

fn attrs_to_json(attrs: &s1_model::AttributeMap) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (key, val) in attrs.iter() {
        map.insert(attr_key_str(key), attr_value_to_json(val));
    }
    serde_json::Value::Object(map)
}

fn node_to_json(node: &s1_model::Node) -> serde_json::Value {
    use serde_json::json;
    json!({
        "id": node_id_str(node.id),
        "nodeType": node_type_str(node.node_type),
        "children": node.children.iter().map(|id| node_id_str(*id)).collect::<Vec<_>>(),
        "parent": node.parent.map(node_id_str),
        "textContent": node.text_content,
        "attributes": attrs_to_json(&node.attributes),
    })
}

fn model_to_json(model: &DocumentModel) -> serde_json::Value {
    use serde_json::json;

    let root_id = model.root_id();

    // Collect all nodes: root + every descendant
    let mut nodes_map = serde_json::Map::new();
    if let Some(root) = model.root_node() {
        nodes_map.insert(node_id_str(root.id), node_to_json(root));
    }
    for node in model.descendants(root_id) {
        nodes_map.insert(node_id_str(node.id), node_to_json(node));
    }

    let meta = model.metadata();
    let metadata_json = json!({
        "title": meta.title,
        "subject": meta.subject,
        "creator": meta.creator,
        "description": meta.description,
        "keywords": meta.keywords,
        "created": meta.created,
        "modified": meta.modified,
        "revision": meta.revision,
        "language": meta.language,
    });

    let styles_json: Vec<serde_json::Value> = model
        .styles()
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "styleType": format!("{:?}", s.style_type).to_lowercase(),
                "parentId": s.parent_id,
                "nextStyleId": s.next_style_id,
                "isDefault": s.is_default,
                "attributes": attrs_to_json(&s.attributes),
            })
        })
        .collect();

    let sections_json: Vec<serde_json::Value> = model
        .sections()
        .iter()
        .map(|s| {
            json!({
                "pageWidth": s.page_width,
                "pageHeight": s.page_height,
                "orientation": format!("{:?}", s.orientation).to_lowercase(),
                "marginTop": s.margin_top,
                "marginBottom": s.margin_bottom,
                "marginLeft": s.margin_left,
                "marginRight": s.margin_right,
                "headerDistance": s.header_distance,
                "footerDistance": s.footer_distance,
                "columns": s.columns,
                "columnSpacing": s.column_spacing,
                "titlePage": s.title_page,
                "evenAndOddHeaders": s.even_and_odd_headers,
            })
        })
        .collect();

    json!({
        "root": node_id_str(root_id),
        "nodes": serde_json::Value::Object(nodes_map),
        "metadata": metadata_json,
        "styles": styles_json,
        "sections": sections_json,
    })
}
