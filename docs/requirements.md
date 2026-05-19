# Casual Core — requirements

## Problem

Office documents come in many formats (DOCX, ODT, PDF, Markdown, plain text)
and need to move between them — for editing, archiving, conversion, search
indexing. Most browser-grade implementations are either tied to a specific
editor (OnlyOffice, OnlyOffice's sdkjs, Collabora) or rely on server-side
LibreOffice headless conversion.

Casual Office (`schnsrw.live`) needs an engine that:

1. Runs **in the browser** (WASM), in Node/Bun/Deno, and as a native library.
2. Is **small enough to ship over the wire** to end users — not a 50 MB
   LibreOffice container.
3. Is **format-agnostic on the consumer side**: the consuming editor only
   needs to call `convert(bytes, from, to)`.

Casual Core fills that gap.

## Users

Casual Core is consumed, not used directly. Its users are *other repos in the
Casual Office suite* and any third party that needs an in-process document
converter:

| Consumer | What they need |
| --- | --- |
| **Casual Editor** (`doc.schnsrw.live`) | DOCX in → DOCX out; PDF export; Markdown/TXT export for sharing. |
| **Casual Sheet** (`sheet.schnsrw.live`) | Not served by Casual Core. Sheet uses its own engine. |
| External users | Same DOCX/ODT/PDF/MD/TXT conversion via npm or a Rust crate. |

There are no human-facing UI requirements. The GitHub Pages demo at
`schnsrw.github.io/core/` exists only as a smoke test and reference.

## Functional requirements

### MUST

- **Read** DOCX, ODT, Markdown, plain text from `Uint8Array` / `&[u8]`.
- **Write** DOCX, ODT, PDF, Markdown, plain text.
- **Auto-detect** the input format from bytes (magic-byte sniff).
- **Convert** between any read-supported format and any write-supported format,
  in a single function call, with no intermediate persistence.
- **Extract plain text** from a document, format-aware.
- Expose all of the above as a minimal WASM API: `detect_format`, `convert`,
  `convert_to_string`, `extract_text`. No stateful objects.

### SHOULD

- Round-trip DOCX → DOCX without losing the markup the original used.
- Round-trip ODT → ODT in the same way.
- Stream large documents without holding two full copies in memory.
- Preserve images and tables through every supported conversion pair.

### MAY (post-1.0)

- Read legacy `.doc` (binary Word). The `s1-convert` crate has a partial
  reader; it is not exposed through the default WASM API.
- Read `.rtf`. There's a fixture set in `testdocs/rtf/` but no reader yet.
- Read `.html`. Not on the roadmap; out of scope unless a consumer asks.

### MUST NOT

- Provide an editor UI. That belongs in consumer repos.
- Provide live collaboration. CRDTs were removed from this repo on purpose.
- Provide spreadsheet support. Casual Sheet handles spreadsheets.
- Add network or HTTP code. Conversion is a pure function on bytes.

## Non-functional requirements

| Property | Target |
| --- | --- |
| WASM bundle size | ≤ 3 MB gzipped (current trimmed surface should clear this comfortably) |
| Cold start | First `convert()` call ≤ 250 ms in modern browsers, including WASM compile |
| Conversion throughput | A 100-page DOCX → PDF in ≤ 1 s on an M1-class laptop |
| Memory | ≤ 4× the input document size during conversion |
| Crash safety | No panics on hostile input — every public function returns `Result` |
| License | Apache-2.0 throughout; no AGPL or GPL dependencies |

## Acceptance criteria

A release of Casual Core is shippable when:

1. `cargo test --workspace` is green across every crate.
2. `wasm-pack build ffi/wasm --target web --release` produces a working
   bundle, validated by the GitHub Pages demo.
3. The fixture set in `testdocs/` round-trips through DOCX→DOCX and ODT→ODT
   without loss of content (formatting drift is acceptable; content loss is not).
4. The `@schnsrw/core` npm package builds, typechecks, and exposes only the
   minimal converter surface (`init`, `convert`, `convertToString`,
   `detectFormat`, `extractText`).
