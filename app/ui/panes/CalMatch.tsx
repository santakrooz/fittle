import { useEffect } from "react";
import type { KindMatch, LightGroup } from "../backend/types";
import { Button, Kbd } from "../ds";
import { baseName, group, num } from "../format";
import { calStore, chooseLibrary, closeCalMatch } from "../state/calmatch";

function Cell({ k }: { k: KindMatch }) {
  if (k.status === "none") return <span className="calchip bad">none</span>;
  const name = k.set?.name ?? "—";
  const title = [name, ...k.notes].join("\n");
  // Mismatches show what's wrong; matches show which set.
  if (k.status === "bad")
    return (
      <span className="calchip bad" title={title}>
        {k.notes[0] ?? name}
      </span>
    );
  return (
    <span className={`calchip ${k.status === "ok" ? "ok" : "warn"}`} title={title}>
      {name}
      {k.notes.length > 0 && <small> · {k.notes[0]}</small>}
    </span>
  );
}

function label(g: LightGroup) {
  const parts: string[] = [];
  if (g.gain != null) parts.push(`Gain ${num(g.gain, 2)}`);
  if (g.exposure_s != null) parts.push(`${num(g.exposure_s, 2)} s`);
  if (g.filter) parts.push(g.filter);
  parts.push(`${group(g.subs)} subs`);
  return parts.join(" · ");
}

const STATUS = { ready: ["Ready", "ok"], partial: ["Partial", "warn"], missing: ["Missing", "bad"] } as const;

export function CalMatch() {
  const open = calStore.use((s) => s.open);
  const lights = calStore.use((s) => s.lights);
  const library = calStore.use((s) => s.library);
  const result = calStore.use((s) => s.result);
  const busy = calStore.use((s) => s.busy);
  const error = calStore.use((s) => s.error);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeCalMatch();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!open) return null;
  const total = result?.groups.reduce((a, g) => a + g.subs, 0) ?? 0;
  const notReady = result?.groups.filter((g) => g.why) ?? [];

  return (
    <section className="report calmatch" aria-label="Match calibration">
      <div className="report-main">
        <header className="report-head">
          <h1>
            Match calibration{" "}
            <span className="muted">
              · {lights ? baseName(lights) : "no folder"}
              {result && ` (${group(total)} lights)`} → {library ? baseName(library) : "choose a library"}
              {result && ` (${group(result.library_frames)} frames)`}
            </span>
          </h1>
          <div className="report-actions">
            <Button variant="secondary" onClick={() => void chooseLibrary()}>
              {library ? "Change library…" : "Choose library…"}
            </Button>
            <button type="button" className="ex-esc" onClick={closeCalMatch} aria-label="Back to the viewer">
              <Kbd>esc</Kbd>
            </button>
          </div>
        </header>
        {error && <p className="ex-error">{error}</p>}
        {!lights && <p className="muted">Open a folder of lights first.</p>}
        {busy && <p className="muted">Reading headers…</p>}
        {result && (
          <>
            <div className="report-tiles four">
              <div className="rtile">
                <b>{result.groups.length}</b>
                <span>Light groups</span>
              </div>
              <div className="rtile good">
                <b>{result.ready}</b>
                <span>Fully matched</span>
              </div>
              <div className={`rtile ${result.partial ? "warn" : ""}`}>
                <b>{result.partial + result.missing}</b>
                <span>{result.missing ? "Partial or missing" : "Partial"}</span>
              </div>
              <div className="rtile">
                <b>{group(result.library_frames)}</b>
                <span>Cal frames scanned</span>
              </div>
            </div>
            <div className="rcard">
              {result.groups.length === 0 ? (
                <p className="muted small">No light subs in this folder.</p>
              ) : (
                <table className="worst caltable">
                  <colgroup>
                    <col style={{ width: "24%" }} />
                    <col style={{ width: "23%" }} />
                    <col style={{ width: "22%" }} />
                    <col style={{ width: "21%" }} />
                    <col style={{ width: "10%" }} />
                  </colgroup>
                  <thead>
                    <tr>
                      <th>Light group</th>
                      <th>Darks</th>
                      <th>Flats</th>
                      <th>Bias / dark-flat</th>
                      <th>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {result.groups.map((g, i) => (
                      <tr key={i}>
                        <td>{label(g)}</td>
                        <td><Cell k={g.darks} /></td>
                        <td><Cell k={g.flats} /></td>
                        <td><Cell k={g.bias} /></td>
                        <td><span className={`calchip ${STATUS[g.status][1]}`}>{STATUS[g.status][0]}</span></td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </div>
            {notReady.map((g, i) => (
              <div key={i} className="rcard why">
                <b>Why {g.status === "missing" ? "missing" : "partial"}{notReady.length > 1 ? ` (${label(g)})` : ""}:</b> {g.why}
              </div>
            ))}
          </>
        )}
      </div>
    </section>
  );
}
