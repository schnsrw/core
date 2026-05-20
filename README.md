# Casual Core

> Document engine for the [Casual Office](https://schnsrw.live) suite.

A pure-Rust engine that reads, writes, and converts office documents — DOCX,
ODT, PDF, Markdown, plain text — with WebAssembly and C FFI bindings for use
from any consuming app.

Casual Core sits underneath everything in Casual Office:

- **Casual Editor** (`doc.schnsrw.live`) — collaborative document editor.
- **Casual Sheet** (`sheet.schnsrw.live`) — spreadsheet editor.
- **Casual Core** (this repo) — the format and conversion engine they share.

## Status

`v0.1.0` · pre-release · workspace builds and passes 1,116 tests on CI.

| Format | Read | Write |
| --- | --- | --- |
| DOCX | ✓ | ✓ |
| ODT  | ✓ | ✓ |
| Markdown | ✓ | ✓ |
| Plain text | ✓ | ✓ |
| PDF  | – | ✓ (export only) |

### DOCX fidelity (measured against the 39-fixture consumer set)

| Path | Zero-drop |
| --- | --- |
| Open DOCX → save DOCX (no edits) | **39 / 39** |
| Open DOCX → save DOCX (edits, non-body parts) | **39 / 39** |
| Open DOCX → save DOCX (edits, body content) | 10 / 39 — Phase 2b target |

The preservation layer (`crates/s1-ooxml/`) keeps theme, fontTable,
customXml, headers/footers, footnotes, endnotes, comments, numbering,
styles, images, rels, and content-types intact across the consumer's
save path. See [`docs/integration-plan.md`](docs/integration-plan.md)
for the migration plan and [`docs/docx-coverage.md`](docs/docx-coverage.md)
for the live scorecard.

## Quick start — JavaScript

```bash
npm install @schnsrw/core
```

```ts
import { init, convert } from "@schnsrw/core";

await init();
const docx = await fetch("/cv.docx").then((r) => r.arrayBuffer());
const pdf  = await convert(new Uint8Array(docx), { to: "pdf" });
```

The full JS surface (five functions) is in [`docs/api.md`](docs/api.md).

## Quick start — Rust

```toml
[dependencies]
s1engine = "0.1"
```

```rust
use s1engine::{Engine, Format};

let engine = Engine::new();
let doc = engine.open(&bytes_in)?;
let pdf = doc.export(Format::Pdf)?;
```

## Repo layout

```
crates/        Pure-Rust workspace
  s1-model       Zero-dep document AST
  s1-ops         Operations / transactions / undo (internal)
  s1-ooxml       OOXML preservation layer — lossless package read/write
  s1-format-*    Per-format readers and writers
  s1-convert     Cross-format conversion + legacy .doc reader
  s1-layout      Layout / pagination (used by PDF export)
  s1-text        Text shaping (rustybuzz, ttf-parser, fontdb)
  s1engine       Facade crate
ffi/
  wasm           wasm-bindgen bindings — minimal converter API
  c              C FFI bindings
js/            @schnsrw/core — TypeScript wrapper over the WASM
demo/          GitHub Pages reference demo
docs/          Requirements, architecture, roadmap, API, fidelity policy
fuzz/          cargo-fuzz harnesses
tests/         Workspace-level integration + fidelity tests
testdocs/      Real-world fixture documents
```

## Documentation

Start here:

- [`docs/requirements.md`](docs/requirements.md) — what Casual Core is for
- [`docs/architecture.md`](docs/architecture.md) — how the layers fit together
- [`docs/api.md`](docs/api.md) — the JS, WASM, and Rust public API
- [`docs/roadmap.md`](docs/roadmap.md) — what's next, what's deliberately out
- [`docs/fidelity.md`](docs/fidelity.md) — round-trip policy and known gaps
- [`CLAUDE.md`](CLAUDE.md) — repo rules for AI development assistants

## Build

```bash
cargo build --workspace
cargo test  --workspace

wasm-pack build ffi/wasm --target web --release
cd js && npm install && npm run build
cd demo && npm install && npm run dev   # http://localhost:5173
```

## Architectural rules (must-follow)

1. `s1-model` has **zero external dependencies**.
2. Format crates depend only on `s1-model` (and `thiserror`).
3. All document mutations go through `s1-ops::Operation` internally.
4. Library code never panics — every public function returns `Result`.

Full rules in [`CLAUDE.md`](CLAUDE.md) and [`docs/architecture.md`](docs/architecture.md).

## License

Apache-2.0. See [`LICENSE`](LICENSE).
