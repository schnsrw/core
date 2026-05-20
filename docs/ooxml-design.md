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

## Edit projection — how the layers fit together (current state)

This crate is preservation-only — it doesn't expose editing.
`s1engine::Document` owns the bridge between preservation and projection:

1. `Engine::open(Format::Docx)` calls
   `s1_format_docx::reader::read_with_package(bytes)`, which returns
   both a projected `DocumentModel` *and* the full `Package`.
2. `s1engine::Document` stores both — model in the `model` field,
   package in the `preservation` field, plus a `model_dirty` flag.
3. On `Document::export(Docx)`:
   - `!model_dirty`  →  `Package::write()` directly (verbatim).
   - `model_dirty + preservation`  →  the **splice path** in
     `s1engine::Document::export_docx_spliced`: regenerate
     `word/document.xml` from the model via the existing DOCX writer,
     swap that part into a clone of the preserved `Package`, write the
     clone. Every other part rides through.
4. Any mutation through the operation system (`apply`,
   `apply_transaction`, `undo`, `redo`, `update_toc`) sets
   `model_dirty = true`. Escape-hatch mutation (`model_mut`,
   `metadata_mut`) drops preservation entirely.

This gives the consumer two of the three lossless cases out of the box:

| Path | Lossless? |
| --- | --- |
| Open + export (no edits) | ✅ verbatim |
| Open + edit + export — non-body parts | ✅ ride through |
| Open + edit + export — unknowns inside `word/document.xml` | ❌ Phase 2b |

## Phase 2b — body preservation under edits

The remaining gap is unknown elements *inside* `word/document.xml`.
The current splice regenerates the whole body, so anything we never
projected into `DocumentModel` (DrawingML / VML / SDT / complex-script
properties, etc.) is gone the moment the user edits.

Plan:

1. **Side-table**: during `read_with_package`, populate a
   `HashMap<NodeId, XmlElementHandle>` that points each projected
   paragraph / table / sectPr back at its origin
   `s1_ooxml::XmlElement`. Stored alongside `Package` on
   `s1engine::Document`.
2. **Dirty NodeIds**: extend `s1_ops::Operation` so we can extract the
   set of `NodeId`s that an operation touches. `Document::apply` /
   `apply_transaction` accumulate dirty IDs into a `HashSet<NodeId>`.
3. **Per-node splice**: rewrite `export_docx_spliced` so it walks the
   preserved body's children in order. For each child:
   - If the side-table tells us which `NodeId` it projected to and
     that NodeId is clean → copy the original `XmlElement` verbatim.
   - If the NodeId is dirty → re-emit from the model via the existing
     element writer.
   - If the side-table has no entry (we never projected this child) →
     copy verbatim. This is how unknowns ride through.

Effect: the body Bucket A closes the same way the no-edits Bucket A
did — structurally, not per-tag.

## Test gates

These run on every push. Regression on any of them fails CI.

1. **`s1-ooxml::passthrough`** — every fixture in
   `testdocs/docx/eigenpal/` parses + writes via `Package` with zero
   tag drop. ✅ today: 39/39.
2. **`s1engine::docx_coverage`** — engine-level coverage scorecard
   (no-edits path). ✅ today: 39/39 zero-drop, Bucket A = 0.
3. **`s1engine::docx_edit_coverage`** — engine-level coverage scorecard
   (with-edits path). Asserts non-body preservation on every fixture.
   ✅ today: 39/39 non-body preserved; 10/39 body-zero-drop (Phase 2b
   target: 39/39).
4. **Per-crate unit tests** — `Package`, `XmlTree`, `ContentTypes`,
   `Relationships`. ✅ today: green.

Future gates (not yet wired):

- Hostile-input tests at the `s1-ooxml` layer (truncated zips, malformed
  XML, missing `[Content_Types].xml`, circular relationships).
- Property tests on the XML AST: random trees survive
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
