import type { InitOptions } from "./types.js";

// `wasm` is the namespace produced by wasm-pack for `ffi/wasm`. The build
// script copies the artefact into `../wasm/` next to this directory.
// We import it lazily so consumers can choose when to pay the WASM init cost.
//
// The types are intentionally `any` here — the wasm-bindgen-generated d.ts
// would normally be re-exported, but until the wasm build step runs there's
// no module to point at. The public surface in `index.ts` re-types the
// functions we care about.

let wasm: unknown = null;
let initPromise: Promise<unknown> | null = null;

export async function loadWasm(opts: InitOptions = {}): Promise<unknown> {
  if (wasm) return wasm;
  if (initPromise) return initPromise;

  initPromise = (async () => {
    // Dynamic import keeps the wasm artefact out of the consumer's bundle
    // until they actually call `init()`.
    // eslint-disable-next-line @typescript-eslint/ban-ts-comment
    // @ts-ignore — generated at build time by wasm-pack
    const mod = await import("../wasm/s1engine_wasm.js");
    const url =
      opts.wasmUrl ??
      // eslint-disable-next-line @typescript-eslint/ban-ts-comment
      // @ts-ignore — wasm-pack output exposes the URL alongside the JS shim
      new URL("../wasm/s1engine_wasm_bg.wasm", import.meta.url);
    await mod.default(url);
    wasm = mod;
    return mod;
  })();

  return initPromise;
}

/** Reset the cached module — primarily useful for tests. */
export function _resetForTests(): void {
  wasm = null;
  initPromise = null;
}
