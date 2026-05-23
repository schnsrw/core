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
  openToModel,
  openToModelString,
  convertModel,
  convertModelString,
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

// Open as a structured JSON model (Phase B).
const model = await openToModel(docxBytes, "docx");
//   model.nodes["0:1"].nodeType === "paragraph"

// Write a (possibly mutated) JSON model back to bytes (Phase C).
const odt = await convertModel(model, { to: "odt" });

// String variants avoid an extra JS object hop — useful when you're
// posting the payload to a worker or storing it.
const modelStr = await openToModelString(docxBytes, "docx");
const bytes    = await convertModelString(modelStr, { to: "pdf" });
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

interface S1Node {
  id: string;                          // "replica:counter", e.g. "0:1"
  nodeType: string;                    // "paragraph" | "run" | "table" | …
  children: string[];                  // child node IDs in order
  parent: string | null;
  textContent: string | null;          // populated for nodeType === "text"
  attributes: Record<string, unknown>; // camelCase keys
}

interface S1DocumentModel {
  root: string;                        // root node ID
  nodes: Record<string, S1Node>;
  metadata: { title?: string; creator?: string; language?: string; /* … */ };
  styles: Array<{ id: string; name: string; styleType: string; /* … */ }>;
  sections: Array<{ pageWidth: number; pageHeight: number; /* … */ }>;
}
```

That's the whole public surface — nine functions plus their supporting
types. The first five cover bytes-in / bytes-out workflows; `openToModel`
and `convertModel` expose the structured document for editor consumers.

## WebAssembly — raw

If you'd rather skip the TS wrapper and import the wasm-pack output directly:

```js
import init, {
  detect_format,
  convert,
  convert_to_string,
  extract_text,
  open_to_json_string,
  open_to_json,
  convert_from_model_string,
  convert_from_model,
} from "@schnsrw/core/wasm";

await init();
const pdf = convert(docxBytes, "docx", "pdf");
```

Functions match the TS wrapper one-to-one (`open_to_json_string` ⇔
`openToModelString`, etc.). The wrapper exists mainly to ergonomically
auto-init and normalise `Uint8Array` / `ArrayBuffer` / `Blob` inputs.

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
surface. Once `1.0` ships, the JS surface (`init`, `convert`,
`convertToString`, `detectFormat`, `extractText`, `openToModel`,
`openToModelString`, `convertModel`, `convertModelString`) and the Rust
`Engine` / `Document` / `Format` / `Error` types are frozen.
