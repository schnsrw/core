import { defineConfig } from "vite";

// Configured for GitHub Pages deployment under /core/ by default.
// Override with `BASE_PATH=/ vite build` for a custom-domain deployment.
const base = process.env.BASE_PATH ?? "/core/";

export default defineConfig({
  base,
  build: {
    outDir: "dist",
    sourcemap: true,
    target: "es2022",
  },
  worker: {
    format: "es",
  },
  optimizeDeps: {
    exclude: ["@schnsrw/core"],
  },
  server: {
    port: 5173,
    headers: {
      // Required so WASM-using consumers can use SharedArrayBuffer if we ever
      // need it; harmless otherwise.
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
    },
  },
});
