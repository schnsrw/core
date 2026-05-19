# Casual Core

> Document engine for the [Casual Office](https://schnsrw.live) suite.

A pure-Rust engine that reads, writes, and converts office documents — DOCX,
ODT, PDF, Markdown, plain text — with bindings for WebAssembly (browser, Node,
Bun, Deno) and C.

Casual Core sits underneath everything in Casual Office:

- **Casual Editor** (`doc.schnsrw.live`) — collaborative document editor.
- **Casual Sheet** (`sheet.schnsrw.live`) — spreadsheet editor.
- **Casual Core** (this repo) — the format and conversion engine they share.

## Status

`v0.1.0` · pre-release · workspace builds and passes 1,200+ tests on CI.

| Format | Read | Write |
| --- | --- | --- |
| DOCX | ✓ | ✓ |
| ODT  | ✓ | ✓ |
| Markdown | ✓ | ✓ |
| Plain text | ✓ | ✓ |
| PDF  | – | ✓ (export only) |

## Repo layout

```
crates/        Pure-Rust workspace
  s1-model       Zero-dep document AST
  s1-ops         Operations, transactions, undo
  s1-format-*    Per-format readers/writers
  s1-convert     Cross-format conversion pipelines
  s1-layout      Layout / pagination engine
  s1-text        Text shaping (rustybuzz, ttf-parser, fontdb)
  s1engine       Facade crate
ffi/
  wasm           wasm-bindgen bindings
  c              C FFI (cbindgen)
js/            @schnsrw/core — TypeScript layer over the WASM build
demo/          GitHub Pages demo (drop-in file converter)
fuzz/          cargo-fuzz harnesses
tests/         Workspace-level integration + fidelity tests
testdocs/      Real-world fixture documents
```

## Use it from JavaScript

```bash
npm install @schnsrw/core
```

```ts
import { init, convert } from "@schnsrw/core";

await init();

const docx = await fetch("/cv.docx").then((r) => r.arrayBuffer());
const pdf  = await convert(new Uint8Array(docx), { from: "docx", to: "pdf" });
```

See [`js/README.md`](js/README.md) for the full API.

## Use it from Rust

```toml
[dependencies]
s1engine = "0.1"
```

```rust
use s1engine::Engine;

let engine = Engine::new();
let doc = engine.open(&bytes_in)?;
let pdf = doc.export_pdf()?;
```

## Build

```bash
# Rust
cargo build --workspace
cargo test  --workspace

# WASM
wasm-pack build ffi/wasm --target web --release

# JS layer
cd js && npm install && npm run build

# Demo
cd demo && npm install && npm run dev
```

## Architectural rules

1. `s1-model` has **zero external dependencies**.
2. Format crates depend only on `s1-model`.
3. All document mutations go through `s1-ops::Operation`.
4. Library code never panics — every public function returns `Result`.

See [`CLAUDE.md`](CLAUDE.md) for the full rules and project context.

## License

Apache-2.0. See [`LICENSE`](LICENSE).
