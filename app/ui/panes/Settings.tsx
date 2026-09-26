import { useEffect, useState } from "react";
import type { McpSetup, McpTest } from "../backend/types";
import { Button, Kbd, Segmented } from "../ds";
import { app, getBackend, notify } from "../state/app";
import { createStore } from "../state/createStore";
import { setTheme, themeStore, type Theme } from "../state/theme";
import { filmstrip, toggleFilmstrip } from "./Filmstrip";

type Section = "appearance" | "ai" | "about";
export const settingsStore = createStore<{ open: boolean; section: Section }>({ open: false, section: "appearance" });
export const openSettings = (section: Section = "appearance") => settingsStore.set({ open: true, section });
const close = () => settingsStore.set({ open: false });

const ROOTS = "fittle.mcpRoots";
const BIN = "fittle.mcpBin";
function load<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? (JSON.parse(v) as T) : fallback;
  } catch {
    return fallback;
  }
}
function keep(key: string, v: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch {
    /* this session only */
  }
}

function Copy({ text }: { text: string }) {
  const [done, setDone] = useState(false);
  return (
    <Button
      variant="secondary"
      onClick={() =>
        navigator.clipboard
          ?.writeText(text)
          .then(() => (setDone(true), setTimeout(() => setDone(false), 1500)))
          .catch(() => notify("Copy failed", "bad"))
      }
    >
      {done ? "Copied" : "Copy"}
    </Button>
  );
}

function ConnectAI() {
  const folder = app.use((s) => s.folder?.path);
  const [roots, setRoots] = useState<string[]>(() => load(ROOTS, folder ? [folder] : []));
  const [bin, setBin] = useState<string>(() => load(BIN, ""));
  const [setup, setSetup] = useState<McpSetup | null>(null);
  const [client, setClient] = useState("claude-code");
  const [test, setTest] = useState<McpTest | null>(null);
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    let live = true;
    getBackend()
      .mcpSetup(bin || null, roots)
      .then((s) => live && setSetup(s))
      .catch((e) => notify(String(e), "bad"));
    return () => {
      live = false;
    };
  }, [bin, roots]);

  const setAndKeepRoots = (r: string[]) => (setRoots(r), keep(ROOTS, r), setTest(null));
  const addRoot = async () => {
    const d = await getBackend().pickFolder();
    if (d && !roots.includes(d)) setAndKeepRoots([...roots, d]);
  };
  const runTest = async () => {
    if (!setup?.bin) return;
    setTesting(true);
    try {
      setTest(await getBackend().mcpTest(setup.bin, roots));
    } catch (e) {
      setTest({ ok: false, tools: [], elapsed_ms: 0, error: e instanceof Error ? e.message : String(e) });
    } finally {
      setTesting(false);
    }
  };
  const snip = setup?.snippets.find((s) => s.id === client) ?? setup?.snippets[0];

  return (
    <div className="settings-section">
      <p className="muted">
        Fittle includes an MCP server so Claude and other assistants can inspect, preview, grade and (after you approve a dry run) edit your FITS files. It runs on
        this computer: the AI tool starts it with a command, there is no URL and nothing leaves your machine. It can only see the folders you allow below.
      </p>

      <div className="rcard">
        <span className="ex-label">Fittle command</span>
        <input
          className="ex-input mono"
          value={bin || setup?.bin || ""}
          placeholder="not found: install with cargo install --path crates/fittle-cli"
          onChange={(e) => (setBin(e.target.value), keep(BIN, e.target.value), setTest(null))}
          spellCheck={false}
        />
        {!setup?.bin && <p className="ex-error small">No fittle binary found. Build it, install it on your PATH, or paste its path.</p>}
        <span className="ex-label">Allowed folders</span>
        <ul className="roots">
          {roots.map((r) => (
            <li key={r} className="mono">
              {r}
              <button type="button" className="rail-link" onClick={() => setAndKeepRoots(roots.filter((x) => x !== r))} aria-label={`Remove ${r}`}>×</button>
            </li>
          ))}
          {roots.length === 0 && <li className="muted small">None: the server would default to your home folder. Add the folders with your astro data.</li>}
        </ul>
        <div className="row-actions">
          <Button variant="secondary" onClick={() => void addRoot()}>Add folder…</Button>
          {folder && !roots.includes(folder) && <Button variant="ghost" onClick={() => setAndKeepRoots([...roots, folder])}>Add the open folder</Button>}
        </div>
      </div>

      <div className="rcard">
        <div className="client-tabs" role="tablist" aria-label="AI tool">
          {setup?.snippets.map((s) => (
            <button key={s.id} type="button" role="tab" aria-selected={s.id === (snip?.id ?? "")} className="ft-chip" aria-pressed={s.id === snip?.id} onClick={() => setClient(s.id)}>
              {s.client}
            </button>
          ))}
        </div>
        {snip && (
          <>
            <p className="small">{snip.how}</p>
            {snip.file && (
              <p className="small muted">
                File: <span className="mono">{snip.file}</span>
              </p>
            )}
            <pre className="snippet mono">{snip.text}</pre>
            <div className="row-actions">
              <Copy text={snip.text} />
            </div>
          </>
        )}
      </div>

      <div className="rcard">
        <div className="rcard-head">
          <h2>Test the connection</h2>
          <Button variant="secondary" disabled={!setup?.bin || testing} onClick={() => void runTest()}>
            {testing ? "Testing…" : "Test"}
          </Button>
        </div>
        {test &&
          (test.ok ? (
            <p className="small ok-line">
              ✓ {test.server} answered with {test.tools.length} tools in {test.elapsed_ms} ms: <span className="mono muted">{test.tools.join(", ")}</span>
            </p>
          ) : (
            <p className="ex-error small">✗ {test.error}</p>
          ))}
        <p className="muted small">Tools that change files are dry runs first: the assistant shows you the plan and only writes after you agree. Header edits keep backups; exports never overwrite.</p>
      </div>
    </div>
  );
}

export function Settings() {
  const open = settingsStore.use((s) => s.open);
  const section = settingsStore.use((s) => s.section);
  const theme = themeStore.use((s) => s.theme);
  const strip = filmstrip.use((s) => s.on);
  const [version, setVersion] = useState("");

  useEffect(() => {
    if (!open) return;
    getBackend()
      .mcpSetup(null, [])
      .then((s) => setVersion(s.version))
      .catch(() => {});
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !(e.target instanceof HTMLInputElement) && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!open) return null;
  const nav: { id: Section; label: string }[] = [
    { id: "appearance", label: "Appearance" },
    { id: "ai", label: "Connect AI tools" },
    { id: "about", label: "About" },
  ];
  return (
    <section className="report settings" aria-label="Settings">
      <nav className="report-nights" aria-label="Settings sections">
        <span className="ft-label">Settings</span>
        {nav.map((n) => (
          <button key={n.id} type="button" className="night" aria-pressed={section === n.id} onClick={() => settingsStore.set({ section: n.id })}>
            <span><b>{n.label}</b></span>
          </button>
        ))}
      </nav>
      <div className="report-main">
        <header className="report-head">
          <h1>{nav.find((n) => n.id === section)?.label}</h1>
          <button type="button" className="ex-esc" onClick={close} aria-label="Close settings">
            <Kbd>esc</Kbd>
          </button>
        </header>
        {section === "appearance" && (
          <div className="settings-section">
            <div className="rcard">
              <span className="ex-label">Theme</span>
              <Segmented<Theme>
                label="Theme"
                value={theme}
                onChange={setTheme}
                options={[
                  { value: "dark", label: "Dark" },
                  { value: "light", label: "Light" },
                  { value: "night", label: "Night-vision" },
                  { value: "system", label: "System" },
                ]}
              />
              <p className="muted small">Night-vision turns the whole window, images included, deep red so it won't spoil dark adaptation at the telescope.</p>
            </div>
            <div className="rcard">
              <label className="check">
                <input type="checkbox" checked={strip} onChange={toggleFilmstrip} /> Show a filmstrip under the viewer
              </label>
              <p className="muted small">The file rail already shows thumbnails; blink always has its own strip.</p>
            </div>
          </div>
        )}
        {section === "ai" && <ConnectAI />}
        {section === "about" && (
          <div className="settings-section">
            <div className="rcard">
              <p>
                <b>Fittle</b> {version && <span className="mono muted">{version}</span>}
              </p>
              <p className="muted small">Explain, inspect and convert astrophotography FITS files. MIT licence. OpenNGC catalogue data CC BY-SA 4.0; fonts SIL OFL.</p>
              <p className="muted small">No accounts, no telemetry. Network use only for optional catalogue or plate-solve lookups.</p>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}
