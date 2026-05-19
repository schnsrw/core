/**
 * @schnsrw/core — Casual Core public API.
 *
 * A small, framework-free TypeScript surface over the Casual Core WASM
 * engine. Use this from browsers, Node, Bun or Deno to convert documents
 * between DOCX, ODT, PDF, Markdown, and plain text.
 *
 * @example
 * ```ts
 * import { init, convert } from "@schnsrw/core";
 *
 * await init();
 * const docx = await fetch("/cv.docx").then(r => r.arrayBuffer());
 * const pdf  = await convert(new Uint8Array(docx), { from: "docx", to: "pdf" });
 * ```
 */

import { loadWasm } from "./loader.js";
import { detectFormat, isSupportedConversion } from "./formats.js";
import type { ConvertOptions, Format, InitOptions } from "./types.js";

export type { ConvertOptions, DetectedFormat, Format, InitOptions } from "./types.js";
export { detectFormat, isSupportedConversion, SUPPORTED_CONVERSIONS } from "./formats.js";

/**
 * Initialise the WASM engine. Must be called once before `convert()` or any
 * other API that touches WASM. Subsequent calls are no-ops.
 */
export async function init(opts: InitOptions = {}): Promise<void> {
  await loadWasm(opts);
}

/**
 * Convert a document from one format to another.
 *
 * If `opts.from` is omitted, the format is detected from input bytes.
 */
export async function convert(
  input: Uint8Array | ArrayBuffer | Blob,
  opts: ConvertOptions,
): Promise<Uint8Array> {
  const bytes = await toBytes(input);
  const from = opts.from ?? detectFormat(bytes).format;

  if (!from) {
    throw new Error("Could not detect input format. Pass `from` explicitly.");
  }
  if (!isSupportedConversion(from, opts.to)) {
    throw new Error(`Conversion ${from} -> ${opts.to} is not supported.`);
  }

  const wasm = (await loadWasm()) as WasmModule;
  return runConversion(wasm, bytes, from, opts.to);
}

/**
 * Parse a DOCX byte buffer into Casual Core's internal document JSON.
 *
 * Stable across releases; intended as the bridge into other editors
 * (e.g. ProseMirror schemas).
 */
export async function parseDocx(input: Uint8Array | ArrayBuffer): Promise<unknown> {
  const bytes = await toBytes(input);
  const wasm = (await loadWasm()) as WasmModule;
  const engine = new wasm.WasmEngine();
  const doc = engine.open(bytes);
  return JSON.parse(doc.to_json());
}

/** Serialise a document JSON (as produced by `parseDocx`) back to DOCX bytes. */
export async function serializeDocx(doc: unknown): Promise<Uint8Array> {
  const wasm = (await loadWasm()) as WasmModule;
  const engine = new wasm.WasmEngine();
  const wasmDoc = engine.from_json(JSON.stringify(doc));
  return wasmDoc.export_docx();
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

async function toBytes(input: Uint8Array | ArrayBuffer | Blob): Promise<Uint8Array> {
  if (input instanceof Uint8Array) return input;
  if (input instanceof ArrayBuffer) return new Uint8Array(input);
  if (input instanceof Blob) return new Uint8Array(await input.arrayBuffer());
  throw new Error("Unsupported input — expected Uint8Array, ArrayBuffer, or Blob.");
}

function runConversion(
  wasm: WasmModule,
  bytes: Uint8Array,
  from: Format,
  to: Format,
): Uint8Array {
  const engine = new wasm.WasmEngine();
  const doc = engine.open(bytes);
  switch (to) {
    case "docx": return doc.export_docx();
    case "odt":  return doc.export_odt();
    case "pdf":  return doc.export_pdf();
    case "md":   return new TextEncoder().encode(doc.export_md());
    case "txt":  return new TextEncoder().encode(doc.export_txt());
    default: {
      // Exhaustiveness check — TS will complain if Format grows a variant.
      const _exhaustive: never = to;
      throw new Error(`Unhandled output format: ${String(_exhaustive)} (from ${from})`);
    }
  }
}

// Minimal shape of the WASM module surface this layer touches. Replace with
// the wasm-pack-generated d.ts once the build pipeline is wired.
interface WasmModule {
  WasmEngine: new () => {
    open(bytes: Uint8Array): WasmDocument;
    from_json(json: string): WasmDocument;
  };
}
interface WasmDocument {
  to_json(): string;
  export_docx(): Uint8Array;
  export_odt(): Uint8Array;
  export_pdf(): Uint8Array;
  export_md(): string;
  export_txt(): string;
}
