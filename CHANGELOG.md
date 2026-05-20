# Changelog

All notable changes to **Casual Core** will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- **DOCX coverage scorecard** (`crates/s1engine/tests/docx_coverage.rs`)
  — three-bucket matrix against the 39 eigenpal fixtures, rendered to
  `docs/docx-coverage.md`.
- **DOCX edit-path coverage** (`crates/s1engine/tests/docx_edit_coverage.rs`)
  — counterpart of the above that exercises the with-edits splice path
  and asserts non-body preservation on every fixture.
- Mirrored 39 DOCX fixtures from the consumer's
  `docx-editor/e2e/fixtures` into `testdocs/docx/eigenpal/` so both
  sides measure against the same gold set.
- [`docs/integration-plan.md`](docs/integration-plan.md) — phased
  migration plan for the consumer (docx-editor / Casual Editor).
- [`docs/testing-strategy.md`](docs/testing-strategy.md) — the test
  categories that gate every phase.

### Changed

- `s1engine::Document` now carries `Option<s1_ooxml::Package>` plus a
  `model_dirty: bool` flag. New methods: `has_preservation()`,
  `is_dirty()`, `invalidate_preservation()`, `preservation()`,
  `from_model_with_package(...)`.
- `s1-format-docx` exposes `reader::read_with_package(bytes) ->
  (DocumentModel, Package)` so consumers can keep both halves of the
  preservation bridge.

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

[Unreleased]: https://github.com/schnsrw/core/commits/main
