import { defineConfig } from "vite";
import { resolve } from "path";

export default defineConfig({
  root: "src",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "esnext",
    outDir: "../dist",
    emptyOutDir: true,
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "src/index.html"),
        overlay: resolve(__dirname, "src/overlay.html"),
      },
    },
  },
});
