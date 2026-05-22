# Casual Core — roadmap

Living document. Targets are guidelines, not contracts. Open issues
override anything written here.

## Now — `v0.1.x` — bootstrap

Status: scaffolding done, CI green, foundation laid.

- [x] Strip the editor / server / CRDT / spreadsheet weight inherited from
      the old `doc-engine` repo.
- [x] Apache-2.0 relicensing.
- [x] Minimal WASM converter surface (`detect_format`, `convert`,
      `convert_to_string`, `extract_text`).
- [x] `@schnsrw/core` npm package scaffolding.
- [x] GitHub Pages demo as a manual smoke-test surface.
- [x] Green CI on every push (Rust + JS + WASM + Pages).
- [ ] First published `0.1.0` npm release.

## Now — `v0.2.x` — DOCX fidelity pass

The big rock. Detailed plan in
[`integration-plan.md`](integration-plan.md) — phases tracked there.

- [x] Mirror eigenpal's 39 DOCX fixtures into `testdocs/docx/eigenpal/`.
- [x] DOCX coverage scorecard (`tests/docx_coverage.rs`) — three-bucket
      matrix on every push, results in `docs/docx-coverage.md`.
- [x] **Phase 2** — `s1-ooxml` preservation layer.
      `crates/s1-ooxml/` parses any OOXML package losslessly and writes
      it back. 39/39 fixtures round-trip with zero tag loss in
      `tests/passthrough.rs`.
- [x] **Phase 2 wire-up** — `s1engine::Document` carries an
      `Option<s1_ooxml::Package>` preservation field; `export(Docx)`
      re-emits it verbatim while preservation is intact.
      Bucket A collapsed 140 → 0 on the no-edits path; Bucket B grew
      0 → 22 (we now lead the consumer on tags they're known to drop).
- [x] **Phase 2a** — preservation survives edits. The package is kept
      across mutation; `export(Docx)` splices the regenerated
      `word/document.xml` into a clone of the preserved package so
      every other part (theme, fontTable, customXml, headers, footers,
      footnotes, endnotes, comments, numbering, styles, images, rels,
      content types) rides through unchanged. New test
      `tests/docx_edit_coverage.rs` is the regression gate.
- [x] **Phase 2b** — body preservation under edits. `BodyOrigin`
      side-table built at parse time maps each top-level body NodeId
      to its preserved `XmlElement`; `Document` tracks
      `dirty_body_ids: HashSet<NodeId>` populated by `apply_transaction`
      from each op's `target_id` (climbed to its top-level body
      ancestor). `export(Docx)` walks the preserved body; clean
      NodeIds stay byte-equal, dirty NodeIds swap in the regenerated
      element. `docx_edit_coverage` body-zero-drop went 10/39 → 39/39;
      all 162 previously-dropped unknown body tags (`a:*`, `mc:*`,
      `wps:*`, `w14:*`) survive edits now.
- [ ] ODT — same playbook against an ODT fixture set. Progress:
  - [x] **Coverage scorecard** (`odt_coverage`): baseline 0 / 4
        zero-drop with 20 unique dropped tag classes on the existing
        `testdocs/odt/` fixtures.
  - [x] **`s1-odf` preservation crate** (`crates/s1-odf/`) — analog
        of `s1-ooxml`. Honours ODF specifics (mimetype-first STORED,
        `META-INF/manifest.xml`, lenient parse for empty `.xml`
        parts). Passthrough audit: **4 / 4 zero-drop** on the
        existing fixture set.
  - [x] **Phase 2 wire-up** — `Engine::open(Odt)` keeps the
        `s1_odf::Package` alongside `DocumentModel`; `export(Odt)`
        re-emits verbatim when `model_dirty == false`. `odt_coverage`
        body-zero-drop jumped 0 / 4 → **4 / 4**; bytes_in ≈ bytes_out
        (1 MB freetestdata fixture now writes back at ~1 MB instead
        of ~4 KB). Contract is asserted, not just reported.
  - [x] **Phase 2a** — non-body parts ride through edits.
        `Document::export(Odt)` on a dirty document splices at the
        XmlTree tier: regenerated `<office:body>` swaps into the
        preserved `content.xml` while sibling sections
        (`<office:automatic-styles>`, `<office:font-face-decls>`,
        `<office:scripts>`, `<office:settings>`) ride through.
        Surrounding parts (`styles.xml`, `meta.xml`,
        `META-INF/manifest.xml`, `Pictures/*`, `Configurations2/*`,
        `Thumbnails/*`) also preserved via the package clone.
        Dropped tag classes on edit went **20 → 11** (the styles
        cascade — `style:*-properties` 88+60+16+15+10+4x,
        `style:font-face` 18x, `office:font-face-decls`,
        `office:scripts` — all recovered).
  - [x] **Phase 2b** — per-NodeId body splice. `s1_format_odt::BodyOrigin`
        mirrors the DOCX one; `Document` carries an
        `odf_body_origin: Option<…>`; `export(Odt)` walks preserved
        `<office:text>` and only swaps dirty NodeIds, so untouched
        paragraphs / headings / tables keep their preserved
        `XmlElement` verbatim. Required fixing a body-parser gap:
        self-closing `<text:p/>` were silently dropped via the
        `Event::Empty` arm — added `insert_empty_paragraph()` and
        the Empty handler so model body and preserved body align
        1:1. Effect: **`odt_edit_coverage` body-zero-drop = 4 / 4**
        (was 0 / 4). All 11 remaining body-internal classes —
        `draw:frame`, `text:span`, `text:s`, `text:soft-page-break`,
        `svg:title`/`desc`, `table:table-columns`,
        `text:sequence-decl[s]`, `draw:image`, `draw:object` — now
        survive edits via the preserved XmlElement.
- [ ] **DOCX → PDF visual fidelity** — user-reported ~2/100 on
      2026-05-22. Progress:
  - [x] `pdf_coverage` scorecard shipped — renders every DOCX + ODT
        fixture through `export(Format::Pdf)` and counts pages, `Tj`
        ops, Image XObjects, embedded fonts, vector path ops. Output
        to `docs/pdf-coverage.md` on every run.
  - [x] Text rendering fixed — root cause was `FontDatabase::empty()`
        in the export path; switched to `FontDatabase::new()`.
        38/39 fixtures now emit `Tj` (the one with 0 is `empty.docx`).
  - [x] Inline image embedding fixed — `collect_and_embed_images` was
        not recursing into `Paragraph` blocks; `render_line` had a
        stray `continue` dropping inline images. Both fixed. Format
        support: PNG, JPEG, WebP, BMP, GIF, TIFF, ICO, SVG.
  - [x] Header / footer tables now lay out — child-walk extended to
        handle `Table` children alongside `Paragraph`; empty Drawing
        anchor nodes no longer inject phantom 100×100 pt runs.
  - [x] Corrupt test-fixture PNGs replaced — `image-hyperlink.docx`
        and `oversized-header-image.docx` had IDAT CRC mismatches;
        "images vanish" aggregate 2× → 0×.
  - [x] **`wordprocessingShape` text boxes** — `<wps:txbx>` content
        extracted by the DOCX parser (new `ShapeText` attribute), laid
        out as `InlineTextBox` runs, rendered in PDF with border
        rectangle + Helvetica text. `textbox-test.docx` and
        `wpg-group.docx` now emit text. Remaining gap: pure
        `prstGeom` rectangle shapes with no text content (1 fixture,
        `drawingml-shape.docx`).
  - [x] **EMF / WMF transcoding** — `crates/s1-format-pdf/src/emf.rs`
        implements EMF → SVG (geometric primitives, GDI objects,
        path brackets, text, embedded DIBs) rasterised to PNG via
        `resvg`. `issue-319-sections.docx` went 0 → **13** `Do` ops;
        all 13 EMF drawings now embed. Both block-level and inline
        image paths route through the transcoder.
  - [x] **Embedded DOCX font loading** — `extract_embedded_fonts` in
        `s1-format-docx` reads `word/fontTable.xml`, follows
        `word/_rels/fontTable.xml.rels`, extracts each `word/fonts/*.odttf`
        binary, and XOR-deobfuscates using the `w:fontKey` GUID (reversed
        byte order, ECMA-376 §9.7.3.3). Bytes loaded into `FontDatabase`
        before PDF layout. Ubuntu, corporate, and other embedded fonts now
        render with correct metrics instead of falling back to Times New
        Roman. `Font::family_name()` bug fixed: Mac-platform name records
        (which ttf_parser can't decode) no longer shadow Windows-Unicode
        records that decode correctly.

## Then — `v0.3.x` — performance + hostile input

- [ ] Stream the DOCX reader (don't hold the full XML in memory). The
      current `s1-format-docx` parser is in-memory; streaming is a known
      target. See `crates/s1-format-docx/src/streaming.rs`.
- [x] `cargo-fuzz` harnesses for the parse + edit + export surfaces.
      Eight targets including `fuzz_ooxml_package` (Phase 2 preservation
      tier) and `fuzz_docx_phase2b` (Phase 2b origin table + per-NodeId
      splice). `.github/workflows/fuzz-nightly.yml` runs each target for
      5 minutes at 03:17 UTC daily; crashes upload as artefacts.
- [x] 500-page DOCX → PDF benchmark — `pdf_500_pages` bench group,
      runs the full layout + shaping + PDF pipeline. Reference number
      ~244 ms median on Apple Silicon (range 221–274 ms across
      10 samples). See [`docs/testing-strategy.md`](testing-strategy.md).
- [ ] Cap memory at ≤ 4× input size during conversion.

## Later — `v1.0` — API freeze

- [ ] Re-tighten clippy to `-D warnings` after the inherited backlog is
      fixed.
- [ ] Lock down the JS public API surface (`init`, `convert`,
      `convertToString`, `detectFormat`, `extractText`).
- [ ] Lock down the Rust public API surface (`Engine`, `Document`,
      `Format`, `Error`).
- [ ] Cut `1.0.0` once the API is stable and fidelity is green.

## Out of scope, forever

These are explicit non-goals. Don't propose them — the answer is "different
repo".

- Editor UI of any kind.
- Real-time collaboration / CRDTs.
- Spreadsheets, presentations.
- Server / HTTP / WebSocket code.
- Async Rust API at the engine boundary.

## Tracking

Live work tracking lives in GitHub Issues on `schnsrw/core`, not here.
