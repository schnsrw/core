# Changelog

All notable changes to **Casual Core** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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

- **DOCX → PDF visual fidelity ~2 / 100** (user-reported 2026-05-22).
  The layout + PDF pipeline (`crates/s1-layout/`,
  `crates/s1-format-pdf/`) does not currently produce a faithful
  rendering of real-world DOCX inputs. No visual-fidelity test gates
  this path — the existing `docx_coverage` / `docx_edit_coverage`
  contracts only check DOCX → DOCX. Pipelined on
  `docs/roadmap.md`.

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
