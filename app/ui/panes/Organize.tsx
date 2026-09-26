import { useEffect, useState } from "react";
import type { OrganizePlan } from "../backend/types";
import { Button, Kbd } from "../ds";
import { group } from "../format";
import { app, getBackend, notify } from "../state/app";
import { createStore } from "../state/createStore";

export const organizeStore = createStore<{ open: boolean }>({ open: false });
export const openOrganize = () => app.get().folder && organizeStore.set({ open: true });
const close = () => organizeStore.set({ open: false });

const FOLDERS = ["{object}/{filter}/{night}", "{object}/{night}", "{night}/{object}", "{frame}/{object}", ""];
const NAMES = ["", "{object}_{filter}_{exptime}s_{seq}", "{object}_{night}_{seq}", "{night}_{time}_{object}"];

async function refreshFolder() {
  const f = app.get().folder;
  if (!f) return;
  const entries = await getBackend().listFolder(f.path);
  app.set({ folder: { ...f, entries } });
}

export function Organize() {
  const open = organizeStore.use((s) => s.open);
  const folder = app.use((s) => s.folder);
  const [by, setBy] = useState(FOLDERS[0]);
  const [rename, setRename] = useState("");
  const [plan, setPlan] = useState<OrganizePlan | null>(null);
  const [done, setDone] = useState<OrganizePlan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open || !folder) return;
    setDone(null);
    setConfirm(false);
    let live = true;
    const t = setTimeout(() => {
      getBackend()
        .organizePlan(folder.path, by, rename || undefined)
        .then((p) => live && (setPlan(p), setError(null)))
        .catch((e) => live && setError(e instanceof Error ? e.message : String(e)));
    }, 200);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [open, folder, by, rename]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  if (!open || !folder) return null;
  const rel = (p: string) => (p.startsWith(folder.path) ? p.slice(folder.path.length).replace(/^[\\/]/, "") : p);
  const files = plan?.moves.filter((m) => !m.companion) ?? [];
  const companions = (plan?.moves.length ?? 0) - files.length;

  const apply = async () => {
    if (!plan) return;
    setBusy(true);
    try {
      const r = await getBackend().organizeApply(plan);
      const failed = r.moves.filter((m) => m.error).length;
      setDone(r);
      setPlan(null);
      notify(failed ? `Moved ${r.moves.length - failed}; ${failed} left in place` : `Moved ${r.moves.length} files`, failed ? "bad" : "ok");
      await refreshFolder();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
      setConfirm(false);
    }
  };
  const undo = async () => {
    if (!done?.manifest) return;
    setBusy(true);
    try {
      const r = await getBackend().organizeApply(null, done.manifest);
      notify(`Put back ${r.moves.filter((m) => !m.error).length} files`);
      setDone(null);
      await refreshFolder();
      close();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="scrim center" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="organize" role="dialog" aria-modal="true" aria-label="Organize folder">
        <div className="ex-head">
          <h2>Organize {folder.name}</h2>
          <button type="button" className="ex-esc" onClick={close} aria-label="Close">
            <Kbd>esc</Kbd>
          </button>
        </div>
        <label className="ex-group">
          <span className="ex-label">Folders</span>
          <input className="ex-input mono" value={by} spellCheck={false} onChange={(e) => setBy(e.target.value)} placeholder="keep files where they are" />
          <span className="org-presets">
            {FOLDERS.map((f) => (
              <button key={f || "none"} type="button" className="ft-chip" aria-pressed={by === f} onClick={() => setBy(f)}>
                {f || "same folder"}
              </button>
            ))}
          </span>
        </label>
        <label className="ex-group">
          <span className="ex-label">Rename</span>
          <input className="ex-input mono" value={rename} spellCheck={false} onChange={(e) => setRename(e.target.value)} placeholder="keep file names" />
          <span className="org-presets">
            {NAMES.map((f) => (
              <button key={f || "keep"} type="button" className="ft-chip" aria-pressed={rename === f} onClick={() => setRename(f)}>
                {f || "keep names"}
              </button>
            ))}
          </span>
        </label>

        {error && <p className="ex-error">{error}</p>}
        {done ? (
          <div className="org-result">
            <p>
              Moved {group(done.moves.filter((m) => !m.error).length)} files.
              {done.manifest && " You can put everything back."}
            </p>
            {done.moves.filter((m) => m.error).slice(0, 5).map((m) => (
              <p key={m.from} className="ex-error small">{rel(m.from)}: {m.error}</p>
            ))}
          </div>
        ) : plan ? (
          <div className="org-preview">
            <p className="small">
              <b>{group(files.length)}</b> files to move{companions > 0 && <> (+{group(companions)} companion files)</>} · {group(plan.unchanged)} already in place · {group(plan.new_folders.length)} new folders
              {plan.missing_tokens.length > 0 && <span className="ex-warn"> · no {plan.missing_tokens.map((t) => `{${t}}`).join(", ")} in some files (written as “unknown”)</span>}
            </p>
            <ol className="org-list mono">
              {files.slice(0, 12).map((m) => (
                <li key={m.from}>
                  <span className="muted">{rel(m.from)}</span> → {rel(m.to)}
                  {m.note && <span className="ex-warn"> ({m.note})</span>}
                </li>
              ))}
              {files.length > 12 && <li className="muted">… and {group(files.length - 12)} more</li>}
            </ol>
          </div>
        ) : (
          <p className="muted small">Planning…</p>
        )}

        <p className="muted small">Files are renamed, never copied or replaced; companion files (same name, e.g. .jpg) follow. An undo record is saved in the folder.</p>
        <div className="ex-actions">
          {done?.manifest && <Button onClick={() => void undo()} disabled={busy}>Undo</Button>}
          <Button onClick={close}>{done ? "Close" : "Cancel"}</Button>
          {!done &&
            (confirm ? (
              <Button variant="primary" onClick={() => void apply()} disabled={busy || !files.length}>
                {busy ? "Moving…" : `Move ${group(files.length)} files`}
              </Button>
            ) : (
              <Button variant="primary" onClick={() => setConfirm(true)} disabled={!files.length}>
                Organize…
              </Button>
            ))}
        </div>
      </div>
    </div>
  );
}
