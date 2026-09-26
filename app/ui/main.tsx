import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/bricolage-grotesque";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./state/theme";
import "./styles/tokens.css";
import "./styles/components.css";
import "./styles/app.css";
import { App } from "./App";
import { demoBackend, isDemo } from "./backend/demo";
import { tauriBackend } from "./backend/tauri";

const demo = isDemo();
// macOS draws the traffic lights inside our title bar (titleBarStyle: Overlay).
document.documentElement.dataset.platform = navigator.userAgent.includes("Mac") ? "mac" : "other";
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App backend={demo ? demoBackend() : tauriBackend} demo={demo} />
  </StrictMode>,
);

// Dev-only state probe (see vite.config.ts): a read-only snapshot on request.
if (import.meta.hot) {
  const hot = import.meta.hot;
  hot.on("fittle:probe", async () => {
    const { app } = await import("./state/app");
    const { edits } = await import("./state/edits");
    const s = app.get();
    const e = edits.get();
    const card = (k: string) => s.header?.hdus.flatMap((h) => h.cards).find((c) => c.keyword === k);
    hot.send("fittle:state", {
      href: location.href,
      current: s.current,
      tab: s.tab,
      loading: s.loading,
      error: s.error,
      headerPath: s.header?.path,
      header: { INSTRUME: card("INSTRUME"), TELESCOP: card("TELESCOP") },
      infoCamera: s.opened?.info.fields.camera,
      edits: { editing: e.editing, ops: e.ops, error: e.error, result: e.result, plan: e.plan?.changes },
    });
  });
}
