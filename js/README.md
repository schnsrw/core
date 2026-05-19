# @schnsrw/core

WASM-powered document converter for DOCX, ODT, PDF, Markdown, and plain text.

The TypeScript entry point for **Casual Core**, the document engine that powers
the [Casual Office](https://schnsrw.live) suite (Casual Editor for documents,
Casual Sheet for spreadsheets, …).

```bash
npm install @schnsrw/core
```

## Quick start

```ts
import { init, convert } from "@schnsrw/core";

await init();

const docx = await fetch("/cv.docx").then((r) => r.arrayBuffer());
const pdf  = await convert(new Uint8Array(docx), { from: "docx", to: "pdf" });

// Save / stream / send — it's just bytes.
```

## API

| Function | Purpose |
| --- | --- |
| `init(opts?)` | Boot the WASM engine. Call once. |
| `convert(input, { from?, to })` | One-shot format conversion. |
| `parseDocx(bytes)` | DOCX → Casual Core document JSON. |
| `serializeDocx(json)` | Document JSON → DOCX bytes. |
| `detectFormat(bytes)` | Sniff a buffer's format. |

Supported formats: `docx`, `odt`, `pdf`, `md`, `txt`.

The full conversion matrix is in `SUPPORTED_CONVERSIONS`.

## Build

```bash
npm run build:wasm   # compiles ../ffi/wasm via wasm-pack
npm run build:ts     # bundles src/ via tsup
npm run build        # both
```

## License

Apache-2.0. See [LICENSE](../LICENSE).
