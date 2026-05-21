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
- [ ] ODT — same playbook against an ODT fixture set.

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
