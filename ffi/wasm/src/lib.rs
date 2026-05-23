//! WebAssembly bindings for Casual Core.
//!
//! Minimal, function-style API for converting documents between formats.
//! Designed for one-shot use: bytes in, bytes out. Stateful editing lives
//! upstream in the consumer (Casual Editor and friends).

use std::ffi::OsStr;

use s1_model::{
    AttributeKey, AttributeValue, DocumentModel, FieldType, LineSpacing, ListFormat, NodeId,
    NodeType, PageOrientation, TableWidth, VerticalAlignment,
};
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

/// Reverse of `open_to_json_string`: take a JSON model string and write it to
/// the requested output format.
///
/// `to` is one of the writable formats (`"docx"`, `"odt"`, `"pdf"`, `"md"`,
/// `"txt"`). Phase C of the WASM ⇄ JS pipeline — bytes-in / model-out becomes
/// model-in / bytes-out.
#[wasm_bindgen]
pub fn convert_from_model_string(json: &str, to: &str) -> Result<Vec<u8>, JsError> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| JsError::new(&format!("invalid JSON model: {e}")))?;
    let model = json_to_model(&value)
        .map_err(|e| JsError::new(&format!("model deserialization failed: {e}")))?;
    let doc = s1engine::Document::from_model(model);
    let target = parse_format(to)?;
    doc.export(target).map_err(jserr)
}

/// JSON-object variant of `convert_from_model_string` — the caller passes a
/// JS object (matching `S1DocumentModel`) instead of a JSON string.
#[wasm_bindgen]
pub fn convert_from_model(model: JsValue, to: &str) -> Result<Vec<u8>, JsError> {
    let s = js_sys::JSON::stringify(&model)
        .map_err(|e| JsError::new(&format!("JSON.stringify failed: {:?}", e)))?;
    let s: String = s.into();
    convert_from_model_string(&s, to)
}

/// Parse a document and return its model as a `JsValue` (parsed JSON object).
///
/// Equivalent to `JSON.parse(open_to_json_string(…))` but done on the Rust
/// side. Use `open_to_json_string` when you want to post the payload to a
/// worker or store it as a string.
#[wasm_bindgen]
pub fn open_to_json(data: &[u8], from: &str) -> Result<JsValue, JsError> {
    let s = open_to_json_string(data, from)?;
    js_sys::JSON::parse(&s).map_err(|e| JsError::new(&format!("JSON.parse failed: {:?}", e)))
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

// ── JSON → Model deserialization (Phase C) ──────────────────────────────────

fn parse_node_id(s: &str) -> Result<NodeId, String> {
    let (rep, ctr) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid node id {s:?} (expected 'replica:counter')"))?;
    let replica: u64 = rep
        .parse()
        .map_err(|e| format!("bad replica in {s:?}: {e}"))?;
    let counter: u64 = ctr
        .parse()
        .map_err(|e| format!("bad counter in {s:?}: {e}"))?;
    Ok(NodeId { replica, counter })
}

fn parse_node_type(s: &str) -> Option<NodeType> {
    Some(match s {
        "document" => NodeType::Document,
        "body" => NodeType::Body,
        "section" => NodeType::Section,
        "paragraph" => NodeType::Paragraph,
        "table" => NodeType::Table,
        "tableRow" => NodeType::TableRow,
        "tableCell" => NodeType::TableCell,
        "run" => NodeType::Run,
        "text" => NodeType::Text,
        "lineBreak" => NodeType::LineBreak,
        "pageBreak" => NodeType::PageBreak,
        "columnBreak" => NodeType::ColumnBreak,
        "tab" => NodeType::Tab,
        "tableOfContents" => NodeType::TableOfContents,
        "equation" => NodeType::Equation,
        "image" => NodeType::Image,
        "drawing" => NodeType::Drawing,
        "header" => NodeType::Header,
        "footer" => NodeType::Footer,
        "field" => NodeType::Field,
        "bookmarkStart" => NodeType::BookmarkStart,
        "bookmarkEnd" => NodeType::BookmarkEnd,
        "commentStart" => NodeType::CommentStart,
        "commentEnd" => NodeType::CommentEnd,
        "commentBody" => NodeType::CommentBody,
        "footnoteRef" => NodeType::FootnoteRef,
        "footnoteBody" => NodeType::FootnoteBody,
        "endnoteRef" => NodeType::EndnoteRef,
        "endnoteBody" => NodeType::EndnoteBody,
        _ => return None,
    })
}

fn parse_attr_key(s: &str) -> Option<AttributeKey> {
    Some(match s {
        "fontFamily" => AttributeKey::FontFamily,
        "fontSize" => AttributeKey::FontSize,
        "bold" => AttributeKey::Bold,
        "italic" => AttributeKey::Italic,
        "underline" => AttributeKey::Underline,
        "strikethrough" => AttributeKey::Strikethrough,
        "color" => AttributeKey::Color,
        "highlightColor" => AttributeKey::HighlightColor,
        "superscript" => AttributeKey::Superscript,
        "subscript" => AttributeKey::Subscript,
        "fontSpacing" => AttributeKey::FontSpacing,
        "language" => AttributeKey::Language,
        "alignment" => AttributeKey::Alignment,
        "indentLeft" => AttributeKey::IndentLeft,
        "indentRight" => AttributeKey::IndentRight,
        "indentFirstLine" => AttributeKey::IndentFirstLine,
        "spacingBefore" => AttributeKey::SpacingBefore,
        "spacingAfter" => AttributeKey::SpacingAfter,
        "lineSpacing" => AttributeKey::LineSpacing,
        "keepWithNext" => AttributeKey::KeepWithNext,
        "keepLinesTogether" => AttributeKey::KeepLinesTogether,
        "pageBreakBefore" => AttributeKey::PageBreakBefore,
        "background" => AttributeKey::Background,
        "styleId" => AttributeKey::StyleId,
        "cellWidth" => AttributeKey::CellWidth,
        "verticalAlign" => AttributeKey::VerticalAlign,
        "cellBackground" => AttributeKey::CellBackground,
        "colSpan" => AttributeKey::ColSpan,
        "rowSpan" => AttributeKey::RowSpan,
        "imageMediaId" => AttributeKey::ImageMediaId,
        "imageWidth" => AttributeKey::ImageWidth,
        "imageHeight" => AttributeKey::ImageHeight,
        "imageAltText" => AttributeKey::ImageAltText,
        "fieldType" => AttributeKey::FieldType,
        "fieldCode" => AttributeKey::FieldCode,
        "hyperlinkUrl" => AttributeKey::HyperlinkUrl,
        "bookmarkName" => AttributeKey::BookmarkName,
        "tableColumnWidths" => AttributeKey::TableColumnWidths,
        _ => return None,
    })
}

fn parse_color(s: &str) -> Option<s1_model::Color> {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(s1_model::Color { r, g, b, a: 255 })
    } else {
        None
    }
}

fn json_to_attr_value(key: &AttributeKey, val: &serde_json::Value) -> Option<AttributeValue> {
    use AttributeKey as K;
    use AttributeValue as V;
    match key {
        K::Bold
        | K::Italic
        | K::Strikethrough
        | K::Superscript
        | K::Subscript
        | K::KeepWithNext
        | K::KeepLinesTogether
        | K::PageBreakBefore => val.as_bool().map(V::Bool),
        K::FontSize
        | K::FontSpacing
        | K::IndentLeft
        | K::IndentRight
        | K::IndentFirstLine
        | K::SpacingBefore
        | K::SpacingAfter
        | K::ImageWidth
        | K::ImageHeight => val.as_f64().map(V::Float),
        K::FontFamily
        | K::Language
        | K::StyleId
        | K::FieldCode
        | K::HyperlinkUrl
        | K::BookmarkName
        | K::ImageAltText
        | K::TableColumnWidths => val.as_str().map(|s| V::String(s.to_string())),
        K::Color | K::Background | K::HighlightColor | K::CellBackground => {
            val.as_str().and_then(parse_color).map(V::Color)
        }
        K::Underline => val.as_str().map(|s| {
            use s1_model::UnderlineStyle::*;
            V::UnderlineStyle(match s {
                "single" => Single,
                "double" => Double,
                "thick" => Thick,
                "dotted" => Dotted,
                "dashed" => Dashed,
                "wave" => Wave,
                _ => None,
            })
        }),
        K::Alignment => val.as_str().map(|s| {
            use s1_model::Alignment::*;
            V::Alignment(match s {
                "left" => Left,
                "center" => Center,
                "right" => Right,
                "justify" => Justify,
                _ => Left,
            })
        }),
        K::VerticalAlign => val.as_str().map(|s| {
            V::VerticalAlignment(match s {
                "top" => VerticalAlignment::Top,
                "center" => VerticalAlignment::Center,
                "bottom" => VerticalAlignment::Bottom,
                _ => VerticalAlignment::Top,
            })
        }),
        K::CellWidth => val.get("type").and_then(|t| t.as_str()).map(|t| {
            let v = val.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            V::TableWidth(match t {
                "fixed" => TableWidth::Fixed(v),
                "percent" => TableWidth::Percent(v),
                _ => TableWidth::Auto,
            })
        }),
        K::ColSpan | K::RowSpan => val.as_i64().map(V::Int),
        K::ImageMediaId => val.as_u64().map(|n| V::MediaId(s1_model::MediaId(n))),
        K::FieldType => val.as_str().map(|s| {
            V::FieldType(match s {
                "pageNumber" => FieldType::PageNumber,
                "pageCount" => FieldType::PageCount,
                "date" => FieldType::Date,
                "time" => FieldType::Time,
                "fileName" => FieldType::FileName,
                "author" => FieldType::Author,
                "tableOfContents" => FieldType::TableOfContents,
                _ => FieldType::Custom,
            })
        }),
        K::LineSpacing => val.get("type").and_then(|t| t.as_str()).map(|t| {
            let v = val.get("value").and_then(|v| v.as_f64()).unwrap_or(0.0);
            V::LineSpacing(match t {
                "single" => LineSpacing::Single,
                "onePointFive" => LineSpacing::OnePointFive,
                "double" => LineSpacing::Double,
                "exact" => LineSpacing::Exact(v),
                "atLeast" => LineSpacing::AtLeast(v),
                "multiple" => LineSpacing::Multiple(v),
                _ => LineSpacing::Single,
            })
        }),
        _ => None,
    }
}

fn json_to_attrs(val: &serde_json::Value) -> s1_model::AttributeMap {
    let mut attrs = s1_model::AttributeMap::new();
    if let Some(obj) = val.as_object() {
        for (k, v) in obj {
            if let Some(key) = parse_attr_key(k) {
                if let Some(av) = json_to_attr_value(&key, v) {
                    attrs.set(key, av);
                }
            }
        }
    }
    attrs
}

fn json_to_model(value: &serde_json::Value) -> Result<DocumentModel, String> {
    let nodes_obj = value
        .get("nodes")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "missing 'nodes' object".to_string())?;
    let root_str = value
        .get("root")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'root' string".to_string())?;
    let root_id_expected = parse_node_id(root_str)?;

    let mut model = DocumentModel::new();

    // The model's own root id (NodeId(0,0)) should match the JSON's root.
    if model.root_id() != root_id_expected {
        return Err(format!(
            "JSON root id {root_id_expected:?} does not match model root {:?}",
            model.root_id()
        ));
    }

    // BFS from root: read children list of each JSON node, insert each child,
    // remembering JSON node id → new model id mapping so subsequent inserts
    // attach under the right parent.
    use std::collections::HashMap;
    let mut json_to_new: HashMap<String, NodeId> = HashMap::new();
    json_to_new.insert(root_str.to_string(), model.root_id());

    // Apply attributes onto root from JSON
    if let Some(root_node) = nodes_obj.get(root_str) {
        if let Some(attrs_val) = root_node.get("attributes") {
            let attrs = json_to_attrs(attrs_val);
            if let Some(rn) = model.node_mut(model.root_id()) {
                rn.attributes = attrs;
            }
        }
    }

    let mut queue: Vec<String> = vec![root_str.to_string()];
    while let Some(parent_key) = queue.pop() {
        let parent_node = match nodes_obj.get(&parent_key) {
            Some(v) => v,
            None => continue,
        };
        let parent_id = match json_to_new.get(&parent_key).copied() {
            Some(id) => id,
            None => continue,
        };
        let children = parent_node
            .get("children")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for (idx, child_val) in children.iter().enumerate() {
            let child_key = match child_val.as_str() {
                Some(s) => s.to_string(),
                None => continue,
            };
            let child_obj = match nodes_obj.get(&child_key) {
                Some(c) => c,
                None => continue,
            };
            let type_str = child_obj
                .get("nodeType")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let node_type = match parse_node_type(type_str) {
                Some(t) => t,
                None => continue,
            };

            let new_id = model.next_id();
            let mut node = if node_type == NodeType::Text {
                let text = child_obj
                    .get("textContent")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                s1_model::Node::text(new_id, text)
            } else {
                let mut n = s1_model::Node::new(new_id, node_type);
                if let Some(t) = child_obj.get("textContent").and_then(|v| v.as_str()) {
                    n.text_content = Some(t.to_string());
                }
                n
            };

            if let Some(av) = child_obj.get("attributes") {
                node.attributes = json_to_attrs(av);
            }

            model
                .insert_node(parent_id, idx, node)
                .map_err(|e| format!("insert {type_str} under {parent_id:?}: {e:?}"))?;

            json_to_new.insert(child_key.clone(), new_id);
            queue.push(child_key);
        }
    }

    // Metadata
    if let Some(meta_val) = value.get("metadata").and_then(|v| v.as_object()) {
        let meta = model.metadata_mut();
        if let Some(s) = meta_val.get("title").and_then(|v| v.as_str()) {
            meta.title = Some(s.to_string());
        }
        if let Some(s) = meta_val.get("subject").and_then(|v| v.as_str()) {
            meta.subject = Some(s.to_string());
        }
        if let Some(s) = meta_val.get("creator").and_then(|v| v.as_str()) {
            meta.creator = Some(s.to_string());
        }
        if let Some(s) = meta_val.get("description").and_then(|v| v.as_str()) {
            meta.description = Some(s.to_string());
        }
        if let Some(s) = meta_val.get("language").and_then(|v| v.as_str()) {
            meta.language = Some(s.to_string());
        }
    }

    Ok(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Phase C smoke test: serialize a real document to JSON, deserialize it
    /// back, and confirm the output is structurally similar.
    #[test]
    fn json_model_round_trip_preserves_paragraphs() {
        let mut model = DocumentModel::new();
        let body_id = model.next_id();
        model
            .insert_node(
                model.root_id(),
                0,
                s1_model::Node::new(body_id, NodeType::Body),
            )
            .unwrap();
        let para_id = model.next_id();
        model
            .insert_node(
                body_id,
                0,
                s1_model::Node::new(para_id, NodeType::Paragraph),
            )
            .unwrap();
        let run_id = model.next_id();
        let mut run = s1_model::Node::new(run_id, NodeType::Run);
        run.attributes
            .set(AttributeKey::Bold, AttributeValue::Bool(true));
        run.attributes.set(
            AttributeKey::FontFamily,
            AttributeValue::String("Arial".into()),
        );
        model.insert_node(para_id, 0, run).unwrap();
        let text_id = model.next_id();
        model
            .insert_node(run_id, 0, s1_model::Node::text(text_id, "hello"))
            .unwrap();

        let json = model_to_json(&model);
        let json_str = serde_json::to_string(&json).unwrap();

        let rebuilt = json_to_model(&serde_json::from_str(&json_str).unwrap()).unwrap();

        // Walk and verify
        let body2 = rebuilt
            .descendants(rebuilt.root_id())
            .into_iter()
            .find(|n| n.node_type == NodeType::Body)
            .expect("body present");
        assert_eq!(body2.children.len(), 1);
        let para2 = rebuilt.node(body2.children[0]).unwrap();
        assert_eq!(para2.node_type, NodeType::Paragraph);
        assert_eq!(para2.children.len(), 1);
        let run2 = rebuilt.node(para2.children[0]).unwrap();
        assert_eq!(run2.node_type, NodeType::Run);
        assert_eq!(run2.attributes.get_bool(&AttributeKey::Bold), Some(true));
        assert_eq!(
            run2.attributes.get_string(&AttributeKey::FontFamily),
            Some("Arial")
        );
        let text2 = rebuilt.node(run2.children[0]).unwrap();
        assert_eq!(text2.node_type, NodeType::Text);
        assert_eq!(text2.text_content.as_deref(), Some("hello"));
    }

    /// Round-trip through DOCX: bytes → model → JSON → model → bytes,
    /// verify text content survives.
    #[test]
    fn convert_from_model_string_writes_docx() {
        use s1engine::{Engine, Format};

        // Build a model with text "ROUNDTRIP" and write to DOCX bytes via the
        // engine to make sure the export path accepts a from-model document.
        let json = r#"{
            "root": "0:0",
            "nodes": {
                "0:0": {"id":"0:0","nodeType":"document","children":["0:1"],"parent":null,"textContent":null,"attributes":{}},
                "0:1": {"id":"0:1","nodeType":"body","children":["0:2"],"parent":"0:0","textContent":null,"attributes":{}},
                "0:2": {"id":"0:2","nodeType":"paragraph","children":["0:3"],"parent":"0:1","textContent":null,"attributes":{}},
                "0:3": {"id":"0:3","nodeType":"run","children":["0:4"],"parent":"0:2","textContent":null,"attributes":{}},
                "0:4": {"id":"0:4","nodeType":"text","children":[],"parent":"0:3","textContent":"ROUNDTRIP","attributes":{}}
            },
            "metadata": {"title": "Test"},
            "styles": [],
            "sections": []
        }"#;
        let model = json_to_model(&serde_json::from_str(json).unwrap()).unwrap();
        let doc = s1engine::Document::from_model(model);
        let docx_bytes = doc.export(Format::Docx).unwrap();
        assert!(docx_bytes.len() > 100, "DOCX output too small");

        // Re-parse and confirm text survives.
        let engine = Engine::new();
        let reread = engine.open(&docx_bytes).unwrap();
        assert!(reread.to_plain_text().contains("ROUNDTRIP"));
    }
}
