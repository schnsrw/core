# Changelog

All notable changes to **Casual Core** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
