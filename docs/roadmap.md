# Casual Core — roadmap

Living document. Targets are guidelines, not contracts. Open issues
override anything written here.

## Now — `v0.1.x`

Status: just shipped the initial public commit. Stabilising.

- [x] Strip the editor / server / CRDT / spreadsheet weight inherited from
      the old `doc-engine` repo.
- [x] Apache-2.0 relicensing.
- [x] Minimal WASM converter surface (`detect_format`, `convert`,
      `convert_to_string`, `extract_text`).
- [x] `@schnsrw/core` npm package scaffolding.
- [x] GitHub Pages demo as a manual smoke-test surface.
- [ ] Green CI on every push (Rust + JS + WASM + Pages).
- [ ] First published `0.1.0` npm release.

## Next — `v0.2.x` — fidelity pass

Goal: round-trip the existing `testdocs/` set without content loss.

- [ ] Wire `tests/fidelity/` into CI so every push reports the drop rate.
- [ ] Establish a fidelity floor: regressions fail the build.
- [ ] Drive the DOCX round-trip drop rate to zero on the bundled fixtures.
- [ ] Same for ODT.
- [ ] Audit the dropped tag classes documented in the old eigenpal
      roundtrip-audit report: fields, advanced numbering, page-number
      properties, text-box wrap properties.

## Then — `v0.3.x` — performance + hostile input

- [ ] Stream the DOCX reader (don't hold the full XML in memory). The
      current `s1-format-docx` parser is in-memory; streaming is a known
      target. See `crates/s1-format-docx/src/streaming.rs`.
- [ ] Add `cargo-fuzz` harnesses under CI on a nightly schedule.
- [ ] Benchmark a 500-page DOCX → PDF conversion and publish a number.
- [ ] Cap memory at ≤ 4× input size during conversion.

## Later — `v1.0` — API freeze

- [ ] Re-tighten clippy to `-D warnings` after the inherited backlog is fixed.
- [ ] Lock down the JS public API surface (`init`, `convert`,
      `convertToString`, `detectFormat`, `extractText`).
- [ ] Lock down the Rust public API surface (`Engine`, `Document`, `Format`,
      `Error`).
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
