# Casual Core — testing strategy

The fidelity tests are the gate for every step of the
[integration plan](integration-plan.md). This document defines the
categories, what each one proves, and where the code lives.

## Test categories

### 1. Unit / property tests (per crate)

Already in place. Each crate has its own `#[cfg(test)] mod tests` blocks
and `proptest` cases for invariants. CI runs `cargo test --workspace` on
every push.

**Status:** healthy. 1,100+ tests passing.

---

### 2. Round-trip fidelity

For every fixture document, run:

```
bytes_in → s1engine::Engine::open → Document → export → bytes_out
bytes_out → s1engine::Engine::open → Document
```

Assert that `Document(bytes_in)` and `Document(bytes_out)` agree on:

- Paragraph count
- Run count
- Plain-text content (whitespace-normalised)
- Heading structure (kind + text)
- Table count, row count, cell count
- Image count
- Section count

**Lives in:** `crates/s1engine/tests/roundtrip_fixtures.rs`
**Covers:** all readable formats — DOCX, ODT, MD, TXT
**Fixtures:** `testdocs/<fmt>/` plus the mirrored eigenpal set at
`testdocs/docx/eigenpal/`

A regression is a fixture that *used* to round-trip and now drops content.
A new gap is a fixture that has always failed and is now flagged.

---

### 3. Lossy-tag tracking (DOCX / ODT only)

For each ZIP-based fixture:

1. Extract `document.xml` from the input zip.
2. Walk it; collect the set of `(namespace, local-name)` tags.
3. Parse → write → re-zip. Extract `document.xml` from the output.
4. Walk; collect the output tag set.
5. Report `input_tags - output_tags` — the tags that disappeared.

The report becomes per-fixture metadata. We don't fail the build on lost
tags — we *track* them so we know which classes still need work. The
roadmap milestone for `v0.2.x` (fidelity pass) targets driving the
combined loss count to zero on the eigenpal set.

**Lives in:** `crates/s1engine/tests/lossy_tags.rs`
**Output:** `target/lossy-tags-report.json` — consumable by the CI badge
and the GitHub issue tracker.

---

### 4. Conversion matrix

For every pair `(from, to)` where both formats are supported in their
respective directions, take each fixture and check:

```
text_in  = plain_text(bytes_in)
bytes_mid = convert(bytes_in, from, to)
bytes_back = convert(bytes_mid, to, from)  (when reversible)
text_out = plain_text(bytes_back)

assert similarity(text_in, text_out) >= threshold
```

`similarity` is a normalised Levenshtein ratio. The threshold depends on
the conversion pair:

| Pair | Threshold | Why |
| --- | --- | --- |
| DOCX → DOCX | 1.00 | Lossless |
| ODT → ODT | 1.00 | Lossless |
| DOCX ↔ ODT | 0.95 | Faithful, some formatting drift |
| DOCX → MD | 0.85 | MD loses formatting, keeps prose |
| MD → DOCX → MD | 0.90 | Headings + lists preserved |
| any → TXT | text identity | only prose survives |
| any → PDF | n/a | not reversible, text extraction tested separately |

**Lives in:** `crates/s1engine/tests/conversion_matrix.rs`

---

### 5. Eigenpal parity (cross-engine)

The consumer (docx-editor / Casual Editor) has 43 DOCX fixtures and its
own roundtrip-audit script. We mirror those fixtures into
`testdocs/docx/eigenpal/` and run Casual Core against the same set. The
report compares:

- Per-fixture: does Casual Core round-trip with ≥ the same fidelity as
  the eigenpal pipeline?
- Aggregate: how many of the 43 are clean for each engine?

This is the metric the integration plan's phase gates depend on.

**Lives in:** `crates/s1engine/tests/eigenpal_parity.rs`
**Output:** `target/eigenpal-parity.json` — included in CI artefacts.

---

### 6. Performance smoke

Wraps each common operation in `Instant::now()` and asserts an upper
bound. The bounds are deliberately loose — they're guard-rails against
catastrophic regressions, not benchmark targets. Real benchmarking lives
in `crates/s1engine/benches/`.

| Operation | Fixture | Bound on Apple Silicon |
| --- | --- | --- |
| Open small DOCX (~100 KB) | `freetestdata_100kb.docx` | < 50 ms |
| Open medium DOCX (~500 KB) | `freetestdata_500kb.docx` | < 200 ms |
| Open large DOCX (~1 MB) | `freetestdata_1mb.docx` | < 500 ms |
| DOCX → PDF (small) | small fixture | < 250 ms |
| Text extraction (large) | 1 MB fixture | < 100 ms |

CI is on x86 GitHub runners — bounds are multiplied by 3× there to absorb
the slower hardware.

**Lives in:** `crates/s1engine/tests/perf_smoke.rs`
**Output:** logged in CI; regressions fail the run.

---

### 7. Hostile-input robustness

Existing tests (`crates/s1engine/tests/hostile_inputs.rs`) cover this.
Continues to grow with each parser bug we find. Fuzz harnesses in
`fuzz/` complement it.

**Status:** ongoing.

---

## Running the suite

```bash
# Everything
cargo test --workspace

# Just fidelity
cargo test --package s1engine --test roundtrip_fixtures
cargo test --package s1engine --test lossy_tags
cargo test --package s1engine --test conversion_matrix
cargo test --package s1engine --test eigenpal_parity
cargo test --package s1engine --test perf_smoke

# Generate the integration scorecard
./scripts/fidelity-audit.sh
```

## How the scorecard maps to migration gates

Phase advances in the [integration plan](integration-plan.md) require
specific scorecard thresholds. `scripts/fidelity-audit.sh` outputs a
JSON summary that maps directly onto the gate table — green ticks for
metrics that meet the bar, red for the laggards. CI fails the build if a
previously-green metric regresses.
