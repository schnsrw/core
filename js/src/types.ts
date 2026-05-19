// Public types for @schnsrw/core.

/** Document formats Casual Core can read or write. */
export type Format = "docx" | "odt" | "pdf" | "md" | "txt";

/** Result of detecting the format of an input byte buffer. */
export interface DetectedFormat {
  /** Canonical short name, or `null` if the bytes don't match a known format. */
  format: Format | null;
  /** Same as `format`, kept separately so callers can switch on it ergonomically. */
  extension: Format | null;
}

/** Options passed to `convert()`. */
export interface ConvertOptions {
  /** Source format. If omitted, the WASM detector decides from input bytes. */
  from?: Format;
  /** Target format. Required. */
  to: Format;
}

/** Options passed to `init()`. */
export interface InitOptions {
  /**
   * Override the URL or `URL` object pointing at the `.wasm` file. By default
   * the loader resolves it from the package install directory (browser) or
   * the filesystem (Node).
   */
  wasmUrl?: string | URL;
}
