// Public types for @schnsrw/core.

/** Document formats Casual Core can read or write.
 *
 * `md-raw` is a passthrough — input bytes land in the model unchanged so a
 * consumer can plug in its own Markdown renderer. CommonMark parsing is
 * applied only for `md`. */
export type Format = "docx" | "odt" | "pdf" | "md" | "md-raw" | "txt";

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

// ── Document model types (Phase B) ──────────────────────────────────────────

/** A single node in the document tree. */
export interface S1Node {
  /** Node ID in "replica:counter" format. */
  id: string;
  /** Node type (camelCase). e.g. "paragraph", "run", "tableCell". */
  nodeType: string;
  /** IDs of child nodes, in document order. */
  children: string[];
  /** ID of the parent node, or null for the root. */
  parent: string | null;
  /** Text content, only set on Text nodes. */
  textContent: string | null;
  /** Formatting attributes. Keys are camelCase attribute names. */
  attributes: Record<string, unknown>;
}

/** Document metadata (Dublin Core properties). */
export interface S1Metadata {
  title: string | null;
  subject: string | null;
  creator: string | null;
  description: string | null;
  keywords: string[];
  created: string | null;
  modified: string | null;
  revision: number | null;
  language: string | null;
}

/** A named style definition. */
export interface S1Style {
  id: string;
  name: string;
  /** "paragraph", "character", "table", or "list". */
  styleType: string;
  parentId: string | null;
  nextStyleId: string | null;
  isDefault: boolean;
  attributes: Record<string, unknown>;
}

/** Page layout for one section. */
export interface S1Section {
  pageWidth: number;
  pageHeight: number;
  /** "portrait" or "landscape". */
  orientation: string;
  marginTop: number;
  marginBottom: number;
  marginLeft: number;
  marginRight: number;
  headerDistance: number;
  footerDistance: number;
  columns: number;
  columnSpacing: number;
  titlePage: boolean;
  evenAndOddHeaders: boolean;
}

/** Complete document model returned by `openToModel` / `openToModelString`. */
export interface S1DocumentModel {
  /** ID of the root Document node. */
  root: string;
  /** All nodes indexed by their ID string. */
  nodes: Record<string, S1Node>;
  metadata: S1Metadata;
  styles: S1Style[];
  sections: S1Section[];
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
