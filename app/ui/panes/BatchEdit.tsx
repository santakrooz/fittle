import { useEffect, useMemo, useState } from "react";
import type { KeyDist, NewValue, Op } from "../backend/types";
import { Button, Kbd, Segmented } from "../ds";
import { group } from "../format";
import { app } from "../state/app";
import { applyBatch, batch, closeBatch, queue, queueRig, saveRig, setOptions, unqueue } from "../state/batch";

function valueOf(text: string): NewValue {
  const t = text.trim();
  if (/^'.*'$/.test(t)) return { type: "string", value: t.slice(1, -1).replace(/''/g, "'") };
  if (t === "T" || t === "F") return { type: "logical", value: t === "T" };
  if (/^[+-]?\d+$/.test(t)) return { type: "integer", value: Number(t) };
  if (/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(t)) return { type: "float", value: Number(t) };
  return { type: "string", value: t };
}

function show(v: NewValue) {
  if (v.type === "string" || v.type === "auto") return `'${v.value}'`;
  if (v.type === "logical") return v.value ? "T" : "F";
  if (v.type === "float") return Number.isInteger(v.value) ? `${v.value}.0` : String(v.value);
  return String(v.value);
}

function opText(o: Op) {
  if (o.op === "set") return <>set <b>{o.key}</b> = {show(o.value)}</>;
  if (o.op === "unset") return <>remove <b>{o.key}</b></>;
  if (o.op === "rename") return <>rename <b>{o.from}</b> → <b>{o.to}</b></>;
  return <>history</>;
}

function values(k: KeyDist) {
  if (k.spread === "same") return k.values[0]?.text;
  if (k.spread === "range") return `${k.min} – ${k.max}`;
  if (k.spread === "unique") return `${k.values[0]?.text} … (${group(k.distinct)} distinct)`;
  return k.values.map((v) => `${v.text} (${group(v.count)})`).join(" · ");
}

function Bar({ k, files }: { k: KeyDist; files: number }) {
  if (k.spread === "unique") return <span className="dist-bar unique" />;
  if (k.spread === "range") return <span className="dist-bar range" />;
  return (
    <span className="dist-bar">
      {k.values.map((v, i) => (
        <i key={v.text} className={`seg s${i % 4}`} style={{ flex: v.count }} title={`${v.text}: ${group(v.count)}`} />
      ))}
      {k.present < files && <i className="seg missing" style={{ flex: files - k.present }} title={`missing on ${group(files - k.present)}`} />}
    </span>
  );
}

export function BatchEdit() {
  const open = batch.use((s) => s.open);
  const dist = batch.use((s) => s.dist);
  const ops = batch.use((s) => s.ops);
  const plan = batch.use((s) => s.plan);
  const rigs = batch.use((s) => s.rigs);
  const busy = batch.use((s) => s.busy);
  const error = batch.use((s) => s.error);
  const options = batch.use((s) => s.options);
  const selection = app.use((s) => s.selection);
  const folder = app.use((s) => s.folder);
  const [tab, setTab] = useState<"edit" | "rigs">("edit");
  const [key, setKey] = useState("");
  const [val, setVal] = useState("");
  const [mode, setMode] = useState<"set" | "unset" | "rename">("set");
  const [rigName, setRigName] = useState("");
  const [rigPick, setRigPick] = useState("");
  const [confirm, setConfirm] = useState(false);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !(e.target instanceof HTMLInputElement) && closeBatch();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);
  useEffect(() => setConfirm(false), [ops]);

  const pending = useMemo(() => {
    const m = new Map<string, Op>();
    for (const o of ops) m.set(o.op === "rename" ? o.from : o.op === "history" ? "" : o.key, o);
    return m;
  }, [ops]);
  const mixed = dist?.keys.filter((k) => k.spread === "mixed" && ["GAIN", "OFFSET", "EXPTIME", "FILTER", "XBINNING", "INSTRUME", "CCD-TEMP"].includes(k.keyword)) ?? [];
  const missing = dist?.keys.filter((k) => ["FOCALLEN", "XPIXSZ", "OBJECT"].includes(k.keyword) && (k.present < dist.files || k.values.some((v) => /^0(\.0*)?$/.test(v.text)))) ?? [];

  if (!open) return null;
  const rig = rigs.find((r) => r.name === rigPick) ?? rigs[0];

  const add = () => {
    const k = key.trim().toUpperCase();
    if (!k) return;
    if (mode === "set") queue({ op: "set", key: k, value: valueOf(val), comment: null });
    else if (mode === "unset") queue({ op: "unset", key: k });
    else if (val.trim()) queue({ op: "rename", from: k, to: val.trim().toUpperCase() });
    setKey("");
    setVal("");
  };

  return (
    <section className="report batch" aria-label="Batch edit">
      <div className="batch-table-wrap">
        <header className="report-head">
          <h1>
            {group(selection.length)} files selected <span className="muted">in {folder?.name}</span>
          </h1>
          <button type="button" className="ex-esc" onClick={closeBatch} aria-label="Back to the viewer">
            <Kbd>esc</Kbd>
          </button>
        </header>
        {error && <p className="ex-error">{error}</p>}
        {!dist ? (
          <p className="muted">Reading {group(selection.length)} headers…</p>
        ) : (
          <table className="worst batch-table">
            <colgroup>
              <col style={{ width: "16%" }} />
              <col style={{ width: "44%" }} />
              <col style={{ width: "26%" }} />
              <col style={{ width: "14%" }} />
            </colgroup>
            <thead>
              <tr>
                <th>Keyword</th>
                <th>Values across selection</th>
                <th>Distribution</th>
                <th>Action</th>
              </tr>
            </thead>
            <tbody>
              {dist.keys.map((k) => {
                const p = pending.get(k.keyword);
                const action = p ? (p.op === "set" ? (k.present ? "set" : "add") : p.op === "unset" ? "remove" : "rename") : k.present < dist.files ? "partial" : k.spread;
                return (
                  <tr key={k.keyword} className={p ? `queued ${action}` : ""} onClick={() => (setKey(k.keyword), setVal(k.spread === "same" ? (k.values[0]?.text ?? "") : ""), setMode("set"), setTab("edit"))}>
                    <td className="mono kw">{k.keyword}</td>
                    <td className="mono vals">
                      {p?.op === "set" ? <>{values(k)} <span className="to">→ {show(p.value)}</span></> : values(k)}
                    </td>
                    <td><Bar k={k} files={dist.files} /></td>
                    <td className={`act ${action}`}>{action}</td>
                  </tr>
                );
              })}
              {[...pending.values()].filter((o) => o.op === "set" && !dist.keys.some((k) => k.keyword === o.key)).map((o) =>
                o.op === "set" ? (
                  <tr key={o.key} className="queued add">
                    <td className="mono kw">{o.key}</td>
                    <td className="mono vals to">{show(o.value)}</td>
                    <td><span className="dist-bar"><i className="seg add" style={{ flex: 1 }} /></span></td>
                    <td className="act add">add</td>
                  </tr>
                ) : null,
              )}
            </tbody>
          </table>
        )}
      </div>

      <aside className="batch-side">
        <Segmented label="Panel" value={tab} onChange={setTab} options={[{ value: "edit", label: "Batch edit" }, { value: "rigs", label: "Rig profiles" }]} />

        {tab === "rigs" && (
          <div className="rcard">
            <span className="ex-label">Apply rig profile</span>
            <select className="ex-input" value={rig?.name ?? ""} onChange={(e) => setRigPick(e.target.value)}>
              {rigs.map((r) => (
                <option key={r.name} value={r.name}>{r.name}{r.builtin ? "" : " (saved)"}</option>
              ))}
            </select>
            {rig && <p className="mono small rig-vals">{Object.entries(rig.values).map(([k, v]) => `${k} ${typeof v === "string" ? `'${v}'` : v}`).join(" · ")}</p>}
            <Button variant="secondary" disabled={!rig} onClick={() => rig && (queueRig(rig), setTab("edit"))}>Queue these values</Button>
            <hr />
            <span className="ex-label">Save a rig from the first selected file</span>
            <div className="add-op">
              <input className="ex-input" placeholder="e.g. RedCat · L-eXtreme" value={rigName} onChange={(e) => setRigName(e.target.value)} />
              <Button variant="secondary" disabled={!rigName.trim()} onClick={() => (void saveRig(rigName), setRigName(""))}>Save</Button>
            </div>
            <p className="muted small">Captures scope, camera, optics, filter and gain keywords.</p>
          </div>
        )}

        {tab === "edit" && (
          <>
            <div className="rcard">
              <div className="rcard-head">
                <h2>Operations</h2>
                <span className="ft-label">{ops.length} queued</span>
              </div>
              <ul className="ops">
                {ops.map((o, i) => (
                  <li key={i} className="mono">
                    <span className="plus">+</span> {opText(o)}
                    <button type="button" className="rail-link" onClick={() => unqueue(i)} aria-label="Remove">×</button>
                  </li>
                ))}
              </ul>
              <div className="add-op">
                <select className="ex-input mode" value={mode} onChange={(e) => setMode(e.target.value as typeof mode)} aria-label="Operation">
                  <option value="set">set</option>
                  <option value="unset">remove</option>
                  <option value="rename">rename</option>
                </select>
                <input className="ex-input mono" placeholder="KEYWORD" value={key} onChange={(e) => setKey(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
                {mode !== "unset" && (
                  <input className="ex-input mono" placeholder={mode === "set" ? "value ('text' or 250.0)" : "NEWNAME"} value={val} onChange={(e) => setVal(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
                )}
                <Button variant="secondary" onClick={add} disabled={!key.trim()}>Add</Button>
              </div>
            </div>

            {mixed.map((k) => {
              const minor = k.values.slice(1);
              return (
                <div key={k.keyword} className="rcard note">
                  <span className="ft-badge warn">Mixed {k.keyword.toLowerCase()}</span>{" "}
                  {minor.map((v) => `${group(v.count)} ${v.count === 1 ? "file uses" : "files use"} ${k.keyword} ${v.text}`).join("; ")}.
                  {minor[0]?.paths?.length ? (
                    <> <button type="button" className="rail-link" onClick={() => (app.set({ selection: minor.flatMap((v) => v.paths ?? []) }), closeBatch())}>Select them</button></>
                  ) : null}
                </div>
              );
            })}
            {missing.map((k) => (
              <div key={k.keyword} className="rcard note">
                <span className="ft-badge bad">{k.keyword}</span> missing or zero on some files; stackers can't compute pixel scale. A rig profile sets it.
              </div>
            ))}

            <div className="rcard">
              <div className="rcard-head">
                <h2>Dry run</h2>
                {plan && <span className={`ft-badge ${plan.errors.length ? "bad" : "ok"}`}>{plan.errors.length ? `${plan.errors.length} problems` : "OK"}</span>}
              </div>
              {!plan ? (
                <p className="muted small">Queue an operation to see what would change.</p>
              ) : (
                <>
                  <dl className="gradegrid">
                    <div><dt>Files changed</dt><dd>{group(plan.changed)}</dd></div>
                    <div><dt>In place</dt><dd>{group(plan.in_place)} <small>{plan.rewrite ? `${group(plan.rewrite)} rewritten` : "no resize"}</small></dd></div>
                    <div><dt>Est. time</dt><dd>~{Math.max(1, Math.ceil(plan.est_seconds))} s</dd></div>
                    <div><dt>Backups</dt><dd>{options.backup ? <>.bak <small>+ {(plan.backup_bytes / 1e6).toFixed(0)} MB</small></> : "off"}</dd></div>
                  </dl>
                  {plan.notes.map((n) => <p key={n} className="small warn-line">! {n}</p>)}
                  {plan.errors.slice(0, 3).map(([p, e]) => <p key={p} className="ex-error small">{p.split(/[\\/]/).pop()}: {e}</p>)}
                </>
              )}
              <label className="small check">
                <input type="checkbox" checked={options.backup} onChange={(e) => setOptions({ backup: e.target.checked })} /> Keep .bak backups
              </label>
            </div>

            <div className="ex-actions">
              <Button variant="secondary" disabled={!plan} onClick={() => plan && (navigator.clipboard?.writeText(plan.cli).catch(() => {}), void 0)} title={plan?.cli}>
                Copy as CLI
              </Button>
              {confirm ? (
                <Button variant="primary" disabled={busy} onClick={() => (setConfirm(false), void applyBatch())}>
                  {busy ? "Writing…" : `Write ${group(plan?.changed ?? 0)} files`}
                </Button>
              ) : (
                <Button variant="primary" disabled={!plan || !!plan.errors.length || !plan.changed} onClick={() => setConfirm(true)}>
                  Apply to {group(selection.length)}
                </Button>
              )}
            </div>
          </>
        )}
      </aside>
    </section>
  );
}
