# Security policy

## Reporting a vulnerability

Email **schnsrw@gmail.com** with the details. Please do not open a public
GitHub issue for a security bug.

Include:

- A description of the issue and where in the code it lives.
- A reproducer (input file or code snippet), if possible.
- Whether the issue affects the Rust crates, the WASM bindings, or the TS
  layer.

I aim to acknowledge reports within a few days and ship a fix as soon as
the issue is reproduced.

## Supported versions

Only the latest published version of `@schnsrw/core` (and the matching
Rust crates) receives security fixes during the `0.x` series.

## Scope

Casual Core processes untrusted document bytes. Memory safety and panic-free
parsing are the two properties that matter most. Issues that crash the WASM
runtime on hostile input qualify; aesthetic round-trip lossiness does not.
