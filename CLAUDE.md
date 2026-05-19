# Casual Core — AI Development Context

## What this repo is

**Casual Core** is the document engine that powers the **Casual Office** suite
(Casual Editor, Casual Sheet, …). It is a pure-Rust workspace plus WebAssembly
and C FFI bindings, focused on three things:

1. **Reading** documents — DOCX, ODT, Markdown, plain text.
2. **Writing / converting** documents — to any of the above plus PDF (export only).
3. **Embedding** — consumers (TS editors, Go services, CLIs) call Casual Core
   through the WASM or C FFI layer.

This repo is **not** an editor. The editor lives elsewhere and treats Casual
Core as a black-box converter at the file-open / file-save boundary.

## Read these first

1. `README.md` — entry point, supported formats, build commands.
2. `crates/s1-model/src/lib.rs` — the document AST is the contract; everything
   else converts to/from it.
3. `ffi/wasm/src/lib.rs` — the WASM surface, which is what most consumers see.
4. `js/src/index.ts` — the TypeScript layer (`@schnsrw/core`) — small,
   stable, framework-free.

## Architecture rules (MUST follow)

### 1. The document model is sacred
- `s1-model` has **zero external dependencies** — pure Rust data structures only.
- Every node has a globally unique `NodeId(replica_id, counter)`.
- Internal representation is never exposed in the public API.

### 2. All mutations go through operations
- Never modify the document tree directly.
- All changes are an `Operation` applied via `s1-ops`.
- Every `Operation` implements `invert()` so undo is free.

### 3. Format isolation
- Each `s1-format-*` crate depends **only** on `s1-model`.
- Format crates never depend on each other.
- Format crates never depend on `s1-ops` or `s1-layout`.

### 4. No panics in library code
- All public functions return `Result<T, Error>`.
- No `.unwrap()` or `.expect()` outside tests.
- Be lenient when parsing (warn on unknown elements), strict when writing
  (always emit valid output).

### 5. Errors are typed
- `thiserror` for derivation.
- Each crate has its own error type, convertible to `s1engine::Error`.
- Errors carry context — file position, node id, format element.

## Coding conventions

- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test --workspace`
  before every PR. CI enforces all three.
- `&str` over `String` in parameters. `impl Into<String>` for builder methods
  that take ownership.
- Derive `Debug, Clone, PartialEq` on public types where it makes sense.
- `#[non_exhaustive]` on public enums that may grow variants.
- Public items get `///` doc comments. Use `# Errors` sections.
- TypeScript in `js/`: strict mode, no `any` in public types, prefer
  `Uint8Array` over `ArrayBuffer` at API boundaries.

## Adding a new format

1. Create `crates/s1-format-<name>/`, depending only on `s1-model` + `thiserror`.
2. Implement `FormatReader::read(&[u8]) -> Result<Document, Error>` and/or
   `FormatWriter::write(&Document) -> Result<Vec<u8>, Error>`.
3. Add round-trip tests against fixtures in `tests/fixtures/`.
4. Wire it into `s1engine` behind a feature flag (`name = ["dep:s1-format-<name>"]`).
5. Update `js/src/types.ts`'s `Format` union and `formats.ts`'s
   `SUPPORTED_CONVERSIONS` matrix.
6. Add it to the `README.md` status table.

## Editor / collaboration scope

Real-time collaboration, CRDTs, and live editing UX **are not in this repo**.
That logic belongs in the consumer editors (Casual Editor, Casual Sheet),
which use Yjs or equivalent on top of Casual Core's pure-function converter
API.

If a request would add a CRDT, an HTTP server, a UI component, or a websocket
to this repo, push back — that's a different repo.

## What NOT to do

- Don't reintroduce `s1-crdt` or `s1-format-xlsx`. They were removed when the
  repo was scoped down to pure conversion.
- Don't add async to the Rust API. Consumers wrap it.
- Don't depend on a JS framework from `js/` — it stays framework-free.
- Don't break the `s1-model` zero-dependency rule.
- Don't add `unsafe` without a comment explaining why.
- Don't skip tests for "simple" code — round-trip cases are where bugs hide.

## Releases

- `v0.x` while the public API is still moving.
- Tag `vX.Y.Z` on `main`, the release workflow publishes `@schnsrw/core` to
  npm and creates a GitHub release.
- GitHub Pages deploys the demo at `https://schnsrw.github.io/core/` on
  every push to `main`. Custom domain can be wired via `demo/public/CNAME`.
