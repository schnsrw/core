# Casual Core — API surface

Three places to call into Casual Core, from least to most direct.

## JavaScript / TypeScript — `@schnsrw/core`

The recommended entry point. Works in browsers, Node, Bun, Deno.

```ts
import {
  init,
  convert,
  convertToString,
  detectFormat,
  extractText,
} from "@schnsrw/core";

await init();

// Bytes-in, bytes-out conversion.
const pdf = await convert(docxBytes, { from: "docx", to: "pdf" });

// Format auto-detection.
const { format } = await detectFormat(unknownBytes);  // "docx" | "odt" | …

// Plain-text extraction.
const text = await extractText(docxBytes, "docx");

// Text-output convenience.
const md = await convertToString(docxBytes, { from: "docx", to: "md" });
```

### Types

```ts
type Format = "docx" | "odt" | "pdf" | "md" | "txt";

interface ConvertOptions {
  from?: Format;   // omit to auto-detect
  to: Format;
}

interface DetectedFormat {
  format: Format | null;
  extension: Format | null;
}

interface InitOptions {
  wasmUrl?: string | URL;  // override the bundled .wasm location
}
```

That's the whole public surface. Five functions, five types.

## WebAssembly — raw

If you'd rather skip the TS wrapper and import the wasm-pack output directly:

```js
import init, {
  detect_format,
  convert,
  convert_to_string,
  extract_text,
} from "@schnsrw/core/wasm";

await init();
const pdf = convert(docxBytes, "docx", "pdf");
```

Functions match the TS wrapper one-to-one. The wrapper exists mainly to
ergonomically auto-init and normalise `Uint8Array` / `ArrayBuffer` / `Blob`
inputs.

## Rust — `s1engine`

```rust
use s1engine::{Engine, Format};

let engine = Engine::new();
let doc = engine.open(&bytes_in)?;        // auto-detect
// or: engine.open_as(&bytes_in, Format::Docx)?;

let pdf_bytes = doc.export(Format::Pdf)?;
let markdown  = doc.export_string(Format::Md)?;
let plain     = doc.to_plain_text();
```

The `s1engine` facade re-exports the common types. For lower-level access:

| Need | Crate |
| --- | --- |
| The document AST | `s1_model` |
| Operations / transactions / undo | `s1_ops` |
| Per-format reader/writer trait impls | `s1_format_{docx,odt,pdf,md,txt}` |
| Page layout | `s1_layout` |
| Text shaping | `s1_text` |
| Legacy `.doc` reader | `s1_convert` |

## Error model

Every fallible operation returns `Result<T, s1engine::Error>` on the Rust
side and rejects with a `JsError` on the JS side. The error message
preserves the underlying cause text (XML parse error, ZIP error, unsupported
format, etc.). There are no exceptions or panics in library code.

## Stability

`0.x` while the API is shaking out. Minor versions may break the public
surface. Once `1.0` ships, all five JS functions and the Rust `Engine` /
`Document` / `Format` / `Error` types are frozen.
