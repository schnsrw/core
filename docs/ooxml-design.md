# `s1-ooxml` — preservation layer for OOXML packages

## Why this crate exists

The DOCX coverage scorecard at [`docx-coverage.md`](docx-coverage.md) puts
**140 tags** in Bucket A — tags the consumer's editor preserves on
round-trip but Casual Core drops. Hand-mapping every one of them into
`s1-model` would take months and we'd still be playing catch-up against
the long tail of OOXML extensions (DrawingML, VML, SDT, complex-script
properties, table micro-properties …).

The architectural alternative is to **preserve the OOXML package
structure end-to-end** and project an editable subset into `s1-model`.
Unknown elements ride through the read → write cycle untouched.

This document specifies `s1-ooxml`, the crate that holds that
preservation layer.

## Scope

In:

- OPC (Open Packaging Convention) — ZIP container, `[Content_Types].xml`,
  `_rels/*.rels` relationship graphs.
- Lossless XML AST — every element, attribute, namespace, text run, CDATA
  block, comment, and processing instruction preserved in document order.
- Read + write APIs that produce a byte stream re-parsing into a
  structurally identical `Package`.

Out (deliberately):

- Any DOCX semantics. WordprocessingML interpretation lives in
  `s1-format-docx` and projects into `s1-model`. `s1-ooxml` does not know
  what `w:p` means.
- Any model translation. There is no `Package → DocumentModel` here. That
  bridge lives in `s1-format-docx`.
- XLSX or PPTX flavor specifics. `s1-ooxml` is the shared substrate; the
  per-flavor crates (`s1-format-docx` today, `s1-format-xlsx` /
  `s1-format-pptx` later if reintroduced) layer on top.

## Core data model

```rust
/// One OOXML package as parsed from a .docx / .xlsx / .pptx file.
pub struct Package {
    /// All parts keyed by their package-relative path
    /// (e.g. "word/document.xml").
    parts: BTreeMap<PartName, Part>,
    /// Parsed `[Content_Types].xml` — needed to know which parts are XML
    /// vs. binary on the way back out.
    content_types: ContentTypes,
    /// Parsed `_rels/*.rels` files keyed by source part name.
    relationships: BTreeMap<PartName, Relationships>,
}

pub struct Part {
    pub name: PartName,
    pub content: PartContent,
}

pub enum PartContent {
    /// XML payload — parsed as a lossless tree.
    Xml(XmlTree),
    /// Binary payload (images, embedded fonts, …) preserved verbatim.
    Binary(Vec<u8>),
}

pub struct XmlTree {
    pub xml_declaration: Option<XmlDeclaration>,
    pub root: XmlNode,
}

pub enum XmlNode {
    Element(XmlElement),
    Text(String),
    CData(String),
    Comment(String),
    ProcessingInstruction { target: String, content: String },
}

pub struct XmlElement {
    pub name: QName,
    pub namespaces: Vec<NamespaceDecl>,
    pub attributes: Vec<XmlAttribute>,
    pub children: Vec<XmlNode>,
}

pub struct QName {
    pub prefix: Option<String>,
    pub local_name: String,
}
```

Key invariants:

1. `XmlElement::children` is order-preserving. Whitespace text nodes
   stay where they were.
2. `XmlElement::attributes` preserves order (XML attribute order is not
   semantic per spec, but some consumers are sensitive to it).
3. `Part::content` distinguishes XML from binary based on the part's
   content type from `[Content_Types].xml`, not on file extension.

## Read flow

```
DOCX bytes
   ▼
zip::ZipArchive
   ▼  for each entry:
   ├─ "[Content_Types].xml"  → ContentTypes::parse
   ├─ "_rels/*.rels" or "**/_rels/*.rels" → Relationships::parse
   ├─ other XML parts        → XmlTree::parse
   └─ binary parts           → PartContent::Binary
   ▼
Package
```

XML parts are determined by the content-type lookup, with a fallback
list for parts that match `*.xml` / `*.rels` extensions when the
`[Content_Types].xml` mapping is missing or partial (common in
real-world DOCX written by non-Microsoft tools).

## Write flow

The inverse:

```
Package
   ▼  for each (name, part):
   ├─ ContentTypes  → "[Content_Types].xml"
   ├─ Relationships → "_rels/*.rels"
   ├─ XmlTree       → serialize with quick-xml
   └─ Binary        → write verbatim
   ▼
zip::ZipWriter
   ▼
DOCX bytes
```

XML serialization preserves:

- The original XML declaration (`<?xml version="1.0" ... ?>`).
- Namespace declarations on the element where they were originally
  declared.
- Attribute order as captured at parse time.
- Self-closing tag form for elements with no children (`<w:b/>` not
  `<w:b></w:b>`) when that matches the input style.

It does *not* attempt to preserve:

- Whitespace within text nodes (already preserved as content).
- Original indentation of synthetic whitespace between elements (each
  consumer normalises differently; we re-emit with a consistent style).

That's enough for the **tag-census** round-trip test to be zero-drop,
which is the metric the integration plan gates on.

## Edit projection (not built in this crate)

This crate is read-only-or-rewrite — it doesn't expose editing.
`s1-format-docx` will:

1. Read DOCX → `Package` (this crate) → project into `Document`
   (`s1-model`) for the parts of WordprocessingML it understands.
2. Hold the `Package` and the projected `Document` together (probably as
   `(Package, Document)` inside `s1engine::Document`).
3. On write, walk the original `Package`, replace the editable subtrees
   with their re-projected forms from the modified `Document`, leave
   everything else untouched.

That's the next layer of work. This crate is only concerned with making
that future layer possible by ensuring **nothing is lost on round-trip
through `Package`**.

## Test gates

Before this crate ships as "passthrough complete":

1. **Per-part tag census round-trip is zero-drop** on every fixture in
   `testdocs/docx/eigenpal/`.
2. **`Package::parse(b).write()` re-parses to a structurally equal
   `Package`** on every fixture.
3. Hostile-input tests: malformed XML, truncated zip, missing
   `[Content_Types].xml`, circular relationships — none of these panic.
4. Property tests on the XML AST: random trees survive
   `serialize → parse` round trip.

## Dependencies

Just three:

```toml
quick-xml = { workspace = true }
zip       = { workspace = true }
thiserror = { workspace = true }
```

Same set as `s1-format-docx`. No `s1-model` dependency — this crate is
*below* the document model layer.

## Why not put this inside `s1-format-docx`?

Because OOXML is broader than DOCX, and even if we never reintroduce
XLSX or PPTX, having a clean crate boundary makes the preservation
discipline easier to enforce in code review. The whole point is that
this crate doesn't know about WordprocessingML — it shouldn't be
allowed to.

## Naming convention reminder

OOXML = the umbrella standard (ECMA-376). DOCX = one *flavor*. This
crate sits at the OOXML tier; `s1-format-docx` sits at the DOCX tier.
