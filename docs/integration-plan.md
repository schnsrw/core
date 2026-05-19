# Casual Core ⇄ Casual Editor integration plan

How [Casual Editor](https://github.com/Rudra-Office/Rudra-Editor) (the
docx-editor fork at `services/document/docx-editor/`) will adopt Casual Core
without breaking what already works.

## Core principle

**The editor never stops working.** xml-js stays as the primary DOCX parser
on day one. Casual Core gets added in piece by piece, behind feature flags,
gated by a per-feature coverage matrix. Migration is reversible until the
last step.

**DOCX comes first. Other formats wait.** The consumer is a DOCX editor;
the highest-impact integration is on the DOCX path. We close the DOCX
coverage gap before we even touch ODT/MD/TXT/PDF interop.

---

## The three-bucket DOCX coverage matrix

Every OOXML element class lands in one of three buckets. Phase planning is
organised around closing buckets in order.

```
┌─────────────────────────────────────────────────────────────┐
│ Consumer (Casual Editor / docx-editor) DOCX support         │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │
         ┌────────────────────┴───────────────────────┐
         │                                            │
         ▼                                            ▼
   Bucket A: matched                          Bucket B: we lead
   Both repos handle it                       We support, they drop
                              ▲
                              │ symmetric difference
                              │
                              ▼
                       Bucket C: joint gap
                       Neither supports today
```

### Bucket A — match the consumer

OOXML element classes the editor handles today. Casual Core MUST handle
each one before it can serve as the editor's primary parser for that class.

This bucket is the integration gate. Closing it = readiness.

### Bucket B — we lead

Element classes Casual Core handles that the editor drops on round-trip
(per their published audit). These are wins we bring to the integration:
the moment Casual Core takes over parsing for any of these classes,
fidelity improves for editor users.

### Bucket C — joint gap

Neither handles. Documented but not gated on. Future work — likely
multi-quarter (track changes, OMML math, advanced fields).

### How buckets are populated

1. **Their support set** is read off the published
   [eigenpal audit](https://github.com/eigenpal/docx-editor) and our local
   copy at `services/document/docx-editor/roundtrip-audit-report.md`. The
   "Global rollup" table lists the tags they parse-but-drop; everything not
   in that list is in their supported set.
2. **Our support set** is computed by the round-trip test suite. For each
   fixture in `testdocs/docx/eigenpal/` (a mirror of the consumer's
   fixture set), we extract the XML tag census in vs. out, and any tag
   that round-trips with the same count is "supported."
3. **The buckets are the set diff**. The test suite writes the matrix to
   `target/docx-coverage.json`; a CI step renders it as a markdown report
   in `docs/docx-coverage.md`.

See [`testing-strategy.md`](testing-strategy.md) for the implementation.

---

## The phases

### Phase 0 — today

Both repos exist independently. Casual Core builds, ships an npm package,
deploys a demo. Casual Editor uses its own xml-js pipeline. No integration
code anywhere.

**Done when:** `@schnsrw/core` is published to npm. ✅ (in progress)

---

### Phase 1 — populate the bucket matrix

No editor changes. Just produce the DOCX coverage report.

- Wire the round-trip + lossy-tag suite against the 39 eigenpal fixtures.
- Publish `docs/docx-coverage.md` on every push.
- File issues for every entry in Bucket A (we lag the consumer).

**Done when:** the matrix is real, current, and CI-published. We know
exactly which element classes need work before Bucket A closes.

---

### Phase 2 — close Bucket A

Per element class in Bucket A, in editor-frequency order:

1. Identify the failing tags (`w:foo`, `wp14:bar`, …).
2. Trace through `crates/s1-format-docx/src/{content_parser, property_parser, writer}.rs`.
3. Either *parse correctly* (if currently dropped) or *write correctly*
   (if parsed but not re-emitted).
4. Re-run the suite, watch the bucket move.

**Done when:** Bucket A is empty. Casual Core round-trips every DOCX the
consumer round-trips.

This is *the* milestone for integration readiness. Until Bucket A is
empty, the consumer cannot safely shadow-parse with Casual Core.

---

### Phase 3 — DOCX shadow parsing in the editor

Casual Core runs *alongside* xml-js on every DOCX import. Output not
consumed — only diffed and logged. Confirms in production that what the
fixture set tells us is also true on real traffic.

- After `parseDocx(bytes)` in the editor, call
  `convert(bytes, { from: "docx", to: "docx" })` and re-parse.
- Diff: paragraph count, run count, table structure, text content.
- Log dev-mode; opt-in telemetry in prod.

**Done when:** Shadow diff is clean across the editor's top-traffic
fixture similarity bucket for two consecutive releases.

---

### Phase 4 — element-level migration

Switch specific OOXML element families from xml-js to Casual Core. Easiest
first (Bucket B wins — instant fidelity bump for the consumer):

Suggested order:

1. **Bucket B wins (do these first — they're wins the moment we ship):**
   - Any class where eigenpal drops and we don't.
2. **Bucket A core:**
   1. Paragraph properties (alignment, spacing, indent)
   2. Run properties (bold, italic, color, font, size)
   3. Styles + style table
   4. Tables (simple → merged → nested)
   5. Lists + numbering
   6. Headers + footers
   7. Images (inline → floating)
   8. Sections, page properties
3. **Bucket C (longest tail — multi-quarter):**
   - Fields (`w:fldChar`, `w:instrText`)
   - Track changes
   - Form controls
   - Math (OMML)

Each step is one PR in the editor repo, behind a feature flag,
independently revertible.

**Done when:** every Bucket A and Bucket B class is served by Casual Core.

---

### Phase 5 — xml-js retirement

Once every relevant element class is migrated and the shadow diff has been
clean for two consecutive releases, remove xml-js from the editor.
`parseDocx` and `repackDocx` become thin wrappers around Casual Core.
Bundle size drops dramatically.

---

### Phase 6 — non-DOCX formats

Only now do we widen to ODT / MD / TXT input and PDF / ODT / MD / TXT
export through Casual Core. Same playbook: build the coverage matrix per
format, close it, integrate.

---

## What gates promotion between phases

| Metric | Phase 2 | Phase 3 | Phase 4 | Phase 5 |
| --- | --- | --- | --- | --- |
| Bucket A — element classes outstanding | 0 | 0 | 0 | 0 |
| DOCX round-trip — text content preserved | ≥ 95 % | ≥ 99 % | 100 % | 100 % |
| DOCX round-trip — paragraph count match | ≥ 95 % | ≥ 99 % | 100 % | 100 % |
| Lossy-tag report — no new losses vs xml-js | yes | yes | yes | yes |
| Perf — Casual Core ≤ 2× xml-js wall time | — | yes | yes | yes |

Each metric is produced by the test suite in
[`testing-strategy.md`](testing-strategy.md). Regressions fail the CI
build.

## Rollback story

At every phase, the previous phase's code path stays in the editor behind a
flag. If Casual Core regresses on a class, flip the flag, ship the fix,
re-enable.

## Open questions

1. **Bundle size budget.** Casual Core WASM is ~3.2 MB unzipped, ~900 KB
   gzipped. Fine for desktop; lazy-load on mobile?
2. **Sync vs async at the editor boundary.** xml-js is sync; Casual Core
   is async. Phase 3 forces an `await` into the file-open path — needs
   careful review.
3. **Shadow telemetry endpoint.** Where does Phase 3's diff go? Log line
   for v1; metrics pipeline later.
