import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const __dirname = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  appType: 'mpa',  
  build: {
    rollupOptions: {
      input: {
        playground: resolve(__dirname, "index.html"),
      },
    },
    outDir: "../web", // Changed back to playground sub-directory
  },
  server: {
    headers: {
      "Cross-Origin-Opener-Policy": "same-origin",
      "Cross-Origin-Embedder-Policy": "require-corp",
      "Cross-Origin-Resource-Policy": "cross-origin",
    },
  },
});
