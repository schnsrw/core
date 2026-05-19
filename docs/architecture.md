# Casual Core — architecture

## Layers

```
┌──────────────────────────────────────────────────────────────┐
│ Consumer  (Casual Editor, Casual Sheet, external apps)      │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼  (npm: @schnsrw/core)
┌──────────────────────────────────────────────────────────────┐
│ js/                                                          │
│ Thin TypeScript wrapper. ~100 LOC.                          │
│ Loads WASM, normalises inputs to Uint8Array, types the API. │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼  (wasm-bindgen)
┌──────────────────────────────────────────────────────────────┐
│ ffi/wasm/                                                    │
│ Function-style WASM surface: detect_format, convert,        │
│ convert_to_string, extract_text. ~70 LOC.                    │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│ crates/s1engine/                                             │
│ Facade. Engine + Document. Routes to format crates by Format.│
└──────────────────────────────────────────────────────────────┘
                              │
       ┌──────────┬───────────┼───────────┬──────────┬─────────┐
       ▼          ▼           ▼           ▼          ▼         ▼
┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────┐ ┌─────────┐ ┌────────┐
│ s1-      │ │ s1-      │ │ s1-      │ │ s1-    │ │ s1-     │ │ s1-    │
│ format-  │ │ format-  │ │ format-  │ │format- │ │format-  │ │convert │
│ docx     │ │ odt      │ │ pdf      │ │ md     │ │ txt     │ │(.doc → │
│          │ │          │ │ (export) │ │        │ │         │ │ .docx) │
└──────────┘ └──────────┘ └──────────┘ └────────┘ └─────────┘ └────────┘
       │          │           │           │          │         │
       └──────────┴───────────┼───────────┴──────────┴─────────┘
                              ▼
                  ┌──────────────────────┐
                  │ crates/s1-model      │
                  │ Zero-dep document AST│
                  └──────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
   ┌────────────────────┐         ┌────────────────────┐
   │ crates/s1-layout   │         │ crates/s1-text     │
   │ (used by PDF export│         │ (used by layout    │
   │  only)             │         │  for shaping)      │
   └────────────────────┘         └────────────────────┘
```

`crates/s1-ops` lives alongside `s1-model` and provides the operation /
transaction primitives Document uses internally. It is not exposed through
the WASM API.

## Core design rules

These are non-negotiable. Reviewers enforce them.

### 1. The model is sacred

- `s1-model` has **zero external dependencies**. Pure Rust standard library
  types only. No serde, no thiserror, no anyhow.
- Every node has a globally-unique `NodeId(replica_id, counter)`.
- The internal representation never leaks through the public API.

### 2. Format isolation

- Each `s1-format-*` crate depends on `s1-model` and `thiserror`. Nothing else.
- Format crates never depend on each other.
- Format crates never depend on `s1-ops`, `s1-layout`, or `s1-text`.
- `s1-format-pdf` is the one exception: it depends on `s1-layout` and `s1-text`
  for PDF export, since rendering needs layout. It is **export-only**.

### 3. All mutations go through operations

- The Document tree is never mutated in place.
- All changes are an `Operation` applied via `s1-ops::Transaction`.
- Every `Operation` implements `invert()`, so undo is free.
- This is internal — the WASM API exposes none of it.

### 4. No panics in library code

- All public functions return `Result<T, Error>`.
- `.unwrap()` and `.expect()` are forbidden outside tests.
- Parsing is **lenient** (warn and continue on unknown markup).
- Writing is **strict** (always emit valid output that round-trips).

### 5. Typed, contextful errors

- `thiserror` everywhere.
- Each crate has its own error type, convertible to `s1engine::Error`.
- Errors carry context — file position, node id, format element.

## The WASM boundary

Casual Core is consumed across a JS↔WASM boundary. The boundary is crossed
**exactly twice per conversion** (once for input bytes, once for output
bytes). This is deliberate — JS↔WASM is expensive and the editor consumers
need predictable performance.

What this means in practice:

- The WASM API is **stateless and function-style**, not object-oriented.
  There is no `WasmEngine` / `WasmDocument` to instantiate from JS.
- The Rust side owns the model. JS never receives a document handle.
- Errors cross the boundary as `JsError`, with the underlying Rust error
  text preserved.

## What lives where

| Concern | Crate |
| --- | --- |
| Document AST (paragraph, run, table, image, …) | `s1-model` |
| Operations + transactions + undo | `s1-ops` |
| DOCX reading and writing | `s1-format-docx` |
| ODT reading and writing | `s1-format-odt` |
| PDF export (no reading) | `s1-format-pdf` |
| Markdown reading and writing | `s1-format-md` |
| Plain-text reading and writing | `s1-format-txt` |
| Legacy `.doc` binary reader and DOC→DOCX pipeline | `s1-convert` |
| Page layout (lines, pages, hyphenation) | `s1-layout` |
| Text shaping, font db, line breaking, bidi | `s1-text` |
| Engine facade (`Engine`, `Document`, `Format`) | `s1engine` |
| WASM bindings (`detect_format`, `convert`, …) | `ffi/wasm` |
| C FFI bindings | `ffi/c` |
| TypeScript wrapper (`@schnsrw/core` on npm) | `js/` |
| Smoke-test demo (GitHub Pages) | `demo/` |
| Fuzzing harnesses | `fuzz/` |
| Workspace-level tests + fidelity audit | `tests/` |
| Sample documents for testing | `testdocs/` |

## What does NOT live here

- Anything UI-shaped.
- A document editor.
- A collaborative editing protocol or CRDT.
- A spreadsheet engine.
- A network or HTTP layer.

If a request would add any of the above, push back — that's a different repo.
