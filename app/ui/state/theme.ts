// Appearance (screen 12): dark (default), light, night-vision, or follow
// the system. Night-vision also turns every image deep red (see app.css).
import { createStore } from "./createStore";

export type Theme = "system" | "dark" | "light" | "night";
const KEY = "fittle.theme";
const ORDER: Theme[] = ["dark", "light", "night", "system"];

function saved(): Theme {
  try {
    const t = localStorage.getItem(KEY) as Theme | null;
    return t && ORDER.includes(t) ? t : "dark";
  } catch {
    return "dark";
  }
}

export const themeStore = createStore<{ theme: Theme }>({ theme: saved() });

const media = typeof matchMedia === "function" ? matchMedia("(prefers-color-scheme: light)") : null;

function apply() {
  const t = themeStore.get().theme;
  const resolved = t === "system" ? (media?.matches ? "light" : "dark") : t;
  document.documentElement.dataset.theme = resolved;
}

export function setTheme(theme: Theme) {
  themeStore.set({ theme });
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    /* this session only */
  }
  apply();
}

export const cycleTheme = () => setTheme(ORDER[(ORDER.indexOf(themeStore.get().theme) + 1) % ORDER.length]);

export const THEME_LABEL: Record<Theme, string> = { dark: "Dark", light: "Light", night: "Night-vision", system: "System" };

media?.addEventListener("change", apply);
apply();
