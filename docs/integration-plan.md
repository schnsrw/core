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

### Phase 0 — repos coexist · ✅ done

Both repos build and ship independently. Casual Core publishes
`@schnsrw/core`, deploys its demo to GitHub Pages, has the WASM bundle in
the 1 MB-gzipped range. Casual Editor uses its own xml-js pipeline. No
integration code anywhere yet.

---

### Phase 1 — populate the bucket matrix · ✅ done

No editor changes. Just produce the DOCX coverage report against the
consumer's fixture set.

- Mirror 39 DOCX fixtures from `docx-editor/e2e/fixtures` into
  `testdocs/docx/eigenpal/`. ✅
- Wire `crates/s1engine/tests/docx_coverage.rs` to produce the
  three-bucket scorecard. ✅
- Publish `docs/docx-coverage.md` on every push. ✅

Baseline reported in `docx-coverage.md`:

```
Bucket A — consumer supports, we drop : 140 tags
Bucket B — we support, consumer drops :   0 tags
Bucket C — neither supports            :  22 tags
```

---

### Phase 2 — close Bucket A · ✅ done

The 140-tag Bucket A turned out to be an **architectural** gap, not a
backlog of per-tag implementations. Hand-coding each tag (5+ months of
work) was the wrong tool. The right one was a **preservation layer**:

- `crates/s1-ooxml/` parses an OOXML package into a lossless tree and
  writes it back. ✅ ([`ooxml-design.md`](ooxml-design.md))
- `s1engine::Document` carries an `Option<s1_ooxml::Package>` as
  preservation metadata, populated by `Engine::open(Docx)`. ✅
- `Document::export(Docx)` has three lanes, gated on whether the
  package is intact and whether the model is dirty (Phase 2a, below). ✅

The Phase 2 gate is met. Re-running the engine-level coverage audit on
the same 39 fixtures (no edits):

```
Bucket A — consumer supports, we drop :   0 tags  (was 140)
Bucket B — we support, consumer drops :  22 tags  (was   0)
Bucket C — neither supports            :   0 tags  (was  22)
Zero-drop round-trip                   : 39 / 39
```

Casual Core now matches or beats Casual Editor on every tag in the
consumer's fixture set. **Integration is unblocked.**

---

### Phase 2a — edits preserve non-body parts · ✅ done

The first Phase 2 milestone gave zero-drop round-trip *only when no edits
happened*. The second milestone keeps the preservation package across
edits, splicing in a regenerated `word/document.xml` while leaving
every other part untouched.

`Document::export(Docx)` lanes:

1. **No edits + preservation** — re-emit the package verbatim. Zero-drop.
2. **Edits + preservation** — regenerate `word/document.xml` from the
   model, swap into a clone of the preserved package, write.
3. **No preservation** — model-only writer (legacy path).

New regression test `docx_edit_coverage` on the same 39 fixtures:

```
non-body parts preserved (Phase 2a)  : 39 / 39   ✓ contract met
body zero-drop                        : 39 / 39   ← see Phase 2b
```

Every theme, font, customXml, footnote, header, comment, embedded
image, style, numbering definition, and relationship table survives a
Casual Editor save through `@schnsrw/core` — even when the body is
modified.

---

### Phase 2b — body preservation under edits · ✅ done

Body unknowns inside `word/document.xml` used to drop on edit because
the body was regenerated wholesale from `DocumentModel`. Phase 2b
replaces that with a per-NodeId splice:

1. `s1_format_docx::reader::read_with_package_and_origin` returns a
   `BodyOrigin { by_node_id, node_id_order }` side-table built at
   parse time by aligning the model's body children with the preserved
   `word/document.xml` body in document order.
2. `Document` now tracks `dirty_body_ids: HashSet<NodeId>` plus a
   `body_structural_dirty` flag. `apply_transaction` walks the
   transaction's operations against the pre-apply model and climbs
   each `target_id` up to its top-level body ancestor — that NodeId
   goes into the dirty set. Insert / delete / move at body level
   flips `body_structural_dirty` and forces wholesale regenerate.
3. `export(Docx)` lanes refined:
   - Nothing dirty → verbatim re-emit (Phase 2).
   - Body structurally changed or no origin → wholesale regenerate
     (Phase 2a fallback).
   - `dirty_body_ids.is_empty()` (no-op edit) → verbatim re-emit.
   - Specific NodeIds dirty → walk preserved body; clean entries
     stay byte-equal, dirty entries swap in the regenerated element
     from the same position.

Every other XmlNode in the body — `<w:sectPr>`, non-TOC `<w:sdt>`,
range markers — sits at its original location and rides through
untouched.

**Done:** `docx_edit_coverage`'s "body zero-drop" climbs from
10/39 to 39/39. The 162 unique unknown body tags Phase 2a dropped
(mostly `a:*` drawing primitives and `mc:*` AlternateContent
fallbacks inside paragraphs with images / shapes) all survive
edits now.

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

### Phase 4 — element-level migration in the editor

Once shadow parsing is clean, the editor starts cutting over: each
element family swaps from xml-js to Casual Core behind a feature flag,
independently revertible.

Suggested order:

1. **Bucket B wins first** — every class where xml-js drops and Casual
   Core preserves. Flipping the flag is an immediate fidelity bump for
   editor users with zero risk of regression for tags eigenpal already
   handled.
2. **Bucket A** — paragraphs, runs, styles, tables, lists, headers,
   footers, images, sections. These already round-trip equivalently;
   migration here is about removing xml-js code, not gaining fidelity.
3. **Bucket C** — fields, track changes, form controls, math. Long-tail
   work; do these last because they're hard for both implementations.

**Done when:** every relevant element class is served by Casual Core.

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

| Metric | Phase 2 | Phase 2a | Phase 2b | Phase 3 | Phase 4 |
| --- | --- | --- | --- | --- | --- |
| Bucket A — outstanding tags | 0 | 0 | 0 | 0 | 0 |
| No-edits zero-drop | ✅ 39/39 | 39/39 | 39/39 | 39/39 | 39/39 |
| Non-body preserved on edits | — | ✅ 39/39 | 39/39 | 39/39 | 39/39 |
| Body zero-drop on edits | — | 10/39 | **39/39** | 39/39 | 39/39 |
| Lossy-tag report — no regression vs xml-js | yes | yes | yes | yes | yes |
| Perf — Casual Core ≤ 2× xml-js wall time | — | — | yes | yes | yes |

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
