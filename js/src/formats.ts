import type { DetectedFormat, Format } from "./types.js";

const ZIP_SIGNATURE = [0x50, 0x4b, 0x03, 0x04]; // PK\x03\x04
const PDF_SIGNATURE = [0x25, 0x50, 0x44, 0x46]; // %PDF

const matchSig = (bytes: Uint8Array, sig: number[], offset = 0): boolean => {
  if (bytes.length < offset + sig.length) return false;
  for (let i = 0; i < sig.length; i++) {
    if (bytes[offset + i] !== sig[i]) return false;
  }
  return true;
};

/**
 * Detect the format of a document from its bytes.
 *
 * Returns `format: null` if the input does not match any known format.
 * For ZIP-based formats (DOCX, ODT) the discriminator lives inside the
 * archive, so this function delegates to the WASM-side detector when one
 * is wired in. For now it uses byte signatures plus heuristics.
 */
export function detectFormat(bytes: Uint8Array): DetectedFormat {
  if (matchSig(bytes, PDF_SIGNATURE)) {
    return { format: "pdf", mime: "application/pdf", label: "PDF" };
  }

  if (matchSig(bytes, ZIP_SIGNATURE)) {
    // DOCX vs ODT requires looking at the mimetype entry inside the zip.
    // The WASM side has a robust detector — JS-side we fall back to "docx"
    // as the most common case; callers should pass `from` explicitly when
    // the input could be ODT.
    return {
      format: "docx",
      mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
      label: "Office Open XML / OpenDocument",
    };
  }

  // Markdown vs plain text: cheap heuristic on the first 1KB.
  const head = new TextDecoder("utf-8", { fatal: false }).decode(
    bytes.subarray(0, 1024),
  );
  if (/^#{1,6}\s|\n#{1,6}\s|\*\*|^[-*+]\s|\[[^\]]+\]\([^)]+\)/m.test(head)) {
    return { format: "md", mime: "text/markdown", label: "Markdown" };
  }

  // If it decodes as valid UTF-8 we call it text; otherwise unknown.
  try {
    new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(0, 4096));
    return { format: "txt", mime: "text/plain", label: "Plain Text" };
  } catch {
    return { format: null, mime: "application/octet-stream", label: "Unknown" };
  }
}

/**
 * Pairs of `(from, to)` that Casual Core supports today.
 *
 * Update this when WASM-side conversion paths are wired through.
 */
export const SUPPORTED_CONVERSIONS: ReadonlyArray<readonly [Format, Format]> = [
  ["docx", "pdf"],
  ["docx", "md"],
  ["docx", "txt"],
  ["odt", "pdf"],
  ["odt", "md"],
  ["odt", "txt"],
  ["md", "docx"],
  ["md", "odt"],
  ["md", "txt"],
  ["txt", "docx"],
  ["txt", "md"],
  ["docx", "docx"],
  ["odt", "odt"],
];

export function isSupportedConversion(from: Format, to: Format): boolean {
  return SUPPORTED_CONVERSIONS.some((pair) => pair[0] === from && pair[1] === to);
}
