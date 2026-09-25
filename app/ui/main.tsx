import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/bricolage-grotesque";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
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
