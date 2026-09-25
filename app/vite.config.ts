import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

// Mirrors AstroSideKick's Tauri-oriented Vite setup.
export default defineConfig({
  root: "ui",
  plugins: [react()],
  clearScreen: false,
  build: { outDir: "../dist", emptyOutDir: true },
  server: {
    port: 1430,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1431 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
