# Casual Core — AI development context

## What this repo is

**Casual Core** is the document engine that powers the **Casual Office**
suite. Pure Rust workspace + WebAssembly + C FFI, focused on:

1. **Reading** documents — DOCX, ODT, Markdown, plain text.
2. **Writing / converting** documents — to any of the above plus PDF (export only).
3. **Embedding** — consumers (TS editors, Go services, CLIs) call Casual Core
   through the WASM or C FFI layer.

This repo is **not** an editor. The editor lives elsewhere and treats Casual
Core as a black-box converter at the file-open / file-save boundary.

## Read these first

1. [`README.md`](README.md) — entry point.
2. [`docs/requirements.md`](docs/requirements.md) — what we're building, for whom.
3. [`docs/architecture.md`](docs/architecture.md) — layers, design rules, what
   lives where.
4. [`docs/api.md`](docs/api.md) — the JS, WASM, and Rust public surfaces.
5. [`docs/roadmap.md`](docs/roadmap.md) — what's next, what's deliberately out.
6. [`crates/s1-model/src/lib.rs`](crates/s1-model/src/lib.rs) — the AST is
   the contract; everything else converts to/from it.
7. [`ffi/wasm/src/lib.rs`](ffi/wasm/src/lib.rs) — ~70 LOC, the entire WASM
   surface. If you can't fit your change here, it probably belongs lower
   in the stack.

## Architecture rules (MUST follow)

These are non-negotiable. Reviewers enforce them.

### 1. The document model is sacred

- `s1-model` has **zero external dependencies**.
- Every node has a globally unique `NodeId(replica_id, counter)`.
- The internal representation is never exposed in the public API.

### 2. Format isolation

- Each `s1-format-*` crate depends **only** on `s1-model` (and `thiserror`).
- Format crates never depend on each other.
- Format crates never depend on `s1-ops` or `s1-layout`.
- Exception: `s1-format-pdf` depends on `s1-layout` + `s1-text` because PDF
  is rendered output, not a parse target. It is export-only.

### 3. All mutations go through operations

- Never modify the document tree directly.
- All changes are an `Operation` applied via `s1-ops::Transaction`.
- Every `Operation` implements `invert()` so undo is free.
- This is internal — the WASM API never exposes mutation primitives.

### 4. No panics in library code

- All public functions return `Result<T, Error>`.
- No `.unwrap()` or `.expect()` outside tests.
- Parsers are **lenient** (warn on unknown markup, drop, continue).
- Writers are **strict** (always emit valid output).

### 5. Errors are typed

- `thiserror` for derivation.
- Each crate has its own error type, convertible to `s1engine::Error`.
- Errors carry context — file position, node id, format element.

## Coding conventions

- `cargo fmt`, `cargo clippy`, `cargo test --workspace` before every PR.
  CI enforces fmt and test; clippy is advisory until the inherited backlog
  is cleared (see roadmap `v1.0` task).
- `&str` over `String` in parameters. `impl Into<String>` for builders.
- Derive `Debug, Clone, PartialEq` on public types where it makes sense.
- `#[non_exhaustive]` on public enums that may grow variants.
- Public items get `///` doc comments. `# Errors` sections on fallible fns.
- TypeScript in `js/`: strict mode, no `any` in public types, prefer
  `Uint8Array` over `ArrayBuffer` at API boundaries.

## The WASM surface — minimal by design

The entire JS-facing API is **five functions**: `init`, `convert`,
`convertToString`, `detectFormat`, `extractText`. The WASM Rust side is
roughly 70 LOC. This is deliberate.

If you're tempted to add an `editParagraph`, `setBold`, `getNodeById`, or
similar editor-style call to the WASM layer, **stop**. That belongs in the
consumer editor, not here. The boundary is bytes-in / bytes-out.

## Adding a new format

1. Create `crates/s1-format-<name>/`, depending only on `s1-model` + `thiserror`.
2. Implement `FormatReader::read(&[u8]) -> Result<Document, Error>` and/or
   `FormatWriter::write(&Document) -> Result<Vec<u8>, Error>`.
3. Add round-trip tests against fixtures in `testdocs/<name>/`.
4. Wire it into `s1engine` behind a feature flag.
5. Add the `Format` variant to `crates/s1engine/src/format.rs` (extension,
   MIME type, is_document/spreadsheet/presentation classification).
6. Update [`docs/requirements.md`](docs/requirements.md)'s format table and
   the README status table.
7. Update `js/src/types.ts`'s `Format` union.

## What NOT to do

- Don't reintroduce CRDT or spreadsheet crates. They were removed when the
  repo was scoped down to pure conversion.
- Don't expand the WASM API surface. Five functions is the budget; bigger
  surface goes to the consumer editor.
- Don't add async to the Rust API. Consumers wrap it.
- Don't depend on a JS framework from `js/` — it stays framework-free.
- Don't break the `s1-model` zero-dependency rule.
- Don't add `unsafe` without a comment explaining why.
- Don't skip tests for "simple" code — round-trip cases are where bugs hide.

## Commits and git workflow

Commits in this repo are authored as the user, with no AI co-author trailer.
Don't add `Co-Authored-By: Claude …` or any AI attribution. Commit messages
focus on the *why* in 1–2 sentences, with a short bullet list of *what* if
needed.

## Releases

- `v0.x` while the public API is still moving.
- Tag `vX.Y.Z` on `main`; the `release.yml` workflow publishes
  `@schnsrw/core` to npm and creates a GitHub release.
- GitHub Pages auto-deploys the demo at `https://schnsrw.github.io/core/`
  on every push to `main`. Custom subdomain (e.g. `core.schnsrw.live`) wires
  via a `demo/public/CNAME` file.
