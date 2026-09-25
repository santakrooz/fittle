import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/bricolage-grotesque";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./styles/tokens.css";
import "./styles/app.css";
import { App } from "./App";
import { tauriBackend } from "./backend/tauri";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App backend={tauriBackend} />
  </StrictMode>,
);
