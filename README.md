<div align="center">

# Casual Core

**A pure-Rust document engine for reading, writing, and converting office files.**

DOCX · ODT · PDF · Markdown · Plain Text — with WebAssembly and C FFI bindings.

[![CI](https://github.com/schnsrw/core/actions/workflows/ci.yml/badge.svg)](https://github.com/schnsrw/core/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/@schnsrw/core.svg)](https://www.npmjs.com/package/@schnsrw/core)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-brightgreen.svg)](https://schnsrw.github.io/core/)

[**Live demo**](https://schnsrw.github.io/core/) ·
[**Documentation**](docs/) ·
[**npm**](https://www.npmjs.com/package/@schnsrw/core) ·
[**Issues**](https://github.com/schnsrw/core/issues)

</div>

---

## Why Casual Core

Most document conversion libraries treat each format in isolation. Casual Core unifies them
behind one **typed document model** and one **minimal API**, so you can:

- **Convert any office document** to any other writable format — bytes in, bytes out.
- **Surface a structured model** to a custom editor without parsing XML yourself.
- **Round-trip without loss** for the constructs your users actually edit (paragraphs,
  styles, tables, images, headers, lists).
- **Stay self-contained**: a single ~3.3 MB gzipped WASM bundle covers every
  format. Framework-free TypeScript wrapper, zero
  runtime dependencies in the core model.

It powers the [Casual Office](https://schnsrw.live) suite (a collaborative editor and
spreadsheet) but ships as an independent library you can use anywhere a JS, Node, Rust,
or C app needs a document pipeline.

## Features

- ✅ **Read** DOCX, ODT, Markdown, plain text
- ✅ **Write** DOCX, ODT, PDF, Markdown, plain text
- ✅ **Round-trip preservation** of styles, tables, images, headers/footers, lists,
  footnotes, comments, tracked changes (DOCX), revisions (ODT)
- ✅ **JSON model surface** — open as JSON, edit in your editor, write back
- ✅ **WebAssembly bindings** for browsers, Node, Bun, Deno
- ✅ **C FFI** for native consumers
- ✅ **No panics** in library code; every public function returns `Result`
- ✅ **Zero external dependencies** in the core document model

## Format support

| Format       | Read | Write | Notes                                     |
| :----------- | :--: | :---: | :---------------------------------------- |
| DOCX         |  ✓   |   ✓   | Lossless preservation of unmodified parts |
| ODT          |  ✓   |   ✓   | Same-format round-trip at parity with DOCX |
| Markdown     |  ✓   |   ✓   | CommonMark + GFM (tables, tasks, strikethrough) |
| Plain text   |  ✓   |   ✓   | UTF-8                                     |
| PDF          |  –   |   ✓   | Export only — render via internal layout engine |

## Quick start

### JavaScript / TypeScript

```bash
npm install @schnsrw/core
```

**Convert a document:**

```ts
import { init, convert } from "@schnsrw/core";

await init();
const docx = await fetch("/cv.docx").then(r => r.arrayBuffer());
const pdf  = await convert(new Uint8Array(docx), { to: "pdf" });
```

**Edit through the structured JSON model:**

```ts
import { init, openToModel, convertModel } from "@schnsrw/core";

await init();
const docx  = await fetch("/report.docx").then(r => r.arrayBuffer());
const model = await openToModel(new Uint8Array(docx), "docx");

// `model.nodes` is a Record<string, S1Node> keyed by "replica:counter".
// Mutate any node's text or attributes, then write back to bytes.

const odt = await convertModel(model, { to: "odt" });
```

The full TypeScript API is five functions (`init`, `convert`, `convertToString`,
`detectFormat`, `extractText`) plus the model-surface pair (`openToModel`,
`convertModel`). See [`docs/api.md`](docs/api.md) for the complete reference.

### Rust

```toml
[dependencies]
s1engine = "0.1"
```

```rust
use s1engine::{Engine, Format};

let engine = Engine::new();
let doc    = engine.open(&bytes_in)?;
let pdf    = doc.export(Format::Pdf)?;
```

### CLI

A simple converter CLI ships as an example:

```bash
cargo run --example convert -- input.docx output.pdf
```

## Status

`v0.1.x` — pre-release while the public API stabilises.
Workspace currently builds and passes **1,135 tests** on CI.

### Fidelity (structural, tag-census level)

Per-construct survival across the test corpus
([`docs/fidelity-scorecard.md`](docs/fidelity-scorecard.md)):

| Format | Lane         | Constructs covered                | Survival   |
| :----- | :----------- | :-------------------------------- | ---------: |
| DOCX   | no-edit      | 16 families · 22 277 input tags   | **100.00 %** |
| DOCX   | with-edit    | same 16 families                  | **100.00 %** |
| ODT    | no-edit      | 7 families · 361 input tags       | **100.00 %** |
| ODT    | with-edit    | same 7 families                   | **100.00 %** |

### Cross-format round-trip

Tag survival when converting through an intermediate format
([`real_world.rs::cross_format_fidelity_audit`](crates/s1engine/tests/real_world.rs)):

| Path                  | Fixtures | Survival     |
| :-------------------- | -------: | -----------: |
| DOCX → ODT → DOCX     |       34 |  **98.2 %**  (raw tag count) |
| ODT  → DOCX → ODT     |        3 |  **56.1 %**  (raw tag count) |
| Markdown → DOCX → MD  |        8 |  **95.9 %**  (word multiset)  |

The ODT → DOCX → ODT figure is dominated by source files that emit one named
auto-style per paragraph; Casual Core deduplicates them to the unique property
blocks they share. Body content (paragraphs, runs, tables, font/size/color,
language, cell borders, column widths) all survives.

### Known gaps

- **DOCX → PDF visual fidelity** — text, images, tables, borders, page geometry,
  header/footer tables, text boxes, EMF/WMF graphics, and embedded fonts all
  render. Tracked in [`docs/pdf-coverage.md`](docs/pdf-coverage.md). Pure
  `prstGeom` shapes with no text content (1 fixture) remain pending.
- **ODT cross-format fidelity** — see the table above; we deliberately
  deduplicate auto-styles, so the raw tag count is lower while the rendered
  output is equivalent.
- **Markdown edge cases** — reference-style links flatten to inline, nested
  emphasis with arbitrary combinations may pick a different (but valid) marker
  order.

## Architecture

```
                ┌─────────────────────┐
                │     consuming app   │
                │ (editor, CLI, etc.) │
                └──────────┬──────────┘
                           │
   ┌─── JS ────┐    ┌──── C ─────┐
   │ @schnsrw/ │    │ ffi/c      │
   │ core      │    │            │
   └──────┬────┘    └─────┬──────┘
          │               │
          └──── WASM ─────┘
                  │
          ┌───────▼────────┐
          │   s1engine     │  facade
          └───────┬────────┘
                  │
   ┌──────────────┼───────────────┐
   ▼              ▼               ▼
s1-format-*  s1-layout       s1-ooxml
(readers /   (pagination,    (lossless DOCX
 writers)    PDF render)      preservation)
   │
   ▼
s1-model  ◄── zero-dep typed document tree
```

| Layer                | Responsibility                                                 |
| :------------------- | :------------------------------------------------------------- |
| `s1-model`           | The typed document tree. Zero external dependencies.           |
| `s1-format-*`        | One reader/writer pair per format. Depend only on `s1-model`.  |
| `s1-ooxml` / `s1-odf` | Lossless package layers. Preserve untouched parts byte-perfect. |
| `s1-layout`          | Paged layout for PDF export.                                   |
| `s1-text`            | Shaping (rustybuzz · ttf-parser · fontdb).                     |
| `s1engine`           | Public Rust facade. The crate consumers actually depend on.    |
| `ffi/wasm`           | WebAssembly bindings (5-function API surface).                 |
| `ffi/c`              | C FFI bindings.                                                |
| `js/`                | `@schnsrw/core` — framework-free TypeScript wrapper.           |

### Architectural rules

1. `s1-model` has **zero external dependencies**.
2. Format crates depend only on `s1-model` (and `thiserror`).
3. All document mutations go through `s1-ops::Operation` internally.
4. Library code never panics — every public function returns `Result`.

Full rules and rationale: [`CLAUDE.md`](CLAUDE.md) ·
[`docs/architecture.md`](docs/architecture.md).

## Repository layout

```
crates/        Pure-Rust workspace
  s1-model       Zero-dep document AST
  s1-ops         Operations / transactions / undo (internal)
  s1-ooxml       OOXML preservation layer
  s1-odf         ODF preservation layer
  s1-format-*    Per-format readers and writers
  s1-convert     Cross-format conversion + legacy .doc reader
  s1-layout      Layout / pagination (used by PDF export)
  s1-text        Text shaping
  s1engine       Public facade crate
ffi/
  wasm           wasm-bindgen bindings — minimal converter API
  c              C FFI bindings
js/            @schnsrw/core — TypeScript wrapper over the WASM
demo/          GitHub Pages reference demo
docs/          Requirements, architecture, roadmap, API, fidelity
fuzz/          cargo-fuzz harnesses
testdocs/      Real-world fixture documents
```

## Building from source

**Rust workspace:**

```bash
cargo build --workspace
cargo test  --workspace
```

**WebAssembly bundle:**

```bash
wasm-pack build ffi/wasm --target web --release
```

**TypeScript wrapper:**

```bash
cd js && npm install && npm run build
```

**Demo (browser):**

```bash
cd demo && npm install && npm run dev   # http://localhost:5173
```

## Documentation

| Document | Contents |
| :--- | :--- |
| [`docs/requirements.md`](docs/requirements.md) | What Casual Core is for, what's in / out of scope |
| [`docs/architecture.md`](docs/architecture.md) | How the layers fit together |
| [`docs/api.md`](docs/api.md) | JS, WASM, and Rust public surfaces |
| [`docs/roadmap.md`](docs/roadmap.md) | What's next |
| [`docs/fidelity.md`](docs/fidelity.md) | Round-trip policy and known gaps |
| [`docs/fidelity-scorecard.md`](docs/fidelity-scorecard.md) | Per-construct survival, regenerated each test run |
| [`docs/docx-coverage.md`](docs/docx-coverage.md) | DOCX coverage matrix |
| [`docs/pdf-coverage.md`](docs/pdf-coverage.md) | PDF export coverage |
| [`docs/integration-plan.md`](docs/integration-plan.md) | Migration plan for editor integration |
| [`CLAUDE.md`](CLAUDE.md) | Repo rules for AI development assistants |

## Contributing

Issues and pull requests are welcome. Before opening a PR:

1. Run `cargo fmt --all` and `cargo test --workspace`.
2. Avoid `unwrap` / `expect` outside tests — library code returns `Result`.
3. New formats: read [`docs/architecture.md`](docs/architecture.md) §"Adding a new format".
4. Round-trip-relevant changes: regenerate
   [`docs/fidelity-scorecard.md`](docs/fidelity-scorecard.md) and call out any deltas.

Smaller bug reports, fixture additions, and documentation improvements are also
high-leverage contributions.

## Releases

`vX.Y.Z` tags on `main` trigger
[`.github/workflows/release.yml`](.github/workflows/release.yml), which:

1. Builds the WASM bundle and TypeScript wrapper.
2. Attaches a tarball to a new GitHub Release.
3. Publishes `@schnsrw/core` to npm (with `--provenance --access public`).

The live demo at <https://schnsrw.github.io/core/> auto-deploys from `main` via
[`pages.yml`](.github/workflows/pages.yml).

## License

Apache-2.0 — see [`LICENSE`](LICENSE).
