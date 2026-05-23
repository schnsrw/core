/**
 * @schnsrw/core — Casual Core public API.
 *
 * A small, framework-free TypeScript surface over the Casual Core WASM
 * engine. Use this from browsers, Node, Bun, or Deno to convert documents
 * between DOCX, ODT, PDF, Markdown, and plain text.
 *
 * @example
 * ```ts
 * import { init, convert } from "@schnsrw/core";
 *
 * await init();
 * const docx = await fetch("/cv.docx").then(r => r.arrayBuffer());
 * const pdf  = await convert(new Uint8Array(docx), { to: "pdf" });
 * ```
 */

import { loadWasm } from "./loader.js";
import type {
  ConvertOptions,
  DetectedFormat,
  Format,
  InitOptions,
  S1DocumentModel,
} from "./types.js";

export type {
  ConvertOptions,
  DetectedFormat,
  Format,
  InitOptions,
  S1DocumentModel,
  S1Metadata,
  S1Node,
  S1Section,
  S1Style,
} from "./types.js";

/** Initialise the WASM engine. Call once. Subsequent calls are no-ops. */
export async function init(opts: InitOptions = {}): Promise<void> {
  await loadWasm(opts);
}

/**
 * Detect the format of a buffer. Returns a normalised extension string,
 * or `null` if the bytes don't match a known format.
 */
export async function detectFormat(
  input: Uint8Array | ArrayBuffer | Blob,
): Promise<DetectedFormat> {
  const bytes = await toBytes(input);
  const wasm = (await loadWasm()) as WasmModule;
  const ext = wasm.detect_format(bytes) as Format | "";
  return {
    format: ext === "" ? null : ext,
    extension: ext || null,
  };
}

/**
 * Convert a document from one format to another.
 *
 * If `opts.from` is omitted, the format is auto-detected from input bytes.
 */
export async function convert(
  input: Uint8Array | ArrayBuffer | Blob,
  opts: ConvertOptions,
): Promise<Uint8Array> {
  const bytes = await toBytes(input);
  const wasm = (await loadWasm()) as WasmModule;
  return wasm.convert(bytes, opts.from ?? "", opts.to);
}

/** Same as `convert`, but returns a UTF-8 string. Best for `md` / `txt` outputs. */
export async function convertToString(
  input: Uint8Array | ArrayBuffer | Blob,
  opts: ConvertOptions,
): Promise<string> {
  const bytes = await toBytes(input);
  const wasm = (await loadWasm()) as WasmModule;
  return wasm.convert_to_string(bytes, opts.from ?? "", opts.to);
}

/** Extract the plain-text content of a document, no formatting. */
export async function extractText(
  input: Uint8Array | ArrayBuffer | Blob,
  from?: Format,
): Promise<string> {
  const bytes = await toBytes(input);
  const wasm = (await loadWasm()) as WasmModule;
  return wasm.extract_text(bytes, from ?? "");
}

/**
 * Parse a document and return its structural model as a parsed object.
 *
 * Internally calls `open_to_json_string` (one WASM call, string transfer)
 * and parses the result with `JSON.parse`.
 */
export async function openToModel(
  input: Uint8Array | ArrayBuffer | Blob,
  from?: Format,
): Promise<S1DocumentModel> {
  const s = await openToModelString(input, from);
  return JSON.parse(s) as S1DocumentModel;
}

/**
 * Parse a document and return its structural model as a raw JSON string.
 *
 * Useful when you want to post the payload to a Worker, store it, or parse
 * it yourself. Cheaper than `openToModel` when you don't need the JS object.
 */
export async function openToModelString(
  input: Uint8Array | ArrayBuffer | Blob,
  from?: Format,
): Promise<string> {
  const bytes = await toBytes(input);
  const wasm = (await loadWasm()) as WasmModule;
  return wasm.open_to_json_string(bytes, from ?? "");
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

// Shape of the WASM module — replaced at build time by the wasm-pack-generated
// `.d.ts` once the build pipeline is wired through tsup.
interface WasmModule {
  detect_format(bytes: Uint8Array): string;
  convert(bytes: Uint8Array, from: string, to: string): Uint8Array;
  convert_to_string(bytes: Uint8Array, from: string, to: string): string;
  extract_text(bytes: Uint8Array, from: string): string;
  open_to_json_string(bytes: Uint8Array, from: string): string;
  open_to_json(bytes: Uint8Array, from: string): unknown;
}
