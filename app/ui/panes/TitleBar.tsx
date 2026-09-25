import { Kbd } from "../ds";
import { app } from "../state/app";
import { baseName } from "../format";

export function TitleBar() {
  const folder = app.use((s) => s.folder?.name);
  const current = app.use((s) => s.current);
  const mac = navigator.platform.toLowerCase().includes("mac");
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
    </header>
  );
}
