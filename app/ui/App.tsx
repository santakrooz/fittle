import { useEffect, useState } from "react";
import type { Backend, Card, HeaderDoc, Preview } from "./backend/types";
import { Stage } from "./viewer/Stage";
import type { StretchMode } from "./viewer/stf";

// M0 hello-world: open a FITS file, show it auto-stretched from a float
// texture, and list its header. The full three-pane layout arrives in M2.

const PREVIEW_EDGE = 4096;

type Loaded = { path: string; header: HeaderDoc; preview: Preview | null; error?: string };

export function App({ backend }: { backend: Backend }) {
  const [file, setFile] = useState<Loaded | null>(null);
  const [mode, setMode] = useState<StretchMode>("auto");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    backend.initialPath().then((p) => {
      if (p) void load(p);
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, at startup
  }, []);

  async function openFile() {
    const path = await backend.pickFile();
    if (path) await load(path);
  }

  async function load(path: string) {
    setBusy(true);
    setError(null);
    try {
      const header = await backend.header(path);
      setFile({ path, header, preview: null });
      try {
        const preview = await backend.preview(path, PREVIEW_EDGE);
        setFile({ path, header, preview });
      } catch (e) {
        setFile({ path, header, preview: null, error: String(e) });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const name = file?.path.split(/[\\/]/).pop();

  return (
    <div className="app">
      <header className="titlebar">
        <span className="brand">Fittle</span>
        {name && <span className="crumb">{name}</span>}
        <button className="pill primary" onClick={openFile} disabled={busy}>
          {busy ? "Opening…" : "Open FITS…"}
        </button>
      </header>

      <main className="stage">
        {file?.preview ? (
          <>
            <div className="toolbar" role="toolbar" aria-label="Stretch">
              {(["auto", "linear"] as const).map((m) => (
                <button key={m} className={`pill ${mode === m ? "active" : ""}`} onClick={() => setMode(m)}>
                  {m === "auto" ? "Auto STF" : "Linear"}
                </button>
              ))}
            </div>
            <Stage preview={file.preview} mode={mode} />
            <div className="hud mono">
              {file.preview.info.source_width} × {file.preview.info.source_height}
              {file.preview.info.channels === 3 ? " · RGB" : ""} · display stretch only
            </div>
          </>
        ) : (
          <div className="empty">
            {error ?? file?.error ?? (busy ? "Reading…" : "Open a FITS file to see what it is.")}
          </div>
        )}
      </main>

      <aside className="inspector">{file && <HeaderPanel doc={file.header} />}</aside>
    </div>
  );
}

function HeaderPanel({ doc }: { doc: HeaderDoc }) {
  return (
    <div className="panel">
      {doc.issues.length > 0 && (
        <section className="card">
          <h3 className="label">Header health</h3>
          {doc.issues.map((i, n) => (
            <div key={n} className={`issue ${i.severity}`}>
              {i.code}: {i.message}
            </div>
          ))}
        </section>
      )}
      {doc.hdus.map((hdu) => (
        <section key={hdu.index} className="card">
          <h3 className="label">
            HDU {hdu.index} · {hdu.kind.replace("_", " ")}
            {hdu.shape.length > 0 && ` · ${hdu.shape.join(" × ")}`}
          </h3>
          <table className="cards">
            <tbody>
              {hdu.cards.map((c) => (
                <tr key={c.record}>
                  <td className="mono key">{c.keyword || " "}</td>
                  <td className="mono val">{show(c)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      ))}
    </div>
  );
}

function show(c: Card): string {
  switch (c.type) {
    case "logical":
      return c.value ? "T" : "F";
    case "undefined":
      return "";
    case "complex":
      return `(${c.value[0]}, ${c.value[1]})`;
    default:
      return String(c.value);
  }
}
