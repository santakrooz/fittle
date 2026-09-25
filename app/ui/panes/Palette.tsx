import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { Card } from "../backend/types";
import { Kbd } from "../ds";
import { baseName } from "../format";
import { app, getBackend, openFile, openFolder, setMode, setStretch, view } from "../state/app";
import { openExport } from "../state/exporter";
import { fuzzy } from "./fuzzy";

type Item = { id: string; group: "Actions" | "Keywords" | "Files"; title: string; hint?: string; cli?: string; run: () => void };

function actions(): Item[] {
  const s = app.get();
  const file = s.current && !s.current.startsWith("demo:") ? `"${baseName(s.current)}"` : "<file>";
  const a: Item[] = [
    { id: "open", group: "Actions", title: "Open file…", cli: "fittle view <file>", run: async () => { const p = await getBackend().pickFile(); if (p) openFile(p); } },
    { id: "folder", group: "Actions", title: "Open folder…", cli: "fittle view <folder>", run: async () => { const p = await getBackend().pickFolder(); if (p) openFolder(p); } },
    ...(s.opened?.image ? [{ id: "export", group: "Actions" as const, title: "Export image…", hint: "⌘E", cli: `fittle export ${file}`, run: () => openExport() }] : []),
    { id: "auto", group: "Actions", title: "Stretch: Auto STF", hint: "A", run: () => setStretch({ kind: "auto" }) },
    { id: "linear", group: "Actions", title: "Stretch: Linear", hint: "L", run: () => setStretch({ kind: "linear" }) },
    { id: "asinh", group: "Actions", title: "Stretch: Asinh", hint: "H", run: () => setStretch({ kind: "asinh" }) },
    { id: "clip", group: "Actions", title: "Toggle clipping overlay", hint: "C", run: () => setStretch({ clipping: !app.get().stretch.clipping }) },
    { id: "fit", group: "Actions", title: "Zoom to fit", hint: "F", run: () => view("fit") },
    { id: "one", group: "Actions", title: "Zoom to actual pixels", hint: "1", run: () => view("one") },
    { id: "overview", group: "Actions", title: "Show overview", cli: `fittle info ${file}`, run: () => app.set({ tab: "overview" }) },
    { id: "header", group: "Actions", title: "Show header", cli: `fittle header ${file}`, run: () => app.set({ tab: "header" }) },
    { id: "hist", group: "Actions", title: "Show histogram", run: () => app.set({ tab: "histogram" }) },
    {
      id: "copyjson",
      group: "Actions",
      title: "Copy header as JSON",
      cli: `fittle header --json ${file}`,
      run: () => { const h = app.get().header; if (h) navigator.clipboard?.writeText(JSON.stringify(h, null, 2)).catch(() => {}); },
    },
    {
      id: "copyinfo",
      group: "Actions",
      title: "Copy overview as JSON",
      cli: `fittle info --json ${file}`,
      run: () => { const i = app.get().opened?.info; if (i) navigator.clipboard?.writeText(JSON.stringify(i, null, 2)).catch(() => {}); },
    },
  ];
  if (s.opened?.image?.can_debayer) {
    a.push({ id: "debayer", group: "Actions", title: s.mode === "debayer" ? "Show raw CFA" : "Debayer preview", hint: "D", run: () => setMode(s.mode === "debayer" ? "raw" : "debayer") });
  }
  return a;
}

function cardText(c: Card) {
  return "value" in c ? String(Array.isArray(c.value) ? c.value.join(", ") : c.value) : "";
}

export function Palette() {
  const open = app.use((s) => s.palette);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQ("");
      setSel(0);
      setTimeout(() => input.current?.focus(), 0);
    }
  }, [open]);

  const results = useMemo(() => {
    if (!open) return [];
    const s = app.get();
    const dict = s.dictionary;
    const keywords: Item[] = (s.header?.hdus[0]?.cards ?? [])
      .filter((c) => c.type !== "commentary")
      .map((c) => ({
        id: `k-${c.record}`,
        group: "Keywords",
        title: c.keyword,
        hint: `= ${cardText(c)}${dict.get(c.keyword) ? ` · ${dict.get(c.keyword)!.label}` : ""}`,
        run: () => app.set({ tab: "header", headerQuery: c.keyword }),
      }));
    const files: Item[] = (s.folder?.entries ?? []).map((e) => ({
      id: `f-${e.path}`,
      group: "Files",
      title: e.name,
      hint: e.label,
      run: () => openFile(e.path, false),
    }));
    const all = [...actions(), ...keywords, ...files];
    const scored = all
      .map((it) => ({ it, m: fuzzy(q, it.title) }))
      .filter((x) => x.m)
      .sort((a, b) => b.m!.score - a.m!.score);
    const cap = { Actions: 8, Keywords: 6, Files: 6 };
    const out: { it: Item; hits: number[] }[] = [];
    for (const g of ["Actions", "Keywords", "Files"] as const) {
      out.push(...scored.filter((x) => x.it.group === g).slice(0, q ? cap[g] : g === "Actions" ? 8 : 0).map((x) => ({ it: x.it, hits: x.m!.hits })));
    }
    return out;
  }, [open, q]);

  if (!open) return null;
  const close = () => app.set({ palette: false });
  const run = (i: number) => {
    const r = results[i];
    if (!r) return;
    close();
    r.it.run();
  };
  const onKey = (e: ReactKeyboardEvent) => {
    if (e.key === "Escape") close();
    else if (e.key === "ArrowDown") (e.preventDefault(), setSel((s) => Math.min(results.length - 1, s + 1)));
    else if (e.key === "ArrowUp") (e.preventDefault(), setSel((s) => Math.max(0, s - 1)));
    else if (e.key === "Enter") run(sel);
  };

  let lastGroup = "";
  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="palette" role="dialog" aria-label="Command palette" onKeyDown={onKey}>
        <div className="pal-q">
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM21 21l-5-5" />
          </svg>
          <input
            ref={input}
            value={q}
            onChange={(e) => (setQ(e.target.value), setSel(0))}
            placeholder="Search actions, keywords, files"
            aria-label="Search"
            role="combobox"
            aria-expanded="true"
            aria-controls="pal-list"
          />
          <Kbd>esc</Kbd>
        </div>
        <div id="pal-list" className="pal-list" role="listbox">
          {results.length === 0 && <p className="pal-empty">Nothing matches “{q}”.</p>}
          {results.map((r, i) => {
            const head = r.it.group !== lastGroup ? ((lastGroup = r.it.group), <div key={`h-${r.it.group}`} className="ft-label pal-group">{r.it.group}</div>) : null;
            return [
              head,
              <button
                key={r.it.id}
                type="button"
                role="option"
                aria-selected={i === sel}
                className="pal-item"
                onMouseEnter={() => setSel(i)}
                onClick={() => run(i)}
              >
                <span className="pal-title">
                  {[...r.it.title].map((ch, k) => (r.hits.includes(k) ? <mark key={k}>{ch}</mark> : ch))}
                  {r.it.hint && <span className="pal-hint"> {r.it.hint}</span>}
                </span>
                {r.it.cli && <span className="pal-cli">{r.it.cli}</span>}
              </button>,
            ];
          })}
        </div>
      </div>
    </div>
  );
}
