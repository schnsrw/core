import { init, convert, detectFormat } from "@schnsrw/core";
import type { Format } from "@schnsrw/core";

const fileInput = qs<HTMLInputElement>("#file");
const dropzone = qs<HTMLLabelElement>(".dropzone");
const controls = qs<HTMLDivElement>("#controls");
const detectedEl = qs<HTMLElement>("#detected");
const targetSel = qs<HTMLSelectElement>("#target");
const convertBtn = qs<HTMLButtonElement>("#convert");
const statusEl = qs<HTMLOutputElement>("#status");

let currentBytes: Uint8Array | null = null;
let currentFormat: Format | null = null;
let currentName = "document";

dropzone.addEventListener("dragover", (e) => {
  e.preventDefault();
  dropzone.classList.add("is-drag");
});
dropzone.addEventListener("dragleave", () => dropzone.classList.remove("is-drag"));
dropzone.addEventListener("drop", (e) => {
  e.preventDefault();
  dropzone.classList.remove("is-drag");
  const file = e.dataTransfer?.files?.[0];
  if (file) void handleFile(file);
});
fileInput.addEventListener("change", () => {
  const file = fileInput.files?.[0];
  if (file) void handleFile(file);
});
convertBtn.addEventListener("click", () => void runConvert());

async function handleFile(file: File) {
  setStatus(`Loading ${file.name}…`);
  currentName = file.name.replace(/\.[^.]+$/, "");
  const buf = await file.arrayBuffer();
  currentBytes = new Uint8Array(buf);
  await init();
  const detected = await detectFormat(currentBytes);
  currentFormat = detected.format;
  detectedEl.textContent = detected.format ?? "unknown";
  controls.hidden = false;
  setStatus(detected.format ? "Ready." : "Could not detect format.");
}

async function runConvert() {
  if (!currentBytes || !currentFormat) return;
  const to = targetSel.value as Format;
  convertBtn.disabled = true;
  setStatus(`Converting ${currentFormat} → ${to}…`);
  try {
    await init();
    const bytes = await convert(currentBytes, { from: currentFormat, to });
    download(bytes, `${currentName}.${to}`, mimeFor(to));
    setStatus("Done.");
  } catch (err) {
    setStatus(err instanceof Error ? err.message : String(err), true);
  } finally {
    convertBtn.disabled = false;
  }
}

function download(bytes: Uint8Array, filename: string, mime: string) {
  const blob = new Blob([bytes], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function mimeFor(fmt: Format): string {
  switch (fmt) {
    case "docx": return "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    case "odt":  return "application/vnd.oasis.opendocument.text";
    case "pdf":  return "application/pdf";
    case "md":   return "text/markdown";
    case "txt":  return "text/plain";
  }
}

function setStatus(msg: string, isError = false) {
  statusEl.textContent = msg;
  statusEl.classList.toggle("error", isError);
}

function qs<T extends Element>(sel: string): T {
  const el = document.querySelector<T>(sel);
  if (!el) throw new Error(`Missing element: ${sel}`);
  return el;
}
