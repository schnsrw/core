# Changelog

All notable changes to **Casual Core** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **WASM Phase B + C — JSON model surface.** `openToModel`,
  `openToModelString` deserialize a document to a typed JSON tree
  (`S1DocumentModel` with `S1Node[]`, metadata, styles, sections);
  `convertModel`, `convertModelString` write a (possibly mutated) model
  back to any supported output format. Symmetric round-trip enables an
  editor consumer to operate on structured nodes without parsing
  format-specific XML. (`ffi/wasm/src/lib.rs`, `js/src/index.ts`,
  `js/src/types.ts`)
- **Cross-format fidelity audit harness.** New
  `cross_format_fidelity_audit` test in
  `crates/s1engine/tests/real_world.rs` walks every DOCX → ODT → DOCX
  and ODT → DOCX → ODT round-trip and reports per-fixture + aggregate
  tag survival. Used as the regression signal for the cross-format
  fixes below.
- **Row height round-trip** — DOCX `<w:trHeight>` ↔ ODT
  `style:row-height` / `style:min-row-height` via new
  `AttributeKey::RowHeight` / `MinRowHeight`.
- **`w:cantSplit` ↔ `fo:keep-together` round-trip** — prevents a table
  row from being broken across pages in either format.
- **`AttributeKey::CodeLanguage`** for fenced-code-block language
  hints (encoded into the paragraph StyleId so DOCX preserves it).
- **Release pipeline publishes to npm** — `release.yml` now runs
  `npm publish --provenance --access public` for `@schnsrw/core` on
  every `vX.Y.Z` tag push, gated by an `NPM_TOKEN` automation token in
  the `release` GitHub Environment.

### Fixed

- **Cross-format DOCX → ODT → DOCX: 53.5 % → 98.2 % raw tag survival.**
  Several discrete fixes:
  - **Style-inherited bold/italic** survives ODT export — the ODT
    `write_run` now calls `doc.resolve_attributes(run_id)` so
    formatting attached via paragraph styles isn't dropped.
  - **`fo:language` / `fo:country`** parse + emit; the `Language`
    attribute now round-trips through both formats.
  - **Cell borders** round-trip via `table-cell-properties` auto-styles
    in the ODT auto-style map (previously parsed but never loaded).
  - **Paragraph-level text-properties inherit to runs** at DOCX write
    time so font name / size / color survive the ODT intermediate
    (`write_run_properties_inherited` + `paragraph_text_defaults`).
  - **Column widths** round-trip in both directions: ODT writer emits
    per-column auto-styles with `style:column-width`; DOCX writer's
    `tblGrid` falls back to table-level `TableColumnWidths` when cells
    don't carry `CellWidth`; DOCX reader parses `w:tblGrid` into
    `TableColumnWidths`.
  - **Cell widths (`w:tcW`)** fall back to column-grid widths after an
    ODT pass so cell-level widths still appear in DOCX output.
  - **Multi-space sequences** encoded as `<text:s text:c="N"/>` per
    ODF §6.1.3 in the ODT writer.
- **Cross-format ODT → DOCX → ODT: 31.7 % → 56.1 % raw tag survival.**
  Same fixes as above plus correct table-row auto-style emission.
- **Markdown — table column alignment round-trips.** The reader
  captures `Tag::Table` column alignments and applies them as
  `Alignment` on each cell's first paragraph; the writer emits
  `:---:` / `---:` / `:---` separator rows accordingly.
- **Markdown — table cells preserve inline formatting.** Cell content
  was previously rendered as plain text, stripping bold / italic /
  code / links inside cells. Cells now go through `write_inline`, and
  literal `|` characters are backslash-escaped.
- **Markdown — fenced code blocks round-trip.** Were being written as
  a single backtick wrapping every line. The reader now marks
  code-block paragraphs with `StyleId="CodeBlock<Lang>"` (e.g.
  `"CodeBlockRust"`); the writer emits a ``` fence with the language
  hint. Language survives the DOCX intermediate via `pStyle`.
- **Markdown — bullet vs. ordered list distinction survives DOCX.**
  Reader now registers a `NumberingDefinitions` entry per list (bullet
  → `•`, decimal → `"%n."`), so the DOCX side ships a backing
  `numbering.xml` and re-parse recovers the correct `ListFormat`.
- **Markdown — list output is tight.** Top-level items no longer
  carry a leading two-space indent; adjacent list items don't get a
  blank separator line.
- **Markdown — nested emphasis emission.** `**bold _italic_ inside**`
  was rendering as `**bold *****italic***** inside**`. The new
  `write_paragraph_runs` uses a marker stack so shared formatting
  (e.g. bold spanning three runs with one italic in the middle) emits
  markers once around the span rather than around each run.
- **Markdown — blockquotes round-trip.** Reader tags paragraphs in
  blockquotes with `StyleId="Quote<N>"`; writer emits `> ` per level.
  Nested blockquotes work because each `Tag::BlockQuote` bumps depth.
- **Markdown — autolinks and link titles round-trip.** Autolinks
  (`<url>`) are re-emitted in `<…>` form instead of `[url](url)`;
  link titles flow through DOCX `<w:hyperlink w:tooltip="…">`.
- **Markdown — inline code with embedded backticks.** Writer now picks
  the minimum fence length that doesn't collide with the content
  (CommonMark §6.1) and pads with spaces when the content begins or
  ends with a backtick.
- **Markdown — task list markers and footnote markers preserved.**
  Enabled `ENABLE_TASKLISTS` + `ENABLE_FOOTNOTES`; the reader emits
  `[ ] ` / `[x] ` for task markers and `[^label]` / `[^label]: ` for
  footnote references and definitions.

### Fixed

- **PDF: embedded DOCX fonts now loaded — Ubuntu and other non-system fonts
  render correctly.** `export(Format::Pdf)` now extracts `.odttf` font files
  from the DOCX preservation package, XOR-deobfuscates each one using the
  `w:fontKey` GUID from `word/fontTable.xml` (reversed byte order per
  ECMA-376 §9.7.3.3), and loads the raw TTF bytes into the `FontDatabase`
  before layout. Documents with embedded fonts (e.g., the calibre `demo.docx`
  which embeds Ubuntu Regular/Bold/Italic/BoldItalic and Ubuntu Mono) now use
  the correct font metrics instead of falling back to Times New Roman.
  (`crates/s1-format-docx/src/reader.rs` — new `extract_embedded_fonts`;
  `crates/s1engine/src/document.rs` — loads them in the PDF export path)

- **Layout: `Font::family_name()` now correctly decodes Mac-platform name
  records.** `ttf_parser::Name::to_string()` only handles Unicode-encoded
  records (Windows platform + Unicode BMP). Many fonts (including Ubuntu)
  list their Mac-platform FAMILY name first; the previous `find()` +
  `and_then(to_string)` pattern took that first record and got `None`,
  falling through to "Unknown". Fixed by scanning all records for each target
  name ID and taking the first that decodes successfully.
  (`crates/s1-text/src/font.rs`)

- **PDF: all embedded content now decodes correctly — black images and garbled
  fonts fixed.** `miniz_oxide::deflate::compress_to_vec` produces raw DEFLATE
  (no zlib wrapper), but PDF's `FlateDecode` filter requires zlib-wrapped
  DEFLATE (RFC 1950). Both font subsets and decoded image pixels were
  compressed with the wrong variant, so strict PDF readers (browser viewers on
  the GitHub Pages demo, Chrome pdfium) failed to decode them — producing
  black image boxes and scrambled text. Replaced all uses with
  `compress_to_vec_zlib` which includes the required 2-byte zlib header and
  Adler-32 trailer. (`crates/s1-format-pdf/src/writer.rs`)

- **PDF: `wordprocessingShape` text boxes now render.** `<wps:wsp>` /
  `<wps:txbx>` inline text boxes were stored as raw XML for round-trip
  preservation but silently dropped from the PDF output. The DOCX parser now
  extracts width, height, stroke color, stroke width, and plain-text content
  from the raw XML (new `AttributeKey::ShapeText`). The layout engine creates
  a new `InlineTextBox` run that occupies the correct space in the line.
  The PDF writer renders the border rectangle (using PDF graphics operators)
  and the text content (using the Helvetica standard font for reliable ASCII
  coverage). `textbox-test.docx` went from 13 → 46 `Tj` ops and 0 → 36 vector
  ops; `wpg-group.docx` from 1 → 6 `Tj`. Aggregate drawing-vanish gap:
  **3× → 1×** (the remaining gap is a pure `prstGeom` rectangle with no text).

- **PDF: glyph advance widths now correct — character overlap fixed.**
  The CIDFont W (width) array was written as a single consecutive range
  starting at the lowest glyph ID in the document. Because glyph IDs from
  text shaping are non-consecutive (gaps are common), intermediate IDs
  received each other's advance widths, causing character overlap and
  irregular spacing in every rendered PDF. Most visible on the GitHub
  Pages demo (WASM / NotoSans fallback) where glyph-ID gaps are larger.
  Fixed by writing one `consecutive(gid, [width])` W entry per glyph so
  each ID maps to exactly its own advance width.

- **Layout: Tables inside headers / footers now render.** The HF
  child-walk previously only processed `Paragraph` nodes, silently
  dropping table-based headers (logo bars, "Name | Date" two-column
  rows) and any images nested inside their cells. The loop now handles
  both `Paragraph` and `Table` children; the collapse-to-single-block
  logic returns the first `Table` block when the header is
  table-driven. (`crates/s1-layout/src/engine.rs`)

- **Layout: empty Drawing anchor nodes no longer inflate line height.**
  `Drawing` / `Image` nodes that carry neither a media reference nor
  explicit dimensions (broken DOCX anchor elements from some producers)
  were emitting a phantom 100×100 pt shaped run into the containing
  paragraph, inflating its line height. These nodes are now skipped
  before the run is pushed. VML shapes that have `ShapeWidth` /
  `ShapeHeight` but no media are still emitted to reserve space.
  (`crates/s1-layout/src/engine.rs`)

- **PDF coverage: corrupt test-fixture PNGs replaced.** Both
  `image-hyperlink.docx` and `oversized-header-image.docx` contained
  a 78-byte PNG with an IDAT CRC mismatch (expected `0x8592550d`,
  actual `0x2772d963`). The PDF writer correctly caught the decode
  error and skipped them, making those fixtures report as
  "images vanish" even though the pipeline itself was fine. Replaced
  with a valid 4×4 RGB PNG. The "images vanish" aggregate in
  `docs/pdf-coverage.md` dropped 2× → 0×.

### Added

- **`pdf_coverage` scorecard** (`crates/s1engine/tests/pdf_coverage.rs`)
  — renders every DOCX + ODT fixture through
  `Document::export(Format::Pdf)` and inspects the resulting PDF via
  `lopdf`: counts pages, `Tj` text-show ops, Image XObjects,
  embedded fonts, vector path ops, then reports per-fixture and
  aggregate gaps. Output also rendered to
  [`docs/pdf-coverage.md`](docs/pdf-coverage.md).
- **EMF / WMF metafile transcoding in PDF export.** A new
  `crates/s1-format-pdf/src/emf.rs` module implements an
  EMF → SVG transcoder covering the GDI record types found in DOCX
  fixtures: geometric primitives (`RECTANGLE`, `ELLIPSE`, `LINETO`,
  `POLYLINE16`, `POLYBEZIER16`), GDI object management
  (`CREATEPEN`, `EXTCREATEPEN`, `CREATEBRUSHINDIRECT`,
  `EXTCREATEFONTINDIRECTW`, `SELECTOBJECT`, `DELETEOBJECT`),
  path brackets (`BEGINPATH` / `FILLPATH` / `STROKEPATH`), text
  (`EXTTEXTOUTW` / `EXTTEXTOUTA`), and embedded DIB bitmaps
  (`BITBLT` / `STRETCHDIBITS`). The SVG is then rasterised to PNG
  via `resvg` and embedded as an Image XObject.
  `issue-319-sections.docx` went from 0 → **13** `Do` ops; all 13
  EMF drawings now embed. The block-level and inline image paths
  both route through the transcoder.

- **Multi-format image support in PDF export.** Image XObjects now
  cover PNG, JPEG / JPG, WebP, BMP, GIF, TIFF, ICO (via the
  `image` crate feature flags) plus SVG (via `resvg` rasterised to
  PNG). EMF / WMF still skip silently — they need a transcoder, not
  a bitmap decoder.

### Fixed

- **DOCX → PDF: inline images now actually embed and render.**
  `crates/s1-format-pdf/src/writer.rs::collect_and_embed_images`
  only recursed into `Image` and `Table` blocks, never into
  `Paragraph` blocks where inline images live (DOCX
  `<wp:inline>` / ODT `<draw:frame>` inside `<text:p>` runs end up
  as `GlyphRun.inline_image`, not as top-level Image blocks).
  Combined with a stray `continue` in `render_line` that silently
  dropped runs carrying an `inline_image`, every inline picture in
  every fixture vanished from the PDF — exactly the user's
  "no images" symptom. Both paths fixed. Scorecard delta on the
  39 DOCX fixtures: **0 / 16 → 10 / 16** image-bearing fixtures
  now emit Image XObjects. Remaining 6 are header/footer images
  on a separate render path (next chunk).
- **DOCX → PDF: text and headers now actually render.**
  `Document::export(Format::Pdf)` was constructing the layout pipeline
  with `FontDatabase::empty()`, so the text-shaping stage produced
  zero glyphs and the resulting PDF was "empty colored tables, no
  text, no images, no headers" (user-reported on 2026-05-22). The
  fix uses `FontDatabase::new()`, which loads system fonts on
  non-WASM and falls back to embedded Noto Sans on WASM. Advanced
  callers can still hand in a custom DB via the public
  `Document::export_pdf(&font_db)` API. The `render_pdf` example was
  extended to support both DOCX and ODT inputs so the manual
  inspection loop works for either format. Visual-fidelity audit and
  the `pdf_coverage` regression test are still pending and tracked
  on the roadmap.

### Added

- **Per-construct fidelity scorecard**
  (`crates/s1engine/tests/fidelity_score.rs`). Walks every DOCX +
  ODT fixture and reports, per construct family (Paragraphs, Runs,
  Tables, DrawingML, VML / legacy drawings, ODF drawings, Text
  boxes, Math, Vectors / SVG primitives, Lists, Footnotes /
  endnotes, Comments, Hyperlinks, Bookmarks, Fields, Tracked
  changes, TOCs, Header / footer references, Section / page
  geometry, Soft formatting), the fraction of input instances that
  survive the round-trip on the no-edit and with-edit lanes.
  Reference numbers as of this commit:
  - DOCX no-edit       100.00% (16 construct families, 22 277 instances)
  - DOCX with-edit     100.00%
  - ODT  no-edit       100.00% (7 construct families on the current
    corpus — expand fixture set to exercise the others)
  - ODT  with-edit     100.00%
  Output is written to `target/fidelity-score.json` and rendered
  to [`docs/fidelity-scorecard.md`](docs/fidelity-scorecard.md).
- **ODT Phase 2b** — per-NodeId body splice. New
  `s1_format_odt::BodyOrigin` mirrors the DOCX one, mapping each
  top-level `<office:text>` child NodeId to its preserved
  `XmlElement`. `Document` carries `odf_body_origin:
  Option<BodyOrigin>`; new constructor
  `Document::from_odt_open_state(model, pkg, origin)`.
  `Engine::open(Odt)` threads the origin through
  `s1_format_odt::read_with_package_and_origin`.
  `Document::export(Odt)` walks the preserved `<office:text>` and
  swaps only the dirty NodeIds with regenerated elements —
  untouched paragraphs / headings / tables keep their preserved
  XmlElement verbatim, so every inline drawing / span / soft page
  break / SVG metadata / sequence declaration rides through
  unchanged. `odt_edit_coverage` body-zero-drop:
  **0 / 4 → 4 / 4**.
- **ODT reader fix** — `parse_content_body` now handles
  `Event::Empty` for body-level `<text:p/>` and `<text:h/>`. New
  helper `insert_empty_paragraph()` creates the model node with
  just its style reference. Without this, self-closing paragraphs
  were silently dropped — owncloud-example.odt's body shrank from
  12 preserved blocks to 8 model children, breaking the 1:1
  alignment required by Phase 2b. After: model body matches the
  preserved body element-for-element.
- **ODT Phase 2a** — non-body parts preserved across edits. Because
  ODF nests `<office:automatic-styles>`, `<office:font-face-decls>`,
  and `<office:scripts>` *inside* `content.xml` (unlike OOXML where
  they live in separate parts), the splice operates at the XmlTree
  tier: regenerated `<office:body>` swaps into the preserved
  content.xml while every sibling section rides through. All
  non-`content.xml` parts (`styles.xml`, `meta.xml`,
  `META-INF/manifest.xml`, `Pictures/*`, `Configurations2/*`,
  `Thumbnails/*`) also preserved via the surrounding package
  clone. New test `crates/s1engine/tests/odt_edit_coverage.rs`
  asserts the contract: **non-body preserved 4 / 4** on edit.
  Dropped tag classes on edit went **20 → 11**: all
  `style:*-properties` (88+60+16+15+10+4x), `style:font-face` 18x,
  `office:font-face-decls`, `office:scripts` recovered. Remaining
  11 are body-internal (`draw:frame`, `text:span`, `text:s`,
  `text:soft-page-break`, `svg:title`/`desc`) — Phase 2b territory.
- **ODT Phase 2** — `Engine::open(Odt)` now keeps the parsed
  `s1_odf::Package` alongside `DocumentModel` via
  `s1_format_odt::read_with_package`, and `Document::export(Odt)`
  re-emits the package verbatim when `model_dirty == false`. New
  constructor `Document::from_model_with_odf_package(model, pkg)`.
  Effect on the coverage scorecard: **`odt_coverage` zero-drop
  4 / 4** (was 0 / 4), contract now asserted in the test.
- **`s1-odf` preservation crate** (`crates/s1-odf/`) — counterpart of
  `s1-ooxml` for the OpenDocument format. Parses any `.odt` / `.ods` /
  `.odp` package into a lossless tree and writes it back. Honours ODF
  specifics: `mimetype` written first as STORED (uncompressed),
  `META-INF/manifest.xml` parsed into a typed [`Manifest`], lenient
  fallback to `Binary` for empty `.xml` parts (e.g.
  `Configurations2/accelerator/current.xml`). Passthrough audit:
  **4 / 4 zero-drop** on the existing `testdocs/odt/` fixture set.
  Zero `s1-model` dependencies — same architectural rule as
  `s1-ooxml`.
- **ODT coverage scorecard** (`crates/s1engine/tests/odt_coverage.rs`)
  — mirror of `docx_coverage`. Establishes the v0.2.x ODT preservation
  baseline: **0 / 4** zero-drop with 20 unique dropped tag classes
  across the existing `testdocs/odt/` fixtures (`draw:frame`,
  `style:*-properties` cascade, `text:span`, `office:font-face-decls`,
  …). Bytes in vs out is dramatic — a 1 MB freetestdata fixture writes
  back at ~4 KB because the current writer regenerates from
  `DocumentModel` and throws away everything outside it. The follow-on
  work is an `s1-odf` preservation crate (analog of `s1-ooxml`).
- `testdocs/odt/realworld/owncloud-example.odt` — first non-synthetic
  ODT fixture, sourced from `owncloud/example-files`.
- **`render_pdf` example** (`crates/s1engine/examples/render_pdf.rs`)
  for manually inspecting `Document::export(Pdf)` output. Used during
  the diagnosis of the user-reported PDF fidelity gap.

### Known issues (tracked, not yet fixed)

- **DOCX → PDF: `wordprocessingShape` text boxes vanish** (3 fixtures).
  `<wps:wsp>` / `<wps:txbx>` shapes (inline text boxes, positioned
  shape groups) have no `<a:blip r:embed>` and no equivalent raster
  path. They are stored as raw XML for round-trip fidelity but are not
  yet rendered to the PDF. Tracked in the roadmap; EMF/WMF transcoding
  is a related blocker for the anchor-image path.

## [0.1.0] — 2026-05-21

### Added

- **`s1-ooxml` preservation crate.** Parses any OOXML package (DOCX, XLSX,
  PPTX) into a lossless tree and writes it back. 39/39 of the consumer's
  fixtures round-trip with zero tag loss. See
  [`docs/ooxml-design.md`](docs/ooxml-design.md).
- **DOCX preservation through `s1engine::Document`.** `Engine::open(Docx)`
  now keeps the parsed `s1-ooxml::Package` alongside the projected
  `DocumentModel`. `Document::export(Docx)` re-emits the package
  verbatim while it's still intact — zero-drop round-trip for the
  converter use case.
- **Phase 2a — preservation survives edits.** Mutations no longer drop
  the preservation package. `export(Docx)` splices a regenerated
  `word/document.xml` into a clone of the preserved package, so theme,
  fontTable, customXml, headers, footers, footnotes, endnotes,
  comments, numbering, styles, images, rels, and content types all
  ride through a Casual Editor save unchanged.
- **Phase 2b — body preservation under edits.** New `BodyOrigin`
  side-table (`s1-format-docx::body_origin`) maps each top-level body
  NodeId to its preserved `s1_ooxml::XmlElement` at parse time.
  `Document` tracks `dirty_body_ids: HashSet<NodeId>` plus a
  `body_structural_dirty` flag; `apply_transaction` classifies each
  operation's `target_id` and climbs it to its top-level body
  ancestor. `export(Docx)` walks the preserved body — clean NodeIds
  stay byte-equal, dirty NodeIds swap in regenerated elements. Every
  unknown OOXML inside untouched paragraphs and tables (DrawingML,
  VML, SDT blocks, AlternateContent fallbacks, MathML) survives an
  edit-and-save round-trip. `docx_edit_coverage` body-zero-drop:
  10/39 → 39/39.
- **DOCX coverage scorecard** (`crates/s1engine/tests/docx_coverage.rs`)
  — three-bucket matrix against the 39 eigenpal fixtures, rendered to
  `docs/docx-coverage.md`.
- **DOCX edit-path coverage** (`crates/s1engine/tests/docx_edit_coverage.rs`)
  — counterpart of the above that exercises the with-edits splice path
  and asserts both non-body preservation (Phase 2a) and body tag-census
  preservation (Phase 2b) on every fixture.
- Mirrored 39 DOCX fixtures from the consumer's
  `docx-editor/e2e/fixtures` into `testdocs/docx/eigenpal/` so both
  sides measure against the same gold set.
- [`docs/integration-plan.md`](docs/integration-plan.md) — phased
  migration plan for the consumer (docx-editor / Casual Editor).
- [`docs/testing-strategy.md`](docs/testing-strategy.md) — the test
  categories that gate every phase.

### Changed

- `s1engine::Document` now carries `Option<s1_ooxml::Package>`, an
  `Option<BodyOrigin>`, a `dirty_body_ids: HashSet<NodeId>` set, and a
  `body_structural_dirty: bool` flag in addition to `model_dirty: bool`.
  New methods: `has_preservation()`, `is_dirty()`,
  `invalidate_preservation()`, `preservation()`,
  `from_model_with_package(...)`, `from_open_state(...)`.
- `s1-format-docx` exposes `reader::read_with_package(bytes) ->
  (DocumentModel, Package)` and `reader::read_with_package_and_origin(bytes)
  -> (DocumentModel, Package, BodyOrigin)` so consumers can keep all
  halves of the preservation bridge.

### Foundation (earlier in 0.1.x cycle)

- Initial public release of Casual Core, the document engine for Casual
  Office.
- Apache-2.0 licensing across all crates.
- Minimal WASM converter surface: `detect_format`, `convert`,
  `convert_to_string`, `extract_text`.
- `@schnsrw/core` npm package scaffolding.
- GitHub Pages demo at `schnsrw.github.io/core/`.
- Pure-Rust crates for model, operations, format readers/writers,
  layout, text shaping.

[Unreleased]: https://github.com/schnsrw/core/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/schnsrw/core/releases/tag/v0.1.0
