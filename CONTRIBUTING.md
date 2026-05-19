# Contributing to Casual Core

Thanks for considering a contribution. Casual Core is the engine underneath
the [Casual Office](https://schnsrw.live) suite, so changes here ripple to
Casual Editor, Casual Sheet, and any downstream consumer.

## Ground rules

- Open an issue before a non-trivial PR so we can align on scope.
- Keep PRs focused — one concern per PR.
- Apache-2.0 licensed; by submitting you agree your contribution is too.

## Local setup

```bash
git clone git@github.com:schnsrw/core.git
cd core

# Rust toolchain
rustup toolchain install stable
rustup target add wasm32-unknown-unknown

# WASM tooling
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Node (for js/ and demo/)
node --version  # >= 18
```

## Day-to-day

```bash
# Rust workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test  --workspace

# WASM bindings
wasm-pack build ffi/wasm --target web --release

# TS layer
cd js && npm install && npm run typecheck && npm run build:ts

# Demo (drop-a-file converter)
cd demo && npm install && npm run dev
```

CI runs the same commands on every push.

## Things to read before changing

- `CLAUDE.md` — architectural rules. The four "MUST follow" rules are
  enforced by reviewers, not just by clippy.
- `crates/s1-model/src/lib.rs` — the AST is the contract.
- `js/src/index.ts` — the public TS surface. Keep it small.

## What's in scope

✓ New format readers/writers (DOCX, ODT, PDF, MD, TXT, and friends).  
✓ Better fidelity on existing formats — measured against `testdocs/`.  
✓ Performance work on the parsers / writers / layout.  
✓ Improvements to the TS layer (`@schnsrw/core`) that don't add framework deps.

## What's out of scope

✗ Real-time collaboration / CRDTs. Lives in the consumer editor.  
✗ A web UI or editor. Lives in [Casual Editor](https://github.com/Rudra-Office/Rudra-Editor) or downstream.  
✗ Networking, HTTP servers, websockets.  
✗ Async Rust APIs at the engine boundary.

## Commits and PRs

- Conventional-ish messages preferred (`feat:`, `fix:`, `docs:`), not required.
- One PR per concern. A 2,000-line PR that touches five subsystems will be
  asked to be split.
- A green CI is the bar — `fmt`, `clippy -D warnings`, `test`, `typecheck`,
  WASM build.

## Reporting bugs

Use the [bug report template](.github/ISSUE_TEMPLATE/bug_report.md). For
fidelity issues, please attach (or link) the offending input file — without
a reproducer there's no fixable bug.
