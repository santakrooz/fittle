import { useEffect, useMemo, useState } from "react";
import type { Report, Reason, SubGrade } from "../backend/types";
import { Button, Kbd } from "../ds";
import { baseName, group } from "../format";
import { openFile } from "../state/app";
import { openCalMatch } from "../state/calmatch";
import { closeReport, moveRejects, reportStore, saveReport } from "../state/report";

const REASON_LABEL: Record<Reason, string> = {
  clouds: "clouds",
  low_altitude: "low altitude",
  dawn: "dawn",
  trailing: "trailing",
  soft: "soft",
  satellite: "satellite",
  unreadable: "unreadable",
};
const REASON_TONE: Record<Reason, string> = {
  clouds: "rose",
  low_altitude: "amber",
  dawn: "amber",
  trailing: "amber",
  soft: "muted",
  satellite: "amber",
  unreadable: "rose",
};

function hm(s: number) {
  const m = Math.round(s / 60);
  return { h: Math.floor(m / 60), m: m % 60 };
}

function Duration({ s }: { s: number }) {
  const { h, m } = hm(s);
  if (h === 0) return m === 0 ? <>{Math.round(s)}<small>s</small></> : <>{m}<small>m</small></>;
  return (
    <>
      {h}
      <small>h</small> {String(m).padStart(2, "0")}
      <small>m</small>
    </>
  );
}

function shortNight(n: string) {
  const d = new Date(`${n}T12:00:00`);
  return isNaN(d.getTime()) ? n : d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

const openSub = (path: string) => {
  closeReport();
  void openFile(path, true);
};

function Chart({ subs, limit }: { subs: SubGrade[]; limit?: number }) {
  const pts = subs.map((s, i) => ({ s, i, hfr: s.stats?.hfr })).filter((p) => p.hfr != null) as { s: SubGrade; i: number; hfr: number }[];
  if (!pts.length) return <p className="muted small">No stars measured.</p>;
  const W = 1000,
    H = 280,
    L = 34,
    R = 12,
    T = 24,
    B = 34;
  const top = Math.max(Math.ceil(Math.max(...pts.map((p) => p.hfr), (limit ?? 0) * 1.3)), 2);
  const x = (i: number) => L + ((W - L - R) * i) / Math.max(subs.length - 1, 1);
  const y = (v: number) => T + (H - T - B) * (1 - Math.min(v, top) / top);
  // Night spans for separators and labels.
  const spans: { night: string; a: number; b: number }[] = [];
  subs.forEach((s, i) => {
    const n = s.night ?? "";
    const last = spans[spans.length - 1];
    if (last && last.night === n) last.b = i;
    else spans.push({ night: n, a: i, b: i });
  });
  const ticks = Array.from({ length: top }, (_, k) => k + 1);
  return (
    <svg className="hfr-chart" viewBox={`0 0 ${W} ${H}`} role="img" aria-label="HFR per sub in capture order">
      <text x={L - 8} y={12} className="axis" textAnchor="end">px</text>
      {ticks.map((k) => (
        <g key={k}>
          <line x1={L} x2={W - R} y1={y(k)} y2={y(k)} className="grid" />
          <text x={L - 8} y={y(k) + 4} className="axis" textAnchor="end">{k}</text>
        </g>
      ))}
      {spans.map((sp, k) => (
        <g key={sp.night + k}>
          {k > 0 && <line x1={x(sp.a) - 0.5} x2={x(sp.a) - 0.5} y1={T} y2={H - B} className="sep" />}
          <text x={Math.min(Math.max((x(sp.a) + x(sp.b)) / 2, L + 24), W - R - 24)} y={H - 10} className="axis night" textAnchor="middle">{sp.night ? shortNight(sp.night) : ""}</text>
        </g>
      ))}
      {limit != null && (
        <g>
          <line x1={L} x2={W - R} y1={y(limit)} y2={y(limit)} className="limit" />
          <text x={W - R} y={y(limit) - 5} className="limit-label" textAnchor="end">{limit.toFixed(1)} px</text>
        </g>
      )}
      {pts.map((p) => (
        <circle key={p.s.path} cx={x(p.i)} cy={y(p.hfr)} r={p.s.reject ? 2.6 : 2.2} className={p.s.reject ? "rej" : "kept"} onClick={() => openSub(p.s.path)}>
          <title>{`${p.s.name}\nHFR ${p.hfr.toFixed(2)} px · ${p.s.stats?.stars ?? 0} stars${p.s.reasons.length ? ` · ${p.s.reasons.map((r) => REASON_LABEL[r]).join(", ")}` : ""}`}</title>
        </circle>
      ))}
    </svg>
  );
}

export function SessionReport() {
  const open = reportStore.use((s) => s.open);
  const folder = reportStore.use((s) => s.folder);
  const report = reportStore.use((s) => s.report);
  const grading = reportStore.use((s) => s.grading);
  const error = reportStore.use((s) => s.error);
  const night = reportStore.use((s) => s.night);
  const [confirm, setConfirm] = useState(false);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeReport();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);
  useEffect(() => setConfirm(false), [report]);

  const view = useMemo(() => shape(report, night), [report, night]);
  if (!open) return null;
  const g = report?.grading;

  return (
    <section className="report" aria-label="Session report">
      <nav className="report-nights" aria-label="Nights">
        <span className="ft-label">Nights</span>
        <button type="button" className="night" aria-pressed={night === null} onClick={() => reportStore.set({ night: null })}>
          <span>
            <b>All nights</b>
            <small>{group(report ? report.nights.reduce((a, n) => a + n.subs, 0) : 0)} subs</small>
          </span>
          {report && <span className="ft-badge accent">{hm(report.summary.total_light_s).h} h</span>}
        </button>
        {report?.nights.map((n) => (
          <button key={n.night} type="button" className="night" aria-pressed={night === n.night} onClick={() => reportStore.set({ night: n.night })}>
            <span>
              <b>{shortNight(n.night)}</b>
              <small>{group(n.subs)} subs</small>
            </span>
            {n.rejects != null && <i className={`ft-dot ${n.status === "ok" ? "ok" : n.status === "warn" ? "warn" : "bad"}`} title={`${n.rejects} suggested rejects`} />}
          </button>
        ))}
      </nav>

      <div className="report-main">
        <header className="report-head">
          <h1>
            Session report <span className="muted">· {folder ? baseName(folder) : ""}</span>
          </h1>
          <div className="report-actions">
            <Button variant="ghost" disabled={!folder} onClick={() => (closeReport(), void openCalMatch(folder ?? undefined))}>Match calibration</Button>
            <Button variant="secondary" disabled={!report} onClick={() => saveReport("md")}>Export Markdown</Button>
            <Button variant="secondary" disabled={!report} onClick={() => saveReport("html")}>HTML</Button>
            <Button variant="secondary" disabled={!g} onClick={() => saveReport("astrobin")} title="Acquisition CSV for AstroBin (kept subs)">AstroBin CSV</Button>
            <button type="button" className="ex-esc" onClick={closeReport} aria-label="Back to the viewer">
              <Kbd>esc</Kbd>
            </button>
          </div>
        </header>

        {error && <p className="ex-error">{error}</p>}
        {!report && !error && <p className="muted">Reading headers…</p>}

        {report && (
          <>
            <div className="report-tiles">
              <div className="rtile">
                <b>{group(view.subs.length || view.allSubs)}</b>
                <span>Subs found</span>
              </div>
              <div className="rtile">
                <b><Duration s={view.captured} /></b>
                <span>Captured</span>
              </div>
              <div className="rtile good">
                <b>{g ? <Duration s={view.usable} /> : "—"}</b>
                <span>Usable</span>
              </div>
              <div className={`rtile ${view.rejects ? "bad" : ""}`}>
                <b>{g ? group(view.rejects) : "—"}</b>
                <span>Suggested rejects</span>
              </div>
              <div className="rtile">
                <b>{view.medianHfr != null ? <>{view.medianHfr.toFixed(1)}<small>px</small></> : "—"}</b>
                <span>Median HFR</span>
              </div>
            </div>

            <div className="rcard">
              <div className="rcard-head">
                <h2>HFR per sub, in capture order</h2>
                <span className="legend">
                  <i className="dot kept" /> kept <i className="dot rej" /> reject {view.limit != null && <><i className="dash" /> threshold {view.limit.toFixed(1)} px</>}
                </span>
              </div>
              {g ? <Chart subs={view.subs} limit={view.limit} /> : <p className="muted grading">{grading ? `Grading ${group(view.allSubs)} subs: measuring stars, HFR and background…` : "Not graded."}</p>}
            </div>

            <div className="report-bottom">
              <div className="rcard">
                <div className="rcard-head">
                  <h2>Worst subs</h2>
                  {g && g.rejected > 0 &&
                    (confirm ? (
                      <span className="confirm">
                        <span className="muted small">Rename {group(g.rejected)} files into _rejected/ beside them?</span>
                        <Button variant="ghost" onClick={() => setConfirm(false)}>Cancel</Button>
                        <Button variant="primary" onClick={() => void moveRejects()}>Move</Button>
                      </span>
                    ) : (
                      <Button variant="secondary" onClick={() => setConfirm(true)}>Move {group(g.rejected)} to _rejected/</Button>
                    ))}
                </div>
                {!g ? (
                  <p className="muted small">{grading ? "Grading…" : "—"}</p>
                ) : view.worst.length === 0 ? (
                  <p className="muted small">Nothing flagged. Every sub is within the session's limits.</p>
                ) : (
                  <table className="worst">
                    <thead>
                      <tr>
                        <th>File</th>
                        <th className="num">Stars</th>
                        <th className="num">HFR</th>
                        <th className="num">Bkg</th>
                        <th>Reason</th>
                      </tr>
                    </thead>
                    <tbody>
                      {view.worst.map((s) => (
                        <tr key={s.path} onClick={() => openSub(s.path)} title="Open this sub">
                          <td className="mono fn">…{s.name.slice(-24)}</td>
                          <td className="num mono">{s.stats?.stars ?? "—"}</td>
                          <td className="num mono">{s.stats?.hfr?.toFixed(1) ?? "—"}</td>
                          <td className="num mono">{s.stats ? s.stats.background.toFixed(3) : "—"}</td>
                          <td>
                            {s.reasons.map((r) => (
                              <span key={r} className={`reason ${REASON_TONE[r]}`}>{REASON_LABEL[r]}</span>
                            ))}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
              <div className="rcard">
                <div className="rcard-head">
                  <h2>Consistency</h2>
                </div>
                <ul className="checks">
                  {report.checks.map((c) => (
                    <li key={c.text} className={c.status}>
                      <span aria-hidden="true">{c.status === "ok" ? "✓" : c.status === "warn" ? "!" : "×"}</span>
                      {c.text}
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </>
        )}
      </div>
    </section>
  );
}

/** What the screen shows for the selected night (display filtering only). */
function shape(report: Report | null, night: string | null) {
  const g = report?.grading;
  const allSubs = report ? report.nights.reduce((a, n) => a + n.subs, 0) : 0;
  const subs = (g?.subs ?? []).filter((s) => night === null || s.night === night);
  const nights = report?.nights.filter((n) => night === null || n.night === night) ?? [];
  const hfrs = subs.map((s) => s.stats?.hfr).filter((h): h is number => h != null).sort((a, b) => a - b);
  const main = g?.groups.reduce((a, b) => (b.subs > a.subs ? b : a), g.groups[0]);
  return {
    allSubs: night === null ? allSubs : (nights[0]?.subs ?? 0),
    subs,
    captured: nights.reduce((a, n) => a + n.captured_s, 0),
    usable: nights.reduce((a, n) => a + (n.usable_s ?? 0), 0),
    rejects: subs.filter((s) => s.reject).length,
    medianHfr: night === null ? g?.median_hfr : hfrs.length ? hfrs[Math.floor(hfrs.length / 2)] : undefined,
    limit: main?.hfr_max,
    worst: subs.filter((s) => s.reasons.length).sort((a, b) => b.badness - a.badness).slice(0, 8),
  };
}
