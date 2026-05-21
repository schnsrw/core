# Casual Core — roadmap

Living document. Targets are guidelines, not contracts. Open issues
override anything written here.

## Now — `v0.1.x` — bootstrap

Status: scaffolding done, CI green, foundation laid.

- [x] Strip the editor / server / CRDT / spreadsheet weight inherited from
      the old `doc-engine` repo.
- [x] Apache-2.0 relicensing.
- [x] Minimal WASM converter surface (`detect_format`, `convert`,
      `convert_to_string`, `extract_text`).
- [x] `@schnsrw/core` npm package scaffolding.
- [x] GitHub Pages demo as a manual smoke-test surface.
- [x] Green CI on every push (Rust + JS + WASM + Pages).
- [ ] First published `0.1.0` npm release.

## Now — `v0.2.x` — DOCX fidelity pass

The big rock. Detailed plan in
[`integration-plan.md`](integration-plan.md) — phases tracked there.

- [x] Mirror eigenpal's 39 DOCX fixtures into `testdocs/docx/eigenpal/`.
- [x] DOCX coverage scorecard (`tests/docx_coverage.rs`) — three-bucket
      matrix on every push, results in `docs/docx-coverage.md`.
- [x] **Phase 2** — `s1-ooxml` preservation layer.
      `crates/s1-ooxml/` parses any OOXML package losslessly and writes
      it back. 39/39 fixtures round-trip with zero tag loss in
      `tests/passthrough.rs`.
- [x] **Phase 2 wire-up** — `s1engine::Document` carries an
      `Option<s1_ooxml::Package>` preservation field; `export(Docx)`
      re-emits it verbatim while preservation is intact.
      Bucket A collapsed 140 → 0 on the no-edits path; Bucket B grew
      0 → 22 (we now lead the consumer on tags they're known to drop).
- [x] **Phase 2a** — preservation survives edits. The package is kept
      across mutation; `export(Docx)` splices the regenerated
      `word/document.xml` into a clone of the preserved package so
      every other part (theme, fontTable, customXml, headers, footers,
      footnotes, endnotes, comments, numbering, styles, images, rels,
      content types) rides through unchanged. New test
      `tests/docx_edit_coverage.rs` is the regression gate.
- [x] **Phase 2b** — body preservation under edits. `BodyOrigin`
      side-table built at parse time maps each top-level body NodeId
      to its preserved `XmlElement`; `Document` tracks
      `dirty_body_ids: HashSet<NodeId>` populated by `apply_transaction`
      from each op's `target_id` (climbed to its top-level body
      ancestor). `export(Docx)` walks the preserved body; clean
      NodeIds stay byte-equal, dirty NodeIds swap in the regenerated
      element. `docx_edit_coverage` body-zero-drop went 10/39 → 39/39;
      all 162 previously-dropped unknown body tags (`a:*`, `mc:*`,
      `wps:*`, `w14:*`) survive edits now.
- [ ] ODT — same playbook against an ODT fixture set. Progress:
  - [x] **Coverage scorecard** (`odt_coverage`): baseline 0 / 4
        zero-drop with 20 unique dropped tag classes on the existing
        `testdocs/odt/` fixtures.
  - [x] **`s1-odf` preservation crate** (`crates/s1-odf/`) — analog
        of `s1-ooxml`. Honours ODF specifics (mimetype-first STORED,
        `META-INF/manifest.xml`, lenient parse for empty `.xml`
        parts). Passthrough audit: **4 / 4 zero-drop** on the
        existing fixture set.
  - [x] **Phase 2 wire-up** — `Engine::open(Odt)` keeps the
        `s1_odf::Package` alongside `DocumentModel`; `export(Odt)`
        re-emits verbatim when `model_dirty == false`. `odt_coverage`
        body-zero-drop jumped 0 / 4 → **4 / 4**; bytes_in ≈ bytes_out
        (1 MB freetestdata fixture now writes back at ~1 MB instead
        of ~4 KB). Contract is asserted, not just reported.
  - [x] **Phase 2a** — non-body parts ride through edits.
        `Document::export(Odt)` on a dirty document splices at the
        XmlTree tier: regenerated `<office:body>` swaps into the
        preserved `content.xml` while sibling sections
        (`<office:automatic-styles>`, `<office:font-face-decls>`,
        `<office:scripts>`, `<office:settings>`) ride through.
        Surrounding parts (`styles.xml`, `meta.xml`,
        `META-INF/manifest.xml`, `Pictures/*`, `Configurations2/*`,
        `Thumbnails/*`) also preserved via the package clone.
        Dropped tag classes on edit went **20 → 11** (the styles
        cascade — `style:*-properties` 88+60+16+15+10+4x,
        `style:font-face` 18x, `office:font-face-decls`,
        `office:scripts` — all recovered).
  - [ ] Phase 2b — per-NodeId body splice (BodyOrigin for ODT).
        Remaining 11 dropped classes are all body-internal
        (`draw:frame` 10x, `text:span` 8x, `text:s` 29x,
        `text:soft-page-break` 11x, `svg:title`/`desc`, …).
- [ ] **DOCX → PDF visual fidelity** — user-reported ~2/100 fidelity
      on the rendered PDF (2026-05-22). Path: `s1-format-docx::read` →
      `DocumentModel` → `s1-layout::layout` → `s1-format-pdf::write_pdf`.
      Currently no visual-fidelity test gates this — `docx_coverage` /
      `docx_edit_coverage` only measure DOCX ↔ DOCX. Plan: ship a
      `pdf_coverage` test that compares rendered PDF text/structure
      against the source DOCX, identify the largest-impact gaps
      (fonts? images? tables? page geometry?), then close them.

## Then — `v0.3.x` — performance + hostile input

- [ ] Stream the DOCX reader (don't hold the full XML in memory). The
      current `s1-format-docx` parser is in-memory; streaming is a known
      target. See `crates/s1-format-docx/src/streaming.rs`.
- [x] `cargo-fuzz` harnesses for the parse + edit + export surfaces.
      Eight targets including `fuzz_ooxml_package` (Phase 2 preservation
      tier) and `fuzz_docx_phase2b` (Phase 2b origin table + per-NodeId
      splice). `.github/workflows/fuzz-nightly.yml` runs each target for
      5 minutes at 03:17 UTC daily; crashes upload as artefacts.
- [x] 500-page DOCX → PDF benchmark — `pdf_500_pages` bench group,
      runs the full layout + shaping + PDF pipeline. Reference number
      ~244 ms median on Apple Silicon (range 221–274 ms across
      10 samples). See [`docs/testing-strategy.md`](testing-strategy.md).
- [ ] Cap memory at ≤ 4× input size during conversion.

## Later — `v1.0` — API freeze

- [ ] Re-tighten clippy to `-D warnings` after the inherited backlog is
      fixed.
- [ ] Lock down the JS public API surface (`init`, `convert`,
      `convertToString`, `detectFormat`, `extractText`).
- [ ] Lock down the Rust public API surface (`Engine`, `Document`,
      `Format`, `Error`).
- [ ] Cut `1.0.0` once the API is stable and fidelity is green.

## Out of scope, forever

These are explicit non-goals. Don't propose them — the answer is "different
repo".

- Editor UI of any kind.
- Real-time collaboration / CRDTs.
- Spreadsheets, presentations.
- Server / HTTP / WebSocket code.
- Async Rust API at the engine boundary.

## Tracking

Live work tracking lives in GitHub Issues on `schnsrw/core`, not here.
