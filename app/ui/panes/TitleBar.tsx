import { Kbd } from "../ds";
import { app } from "../state/app";
import { baseName } from "../format";
import { THEME_LABEL, cycleTheme, themeStore } from "../state/theme";

export function TitleBar() {
  const folder = app.use((s) => s.folder?.name);
  const current = app.use((s) => s.current);
  const mac = navigator.platform.toLowerCase().includes("mac");
  const theme = themeStore.use((s) => s.theme);
  return (
    <header className="titlebar" data-tauri-drag-region>
      <span className="brand" aria-hidden="true" data-tauri-drag-region />
      <nav className="crumbs" aria-label="Location" data-tauri-drag-region>
        {folder && <span className="crumb" data-tauri-drag-region>{folder}</span>}
        {folder && current && <span className="crumb-sep" data-tauri-drag-region>›</span>}
        {current && <span className="crumb current" data-tauri-drag-region>{current.startsWith("demo:") ? "" : baseName(current)}</span>}
      </nav>
      <button type="button" className="search-pill" onClick={() => app.set({ palette: true })}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path d="M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM21 21l-5-5" />
        </svg>
        <span>Search files, keys, actions</span>
        <Kbd>{mac ? "⌘K" : "Ctrl K"}</Kbd>
      </button>
      <button type="button" className="theme-btn" onClick={cycleTheme} title={`Theme: ${THEME_LABEL[theme]} (click to change)`} aria-label={`Theme: ${THEME_LABEL[theme]}`}>
        <svg viewBox="0 0 24 24" aria-hidden="true">
          {theme === "light" ? (
            <path d="M12 7a5 5 0 1 1 0 10 5 5 0 0 1 0-10zM12 1v3M12 20v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M1 12h3M20 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1" />
          ) : theme === "night" ? (
            <path d="M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 7v5l3 2" />
          ) : theme === "system" ? (
            <path d="M3 5h18v11H3zM8 20h8M12 16v4" />
          ) : (
            <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z" />
          )}
        </svg>
      </button>
    </header>
  );
}
