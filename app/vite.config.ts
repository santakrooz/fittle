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

/** Dev-only: GET /__fittle/state asks the running window for a read-only
 *  snapshot of its state (for debugging the desktop app, which can't be
 *  inspected from outside). Never bundled. */
function stateProbe(): Plugin {
  return {
    name: "fittle-state-probe",
    apply: "serve",
    configureServer(server) {
      let waiting: ((s: unknown) => void)[] = [];
      server.ws.on("fittle:state", (data) => {
        waiting.forEach((w) => w(data));
        waiting = [];
      });
      server.middlewares.use("/__fittle/state", (_req, res) => {
        // Answer once: the first window to reply wins; later or extra replies
        // (several windows, or after the timeout) are ignored.
        const reply = (body: unknown) => {
          if (res.writableEnded || res.headersSent) return;
          res.setHeader("Content-Type", "application/json");
          res.end(JSON.stringify(body, null, 2));
        };
        const timer = setTimeout(() => reply({ error: "no window answered" }), 2000);
        waiting.push((s) => {
          clearTimeout(timer);
          reply(s);
        });
        server.ws.send("fittle:probe", {});
      });
    },
  };
}

// Mirrors AstroSideKick's Tauri-oriented Vite setup.
export default defineConfig({
  root: "ui",
  plugins: [react(), demoFixtures(), stateProbe()],
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
