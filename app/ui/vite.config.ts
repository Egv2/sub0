import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri best practice: fixed port (devUrl in tauri.conf.json points here),
// no clearScreen noise, deps prebundled.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    outDir: "dist",
    target: "safari15",
    minify: "esbuild",
    sourcemap: false,
  },
});
