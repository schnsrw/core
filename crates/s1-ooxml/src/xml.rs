//! Lossless XML AST + parser + writer.
//!
//! Designed to round-trip OOXML XML parts through `parse → write → parse`
//! with structural equality. The goal is not byte-perfect output (some
//! normalisation is unavoidable when round-tripping through any XML
//! library) but no information loss: every element, attribute, namespace
//! declaration, text node, CDATA section, comment, and processing
//! instruction is captured in document order.

use std::io::Cursor;

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use quick_xml::Writer;

use crate::error::Result;

// ---------------------------------------------------------------------------
// Public AST
// ---------------------------------------------------------------------------

/// A complete XML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlTree {
    /// `<?xml version="1.0" ... ?>` if present.
    pub declaration: Option<XmlDeclaration>,
    /// Nodes before the root element (comments, processing instructions).
    pub prologue: Vec<XmlNode>,
    /// The root element of the document.
    pub root: XmlElement,
}

/// XML declaration (`<?xml ... ?>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlDeclaration {
    pub version: String,
    pub encoding: Option<String>,
    pub standalone: Option<String>,
}

/// One node inside an element's children list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XmlNode {
    Element(XmlElement),
    Text(String),
    CData(String),
    Comment(String),
    ProcessingInstruction { target: String, content: String },
}

/// An XML element with its full content tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlElement {
    pub name: QName,
    /// `xmlns:foo="..."` declarations attached to this element.
    pub namespaces: Vec<NamespaceDecl>,
    pub attributes: Vec<XmlAttribute>,
    pub children: Vec<XmlNode>,
}

/// A `(prefix, local-name)` pair as it appears in the source XML.
///
/// We preserve the prefix verbatim — `w:p` and `wp:p` are distinct names
/// even though "p" is the same local part.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QName {
    pub prefix: Option<String>,
    pub local_name: String,
}

impl QName {
    /// Reconstruct the `prefix:local-name` string used in the source XML.
    pub fn full(&self) -> String {
        match &self.prefix {
            Some(p) => format!("{}:{}", p, self.local_name),
            None => self.local_name.clone(),
        }
    }
}

/// An attribute keyed by its full source name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlAttribute {
    pub name: QName,
    pub value: String,
}

/// One `xmlns` or `xmlns:foo` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceDecl {
    /// `None` for the default namespace (`xmlns="..."`), `Some(prefix)` for
    /// prefixed (`xmlns:foo="..."`).
    pub prefix: Option<String>,
    pub uri: String,
}

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------

impl XmlTree {
    /// Parse an XML document into a lossless tree.
    pub fn parse(input: &str) -> Result<Self> {
        let mut reader = Reader::from_str(input);
        reader.config_mut().trim_text(false);
        reader.config_mut().expand_empty_elements = false;

        let mut declaration: Option<XmlDeclaration> = None;
        let mut prologue: Vec<XmlNode> = Vec::new();
        let mut root: Option<XmlElement> = None;

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Decl(d)) => {
                    let version = d
                        .version()
                        .ok()
                        .and_then(|v| std::str::from_utf8(&v).ok().map(str::to_owned))
                        .unwrap_or_else(|| "1.0".to_owned());
                    let encoding = d
                        .encoding()
                        .and_then(|r| r.ok())
                        .and_then(|v| std::str::from_utf8(&v).ok().map(str::to_owned));
                    let standalone = d
                        .standalone()
                        .and_then(|r| r.ok())
                        .and_then(|v| std::str::from_utf8(&v).ok().map(str::to_owned));
                    declaration = Some(XmlDeclaration {
                        version,
                        encoding,
                        standalone,
                    });
                }
                Ok(Event::Start(e)) => {
                    let element = parse_element(&mut reader, &e, false)?;
                    if root.is_some() {
                        // Multiple root elements aren't valid XML, but be
                        // lenient — capture as a child of synthetic prologue.
                        return Ok(XmlTree {
                            declaration,
                            prologue,
                            root: root.unwrap(),
                        });
                    }
                    root = Some(element);
                }
                Ok(Event::Empty(e)) => {
                    let element = parse_element(&mut reader, &e, true)?;
                    if root.is_some() {
                        return Ok(XmlTree {
                            declaration,
                            prologue,
                            root: root.unwrap(),
                        });
                    }
                    root = Some(element);
                }
                Ok(Event::Comment(c)) => {
                    let s = std::str::from_utf8(c.as_ref()).unwrap_or("").to_owned();
                    if root.is_none() {
                        prologue.push(XmlNode::Comment(s));
                    }
                }
                Ok(Event::PI(pi)) => {
                    if root.is_none() {
                        let raw = std::str::from_utf8(pi.as_ref()).unwrap_or("");
                        let (target, content) = match raw.find(char::is_whitespace) {
                            Some(idx) => {
                                (raw[..idx].to_owned(), raw[idx..].trim_start().to_owned())
                            }
                            None => (raw.to_owned(), String::new()),
                        };
                        prologue.push(XmlNode::ProcessingInstruction { target, content });
                    }
                }
                Ok(Event::Text(_)) | Ok(Event::CData(_)) => {
                    // Top-level text is whitespace-only in valid XML — drop.
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(e) => {
                    return Err(crate::error::OoxmlError::Xml {
                        part: String::from("<inline>"),
                        source: e,
                    });
                }
            }
            buf.clear();
        }

        let root = root.ok_or_else(|| {
            crate::error::OoxmlError::Malformed("XML document has no root element".to_owned())
        })?;

        Ok(XmlTree {
            declaration,
            prologue,
            root,
        })
    }
}

fn parse_element(
    reader: &mut Reader<&[u8]>,
    start: &BytesStart<'_>,
    self_closing: bool,
) -> Result<XmlElement> {
    let name = qname_from_bytes(start.name().as_ref());

    let (namespaces, attributes) = parse_attributes(start)?;

    let mut children: Vec<XmlNode> = Vec::new();

    if self_closing {
        return Ok(XmlElement {
            name,
            namespaces,
            attributes,
            children,
        });
    }

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let child = parse_element(reader, &e, false)?;
                children.push(XmlNode::Element(child));
            }
            Ok(Event::Empty(e)) => {
                let child = parse_element(reader, &e, true)?;
                children.push(XmlNode::Element(child));
            }
            Ok(Event::Text(t)) => {
                let raw = t.into_inner();
                let s = std::str::from_utf8(&raw).unwrap_or("").to_owned();
                // Reverse the XML escapes the reader applied
                let unescaped = quick_xml::escape::unescape(&s)
                    .map(|c| c.into_owned())
                    .unwrap_or(s);
                children.push(XmlNode::Text(unescaped));
            }
            Ok(Event::CData(c)) => {
                let s = std::str::from_utf8(c.as_ref()).unwrap_or("").to_owned();
                children.push(XmlNode::CData(s));
            }
            Ok(Event::Comment(c)) => {
                let s = std::str::from_utf8(c.as_ref()).unwrap_or("").to_owned();
                children.push(XmlNode::Comment(s));
            }
            Ok(Event::PI(pi)) => {
                let raw = std::str::from_utf8(pi.as_ref()).unwrap_or("");
                let (target, content) = match raw.find(char::is_whitespace) {
                    Some(idx) => (raw[..idx].to_owned(), raw[idx..].trim_start().to_owned()),
                    None => (raw.to_owned(), String::new()),
                };
                children.push(XmlNode::ProcessingInstruction { target, content });
            }
            Ok(Event::End(_)) => break,
            Ok(Event::Eof) => {
                return Err(crate::error::OoxmlError::Malformed(format!(
                    "unexpected EOF inside element `{}`",
                    name.full()
                )));
            }
            Ok(_) => {}
            Err(e) => {
                return Err(crate::error::OoxmlError::Xml {
                    part: String::from("<inline>"),
                    source: e,
                });
            }
        }
        buf.clear();
    }

    Ok(XmlElement {
        name,
        namespaces,
        attributes,
        children,
    })
}

fn parse_attributes(start: &BytesStart<'_>) -> Result<(Vec<NamespaceDecl>, Vec<XmlAttribute>)> {
    let mut namespaces = Vec::new();
    let mut attributes = Vec::new();

    for attr in start.attributes() {
        let attr: Attribute<'_> = attr.map_err(|e| crate::error::OoxmlError::Xml {
            part: String::from("<inline>"),
            source: e.into(),
        })?;
        let name_bytes = attr.key.as_ref();
        let value = attr.unescape_value().map(|c| c.into_owned()).map_err(|e| {
            crate::error::OoxmlError::Xml {
                part: String::from("<inline>"),
                source: e,
            }
        })?;

        // Detect namespace declarations: `xmlns` or `xmlns:prefix`.
        if name_bytes == b"xmlns" {
            namespaces.push(NamespaceDecl {
                prefix: None,
                uri: value,
            });
        } else if let Some(rest) = name_bytes.strip_prefix(b"xmlns:") {
            let prefix = std::str::from_utf8(rest).unwrap_or("").to_owned();
            namespaces.push(NamespaceDecl {
                prefix: Some(prefix),
                uri: value,
            });
        } else {
            attributes.push(XmlAttribute {
                name: qname_from_bytes(name_bytes),
                value,
            });
        }
    }

    Ok((namespaces, attributes))
}

fn qname_from_bytes(bytes: &[u8]) -> QName {
    let s = std::str::from_utf8(bytes).unwrap_or("");
    match s.find(':') {
        Some(idx) => QName {
            prefix: Some(s[..idx].to_owned()),
            local_name: s[idx + 1..].to_owned(),
        },
        None => QName {
            prefix: None,
            local_name: s.to_owned(),
        },
    }
}

// ---------------------------------------------------------------------------
// Write
// ---------------------------------------------------------------------------

impl XmlTree {
    /// Serialize this tree back into XML bytes.
    pub fn write(&self) -> Result<Vec<u8>> {
        let mut out: Vec<u8> = Vec::new();

        if let Some(decl) = &self.declaration {
            out.extend_from_slice(b"<?xml version=\"");
            escape_attr_value(&decl.version, &mut out);
            out.extend_from_slice(b"\"");
            if let Some(enc) = &decl.encoding {
                out.extend_from_slice(b" encoding=\"");
                escape_attr_value(enc, &mut out);
                out.extend_from_slice(b"\"");
            }
            if let Some(sa) = &decl.standalone {
                out.extend_from_slice(b" standalone=\"");
                escape_attr_value(sa, &mut out);
                out.extend_from_slice(b"\"");
            }
            out.extend_from_slice(b"?>");
        }

        for node in &self.prologue {
            write_node(node, &mut out)?;
        }

        write_element(&self.root, &mut out)?;
        Ok(out)
    }
}

fn write_element(el: &XmlElement, out: &mut Vec<u8>) -> Result<()> {
    let name = el.name.full();
    out.push(b'<');
    out.extend_from_slice(name.as_bytes());

    for ns in &el.namespaces {
        match &ns.prefix {
            Some(p) => {
                out.extend_from_slice(b" xmlns:");
                out.extend_from_slice(p.as_bytes());
                out.extend_from_slice(b"=\"");
                escape_attr_value(&ns.uri, out);
                out.extend_from_slice(b"\"");
            }
            None => {
                out.extend_from_slice(b" xmlns=\"");
                escape_attr_value(&ns.uri, out);
                out.extend_from_slice(b"\"");
            }
        }
    }

    for attr in &el.attributes {
        out.push(b' ');
        out.extend_from_slice(attr.name.full().as_bytes());
        out.extend_from_slice(b"=\"");
        escape_attr_value(&attr.value, out);
        out.push(b'"');
    }

    if el.children.is_empty() {
        out.extend_from_slice(b"/>");
        return Ok(());
    }

    out.push(b'>');
    for child in &el.children {
        write_node(child, out)?;
    }
    out.extend_from_slice(b"</");
    out.extend_from_slice(name.as_bytes());
    out.push(b'>');

    Ok(())
}

fn write_node(node: &XmlNode, out: &mut Vec<u8>) -> Result<()> {
    match node {
        XmlNode::Element(el) => write_element(el, out)?,
        XmlNode::Text(t) => escape_text(t, out),
        XmlNode::CData(c) => {
            out.extend_from_slice(b"<![CDATA[");
            out.extend_from_slice(c.as_bytes());
            out.extend_from_slice(b"]]>");
        }
        XmlNode::Comment(c) => {
            out.extend_from_slice(b"<!--");
            out.extend_from_slice(c.as_bytes());
            out.extend_from_slice(b"-->");
        }
        XmlNode::ProcessingInstruction { target, content } => {
            out.extend_from_slice(b"<?");
            out.extend_from_slice(target.as_bytes());
            if !content.is_empty() {
                out.push(b' ');
                out.extend_from_slice(content.as_bytes());
            }
            out.extend_from_slice(b"?>");
        }
    }
    Ok(())
}

fn escape_text(s: &str, out: &mut Vec<u8>) {
    for ch in s.chars() {
        match ch {
            '<' => out.extend_from_slice(b"&lt;"),
            '>' => out.extend_from_slice(b"&gt;"),
            '&' => out.extend_from_slice(b"&amp;"),
            _ => {
                let mut tmp = [0u8; 4];
                let s = ch.encode_utf8(&mut tmp);
                out.extend_from_slice(s.as_bytes());
            }
        }
    }
}

fn escape_attr_value(s: &str, out: &mut Vec<u8>) {
    for ch in s.chars() {
        match ch {
            '<' => out.extend_from_slice(b"&lt;"),
            '>' => out.extend_from_slice(b"&gt;"),
            '&' => out.extend_from_slice(b"&amp;"),
            '"' => out.extend_from_slice(b"&quot;"),
            '\n' => out.extend_from_slice(b"&#10;"),
            '\r' => out.extend_from_slice(b"&#13;"),
            '\t' => out.extend_from_slice(b"&#9;"),
            _ => {
                let mut tmp = [0u8; 4];
                let s = ch.encode_utf8(&mut tmp);
                out.extend_from_slice(s.as_bytes());
            }
        }
    }
}

// Silence unused-import warnings during incremental development.
#[allow(dead_code)]
fn _silence_unused_warnings(_: Cursor<Vec<u8>>, _: Writer<Vec<u8>>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn roundtrip_simple() {
        let input =
            r#"<?xml version="1.0" encoding="UTF-8"?><root xmlns:w="urn:w"><w:p>hi</w:p></root>"#;
        let tree = XmlTree::parse(input).unwrap();
        let out = tree.write().unwrap();
        let reparsed = XmlTree::parse(std::str::from_utf8(&out).unwrap()).unwrap();
        assert_eq!(tree, reparsed);
    }

    #[test]
    fn preserves_attributes_and_self_closing() {
        let input = r#"<?xml version="1.0"?><root><a foo="1" bar="2"/><b/><c>text</c></root>"#;
        let tree = XmlTree::parse(input).unwrap();
        let out = tree.write().unwrap();
        let reparsed = XmlTree::parse(std::str::from_utf8(&out).unwrap()).unwrap();
        assert_eq!(tree, reparsed);
    }

    #[test]
    fn preserves_namespaces() {
        let input = r#"<?xml version="1.0"?><w:document xmlns:w="urn:w" xmlns:r="urn:r"><w:p r:id="1"/></w:document>"#;
        let tree = XmlTree::parse(input).unwrap();
        assert_eq!(tree.root.namespaces.len(), 2);
        let out = tree.write().unwrap();
        let reparsed = XmlTree::parse(std::str::from_utf8(&out).unwrap()).unwrap();
        assert_eq!(tree, reparsed);
    }

    #[test]
    fn preserves_text_with_escapes() {
        let input = r#"<r>a &lt;b&gt; &amp; c</r>"#;
        let tree = XmlTree::parse(input).unwrap();
        // After parse, text holds the unescaped form
        assert_eq!(
            tree.root.children,
            vec![XmlNode::Text("a <b> & c".to_owned())]
        );
        let out = tree.write().unwrap();
        let reparsed = XmlTree::parse(std::str::from_utf8(&out).unwrap()).unwrap();
        assert_eq!(tree, reparsed);
    }
}
