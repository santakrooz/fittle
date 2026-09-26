import { useEffect, useState } from "react";
import type { HeaderDiff as Diff } from "../backend/types";
import { Kbd, Segmented } from "../ds";
import { baseName, shortName } from "../format";
import { app, getBackend } from "../state/app";
import { createStore } from "../state/createStore";

export const diffStore = createStore<{ open: boolean; pair: [string, string] | null; diff: Diff | null; error: string | null }>({
  open: false,
  pair: null,
  diff: null,
  error: null,
});

/** Compare the two selected files (a/b in selection order). */
export async function openDiff(pair?: [string, string]) {
  const sel = app.get().selection;
  const p = pair ?? (sel.length === 2 ? ([sel[0], sel[1]] as [string, string]) : null);
  if (!p) return;
  diffStore.set({ open: true, pair: p, diff: null, error: null });
  try {
    diffStore.set({ diff: await getBackend().diffFiles(p[0], p[1]) });
  } catch (e) {
    diffStore.set({ error: e instanceof Error ? e.message : String(e) });
  }
}
const close = () => diffStore.set({ open: false });

export function HeaderDiff() {
  const open = diffStore.use((s) => s.open);
  const pair = diffStore.use((s) => s.pair);
  const diff = diffStore.use((s) => s.diff);
  const error = diffStore.use((s) => s.error);
  const [which, setWhich] = useState<"diff" | "all">("diff");

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!open || !pair) return null;
  const rows = diff?.rows.filter((r) => which === "all" || r.status !== "same" || r.impact) ?? [];
  const notable = diff?.rows.filter((r) => r.status !== "same" || r.impact).length ?? 0;

  return (
    <section className="report diffview" aria-label="Header diff">
      <div className="report-main">
        <header className="report-head">
          <h1>
            Header diff{" "}
            {diff && (
              <span className="muted">
                · {diff.a.verdict} vs {diff.b.verdict}
                {diff.blockers > 0 && <span className="ex-warn"> · {diff.blockers} blocks calibration</span>}
              </span>
            )}
          </h1>
          <div className="report-actions">
            {diff && (
              <Segmented
                label="Rows"
                value={which}
                onChange={setWhich}
                options={[
                  { value: "diff", label: `Differences (${notable})` },
                  { value: "all", label: `All (${diff.rows.length})` },
                ]}
              />
            )}
            <button type="button" className="rail-link" onClick={() => void openDiff([pair[1], pair[0]])}>Swap</button>
            <button type="button" className="ex-esc" onClick={close} aria-label="Close">
              <Kbd>esc</Kbd>
            </button>
          </div>
        </header>
        {error && <p className="ex-error">{error}</p>}
        {!diff && !error && <p className="muted">Reading headers…</p>}
        {diff && (
          <div className="rcard">
            <table className="worst difftable">
              <colgroup>
                <col style={{ width: "14%" }} />
                <col style={{ width: "33%" }} />
                <col style={{ width: "33%" }} />
                <col style={{ width: "20%" }} />
              </colgroup>
              <thead>
                <tr>
                  <th>Keyword</th>
                  <th title={pair[0]}>{shortName(baseName(pair[0]), 40)}</th>
                  <th title={pair[1]}>{shortName(baseName(pair[1]), 40)}</th>
                  <th>Impact</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.keyword} className={r.impact && (r.impact.level === "warning" || r.impact.level === "blocker") ? "flag" : ""}>
                    <td className="mono kw">{r.keyword}</td>
                    <td className="mono">{r.a ?? "—"}</td>
                    <td className="mono">{r.b ?? "—"}</td>
                    <td className={`impact ${r.impact?.level ?? (r.status === "same" ? "ok" : "")}`}>
                      {r.impact?.text ?? (r.status === "same" ? "match" : r.status === "only_a" ? "only in first" : r.status === "only_b" ? "only in second" : "differs")}
                    </td>
                  </tr>
                ))}
                {rows.length === 0 && (
                  <tr>
                    <td colSpan={4} className="muted">The headers match.</td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </section>
  );
}
