# @schnsrw/core

WASM-powered document converter for DOCX, ODT, PDF, Markdown, and plain text.

The TypeScript entry point for **Casual Core**, the document engine behind
the [Casual Office](https://schnsrw.live) suite.

```bash
npm install @schnsrw/core
```

## Quick start

```ts
import { init, convert } from "@schnsrw/core";

await init();

const docx = await fetch("/cv.docx").then((r) => r.arrayBuffer());
const pdf  = await convert(new Uint8Array(docx), { to: "pdf" });

// Save / stream / send — it's just bytes.
```

## API

Five functions. That's the whole surface.

| Function | Purpose |
| --- | --- |
| `init(opts?)` | Boot the WASM engine. Call once. |
| `convert(input, { from?, to })` | One-shot conversion, returns `Uint8Array`. |
| `convertToString(input, { from?, to })` | Same, returns a UTF-8 string. Best for `md` / `txt`. |
| `detectFormat(input)` | Sniff a buffer's format. |
| `extractText(input, from?)` | Get the plain-text content of a document. |

`input` may be `Uint8Array`, `ArrayBuffer`, or `Blob`. `from` is auto-detected
if omitted.

Supported formats: `docx`, `odt`, `pdf`, `md`, `txt`.

Full type reference in [`docs/api.md`](../docs/api.md) at the repo root.

## Build

```bash
npm run build:wasm   # compiles ../ffi/wasm via wasm-pack into ../js/wasm/
npm run build:ts     # bundles src/ via tsup into dist/
npm run build        # both
```

## License

Apache-2.0. See [LICENSE](../LICENSE).
