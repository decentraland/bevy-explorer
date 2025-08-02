import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const __dirname = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  appType: 'mpa',  
  build: {
    rollupOptions: {
      input: {
        root: resolve(__dirname, "index.html"),
        play: resolve(__dirname, "play/index.html"),
        playground: resolve(__dirname, "playground/index.html"),
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
