import { createReadStream, existsSync, statSync } from "node:fs";
import { join, normalize } from "node:path";
import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

const host = process.env.TAURI_DEV_HOST;

/** Dev-only: serve `?demo` fixtures from app/demo-fixtures at /demo/* (never bundled). */
function demoFixtures(): Plugin {
  const root = join(import.meta.dirname, "demo-fixtures");
  return {
    name: "fittle-demo-fixtures",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use("/demo", (req, res, next) => {
        const rel = normalize(decodeURIComponent((req.url ?? "/").split("?")[0])).replace(/^(\.\.[/\\])+/, "");
        const file = join(root, rel);
        if (!file.startsWith(root) || !existsSync(file) || !statSync(file).isFile()) return next();
        res.setHeader("Content-Type", file.endsWith(".json") ? "application/json" : "application/octet-stream");
        createReadStream(file).pipe(res);
      });
    },
  };
}

// Mirrors AstroSideKick's Tauri-oriented Vite setup.
export default defineConfig({
  root: "ui",
  plugins: [react(), demoFixtures()],
  clearScreen: false,
  build: { outDir: "../dist", emptyOutDir: true },
  server: {
    port: 1430,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1431 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/demo-fixtures/**"] },
  },
});
