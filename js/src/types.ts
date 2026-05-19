// Public types for @schnsrw/core.

/**
 * Document formats Casual Core can read or write.
 *
 * Not all combinations of `from` and `to` are supported — see
 * `SUPPORTED_CONVERSIONS` in `formats.ts` for the matrix.
 */
export type Format = "docx" | "odt" | "pdf" | "md" | "txt";

/** Result of detecting the format of an input byte buffer. */
export interface DetectedFormat {
  /** Canonical short name. `null` if the format is unknown to Casual Core. */
  format: Format | null;
  /** Best-guess MIME type. */
  mime: string;
  /** Human label, e.g. "Word Document". */
  label: string;
}

/** Options passed to `convert()`. */
export interface ConvertOptions {
  /**
   * Source format. If omitted, Casual Core detects it from the input bytes
   * (magic bytes for ZIP-based formats, content sniff for text formats).
   */
  from?: Format;
  /** Target format. Required. */
  to: Format;
}

/** Options passed to `init()`. */
export interface InitOptions {
  /**
   * Override the URL or `URL` object pointing at the `.wasm` file. By default
   * the loader resolves it from the package install directory (browser) or the
   * filesystem (Node).
   */
  wasmUrl?: string | URL;
}
