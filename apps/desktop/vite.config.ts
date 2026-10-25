import path from "node:path";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port and does not want Vite to clear the screen.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // Tauri drives the Rust build itself. If Vite watches `src-tauri` (in
      // particular `src-tauri/target`, which holds `.o`/`.rlib` files that the
      // compiler keeps locked), chokidar hits EBUSY on Windows and crashes the
      // dev server. Let Tauri own that directory.
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "esnext",
    sourcemap: true,
  },
});
