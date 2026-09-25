import { Badge, Button } from "../ds";
import { app, visibleEntries } from "../state/app";
import { discard, edits, setOption, stageScrub, targets, write } from "../state/edits";

function Check(p: { label: string; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <label className={`check ${p.disabled ? "disabled" : ""}`}>
      <input type="checkbox" checked={p.checked} disabled={p.disabled} onChange={(e) => p.onChange(e.target.checked)} />
      <span className="box" aria-hidden="true" />
      {p.label}
    </label>
  );
}

export function StagedPanel() {
  const s = edits.use();
  const folderCount = app.use((a) => visibleEntries(a).length);
  const plan = s.plan;
  const n = plan?.changes.length ?? 0;
  const files = s.batch ? targets().length : 1;
  const bytes = (b: number) => (b * 2880).toLocaleString("en-US");

  return (
    <aside className="staged" aria-label="Staged changes">
      <div className="staged-head">
        <span className="ft-card-title">Staged changes</span>
        {n > 0 && <Badge>{n} staged</Badge>}
      </div>

      <section className="ft-card diff" aria-label="Diff">
        {!plan && !s.error && <p className="muted small">Click a value to change it, add a keyword, or use a preset below. Nothing is written until you choose Write.</p>}
        {plan?.changes.map((c, i) => (
          <div key={i} className="diff-rows">
            {(c.kind === "modified" || c.kind === "removed" || c.kind === "renamed") && (
              <div className="diff-line minus">− {c.kind === "renamed" ? c.before : `${c.key} = ${c.before}`}</div>
            )}
            {(c.kind === "modified" || c.kind === "added" || c.kind === "renamed" || c.kind === "history") && (
              <div className="diff-line plus">
                + {c.kind === "renamed" ? c.after : c.kind === "history" ? `HISTORY ${c.after}` : `${c.key} = ${c.after}`}
              </div>
            )}
          </div>
        ))}
        {s.error && <p className="edit-error">{s.error}</p>}
      </section>

      <section className="ft-card options" aria-label="Write options">
        <Check label="Keep a .bak copy" checked={s.options.backup} onChange={(v) => setOption({ backup: v })} />
        <Check label="Add HISTORY audit line" checked={s.options.history} onChange={(v) => setOption({ history: v })} />
        <Check label="Update CHECKSUM / DATASUM" checked={s.options.checksum} onChange={(v) => setOption({ checksum: v })} />
        <Check
          label={`Apply to all ${folderCount.toLocaleString("en-US")} files in this list`}
          checked={s.batch}
          disabled={folderCount < 2}
          onChange={(v) => edits.set({ batch: v })}
        />
      </section>

      <Button variant="ghost" onClick={stageScrub}>
        Privacy scrub: remove site, observer and serials
      </Button>

      {plan && n > 0 && (
        <section className="ft-card note-card">
          <Badge tone={plan.in_place ? "ok" : "teal"}>{plan.in_place ? "Safe" : "Rewrite"}</Badge>{" "}
          {plan.in_place
            ? `Header stays within ${plan.header_blocks_after} block${plan.header_blocks_after === 1 ? "" : "s"} (${bytes(plan.header_blocks_after)} bytes). Rewrite in place; image data untouched.`
            : `Header grows from ${plan.header_blocks_before} to ${plan.header_blocks_after} blocks. Fittle writes a new copy, checks the image data is byte-identical, then swaps it in.`}
        </section>
      )}
      {plan?.consequences.map((c) => (
        <section key={c} className="ft-card note-card">
          <Badge tone="orange">Heads-up</Badge> {c}
        </section>
      ))}
      {plan?.warnings.map((w) => (
        <p key={w} className="edit-warning">
          ⚠ {w}
        </p>
      ))}
      {s.result && (
        <section className="ft-card note-card">
          <Badge tone={s.result.failed.length ? "bad" : "ok"}>{s.result.failed.length ? "Partly written" : "Written"}</Badge>{" "}
          {s.result.ok} file{s.result.ok === 1 ? "" : "s"} updated; image data verified unchanged.
          {s.result.failed.map((f) => (
            <div key={f.path} className="edit-error">
              {f.path.split(/[\\/]/).pop()}: {f.error}
            </div>
          ))}
        </section>
      )}

      <div className="staged-actions">
        <Button onClick={discard} disabled={!s.ops.length || s.busy}>
          Discard
        </Button>
        <Button variant="primary" onClick={write} disabled={!n || !!s.error || s.busy}>
          {s.busy ? "Writing…" : `Write ${n} change${n === 1 ? "" : "s"}${files > 1 ? ` to ${files.toLocaleString("en-US")} files` : ""}`}
        </Button>
      </div>
    </aside>
  );
}
