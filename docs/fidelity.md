# Casual Core — fidelity policy

## What "fidelity" means here

A conversion has high fidelity when the **content** survives the round trip
unchanged. Formatting drift (different exact font metrics, slightly shifted
margins) is acceptable. Content loss (a missing paragraph, dropped table cell,
discarded image) is not.

For format pairs that target a richer model (e.g. DOCX → MD), fidelity is
only meaningful one way: the source's content reaches the destination, even
if Markdown can't represent every nuance.

## Tiers

| Tier | What we promise | Examples |
| --- | --- | --- |
| **Lossless** | Round-trip with identical content. Formatting may drift. | DOCX → DOCX, ODT → ODT, MD → MD, TXT → TXT |
| **Faithful** | All content preserved, format-appropriate output. Formatting often differs. | DOCX → ODT, ODT → DOCX |
| **Lossy** | Content collapses to what the target supports. | DOCX → MD, DOCX → TXT, anything → PDF |

PDF is export-only. There is no PDF → anything path.

## What "unknown markup" does

Casual Core's parsers are lenient. When they encounter markup they don't
understand, they:

1. Log a warning (when warnings are wired up — currently silent).
2. Continue parsing.
3. Drop the unknown element from the output.

This is by design. A document that crashes the parser is unshippable; a
document that parses with the table footer style ignored is shippable and
flagged for future work.

## Known gaps

Tracked here so consumers know what to expect. Each gap is a candidate
for the `v0.2.x` fidelity pass on the roadmap.

### DOCX

- **Field codes** (`w:fldChar`, `w:instrText`) — parsed but collapse to
  static text on round-trip. TOCs, page numbers, cross-references become
  frozen snapshots.
- **Track changes** (`w:ins`, `w:del`) — not parsed. Revisions are lost.
- **Form controls** (checkbox, dropdown) — parsed, partially round-tripped.
- **Text-box wrap properties** — geometric positioning of floating text
  boxes is approximate; the original wrap mode is sometimes dropped.
- **OMML math** (`m:` namespace) — not parsed.
- **Relative sizing** (`wp14:pctWidth`, `wp14:pctHeight`) — dropped.

### Markdown

CommonMark / GFM is a **deliberately small** target. Some Word-style
formatting has no Markdown syntax to land on; the converter handles the
drop predictably rather than inventing custom syntax.

What collapses on `… → MD` (and therefore has nowhere to come back from
on `MD → DOCX`):

| Source feature | What happens |
| :--- | :--- |
| Line spacing (`w:spacing w:line`, `fo:line-height`) | Dropped. CommonMark has no line-height syntax. |
| Paragraph spacing (`w:spacing w:before/after`) | Dropped. |
| Table cell shading (`w:tcShd`, `fo:background-color`) | Dropped. |
| Table cell borders, font color inside tables | Dropped. |
| Run colors (`w:color`, `w:highlight`) outside code | Dropped. |
| Page geometry, margins, columns | Dropped. |
| Footnotes / endnotes (DOCX) | Reference text emitted inline as `[^label]` — not structured. |
| Custom paragraph styles that aren't headings / quotes / code | Flatten to body text. |
| Word's `Title` / `Subtitle` styles | Map to `# H1` / `## H2`. |
| Localized DOCX heading style IDs (`Überschrift1`, `Titre1`, …) | Recognised via the style's `w:name`. |

What the **MD → DOCX** path injects to keep the converted Word document
looking native (none of these come from the Markdown source — they are
opinionated defaults the converter applies):

- Body line spacing 1.15 with 8pt-after and 11pt Calibri default.
- `Heading1..6` style definitions (bold; 18 → 11pt; before-spacing
  decreasing with depth). `Heading5/6` italicised.
- Tables get a 0.5pt black single-line border on all six edges
  (top/left/bottom/right + insideH + insideV).
- Table column widths sized proportionally to per-column content
  length; `tblW` set to `auto` so Word still autofits.

If you need a *strict* "treat MD as text" conversion — bypassing the
CommonMark parser entirely so consumers can ship their own renderer —
see `Format::MdRaw` (planned).

### ODT

- **Settings file** (`settings.xml`) — written but not always preserved
  through round-trip.
- **Form controls** — same partial support as DOCX.

### PDF (export only)

- **Subset font embedding** — yes; full font embedding for accessibility is
  not yet wired.
- **PDF/A** — partial. Use `Document::export_pdf_a()` (Rust only) for the
  current best-effort export.

## How to report a fidelity bug

1. Attach the input document.
2. Specify the conversion path (e.g. "DOCX → DOCX" or "ODT → MD").
3. Describe what's lost: which paragraph, which table, which property.
4. File at `github.com/schnsrw/core/issues` with label `fidelity`.

Bug reports without a reproducer get closed — fidelity work needs concrete
inputs.

## Fidelity audit (planned)

The `tests/fidelity/` directory will host a CI-driven round-trip audit:
take each fixture in `testdocs/`, run it through every supported
conversion path, diff input vs. output structurally, fail the build on
content loss. Currently a placeholder; wiring it up is on the `v0.2.x`
roadmap.
