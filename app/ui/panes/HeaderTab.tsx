import { useMemo, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { Card, Change, KeywordInfo, NewValue } from "../backend/types";
import { Button, Icon, Segmented } from "../ds";
import { app } from "../state/app";
import { edits, setEditing, stage, unstage } from "../state/edits";
import { StagedPanel } from "./StagedPanel";

const GROUP_ORDER = ["structure", "target", "optics", "camera", "filter", "exposure", "time", "site", "mount", "astrometry", "processing", "software", "other"];
const GROUP_LABEL: Record<string, string> = { mount: "Mount / guiding" };

function lookup(dict: Map<string, KeywordInfo>, key: string): KeywordInfo | undefined {
  const exact = dict.get(key);
  if (exact) return exact;
  const base = key.replace(/\d+$/, "");
  if (base !== key) {
    const ij = /^(.*?)\d+_$/.exec(base);
    return (ij && dict.get(`${ij[1]}i_j`)) || dict.get(`${base}n`);
  }
  return undefined;
}

/** Mirrors fittle_core::edit::is_locked for display; the core enforces it. */
const MANAGED = new Set(["BZERO", "BSCALE", "BLANK", "CHECKSUM", "DATASUM", "CONTINUE", "END"]);
function locked(info: KeywordInfo | undefined, key: string, compressed: boolean): boolean {
  if (info?.structural || MANAGED.has(key)) return true;
  if (/^(TFORM|TTYPE|TBCOL|TDIM|TZERO|TSCAL|TNULL)\d+$/.test(key) || key === "TFIELDS" || key === "THEAP") return true;
  return compressed && key.startsWith("Z");
}

function valueText(c: Card): string {
  switch (c.type) {
    case "string":
      return `'${c.value}'`;
    case "logical":
      return c.value ? "T" : "F";
    case "undefined":
      return "";
    case "complex":
      return `(${c.value[0]}, ${c.value[1]})`;
    case "commentary":
      return c.value;
    default:
      return String(c.value);
  }
}

/** The text a person edits: strings without quotes. */
const editText = (c: Card) => (c.type === "string" ? c.value : valueText(c));

/** Keep a string a string; infer everything else. */
function toValue(c: Card | null, text: string): NewValue {
  return c?.type === "string" ? { type: "string", value: text } : { type: "auto", value: text };
}

function ValueCell({ card, editable }: { card: Card; editable: boolean }) {
  const [editing, setEditingCell] = useState(false);
  const [text, setText] = useState("");
  if (!editable) return <>{valueText(card)}</>;
  if (editing) {
    const commit = () => {
      setEditingCell(false);
      if (text !== editText(card)) stage({ op: "set", key: card.keyword, value: toValue(card, text), comment: null });
    };
    const onKey = (e: ReactKeyboardEvent) => {
      if (e.key === "Enter") commit();
      else if (e.key === "Escape") setEditingCell(false);
    };
    return (
      <input
        className="ft-input cell-input"
        autoFocus
        onFocus={(e) => e.currentTarget.select()}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={onKey}
        aria-label={`New value for ${card.keyword}`}
      />
    );
  }
  return (
    <button
      type="button"
      className="cell-edit"
      title="Click to edit"
      onClick={() => {
        setText(editText(card));
        setEditingCell(true);
      }}
    >
      {valueText(card)}
    </button>
  );
}

function AddRow() {
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const add = () => {
    const k = key.trim().toUpperCase();
    if (!k) return;
    stage({ op: "set", key: k, value: { type: "auto", value }, comment: null });
    setKey("");
    setValue("");
  };
  return (
    <div className="add-row">
      <input className="ft-input mono" placeholder="KEYWORD" maxLength={8} value={key} onChange={(e) => setKey(e.target.value.toUpperCase())} aria-label="New keyword" />
      <input
        className="ft-input mono"
        placeholder="value (quote with '…' to force text)"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && add()}
        aria-label="New value"
      />
      <Button onClick={add} disabled={!key.trim()}>
        Add
      </Button>
    </div>
  );
}

export function HeaderTab() {
  const doc = app.use((s) => s.header);
  const dict = app.use((s) => s.dictionary);
  const query = app.use((s) => s.headerQuery);
  const ignored = app.use((s) => s.opened?.info.fields.ignored);
  const imageHdu = app.use((s) => s.opened?.info.image?.hdu ?? 0);
  const canEdit = app.use((s) => !!s.current);
  const editing = edits.use((s) => s.editing);
  const plan = edits.use((s) => s.plan);
  const editError = edits.use((s) => s.error);
  const [view, setView] = useState<"grouped" | "raw">("grouped");
  const [hdu, setHdu] = useState<number | null>(null);
  const h = doc?.hdus[Math.min(hdu ?? imageHdu, (doc?.hdus.length ?? 1) - 1)];
  const byKey = useMemo(() => {
    const m = new Map<string, Change[]>();
    for (const c of plan?.hdu === h?.index ? (plan?.changes ?? []) : []) {
      const k = c.kind === "renamed" ? c.key.split(" → ")[0] : c.key;
      m.set(k, [...(m.get(k) ?? []), c]);
    }
    return m;
  }, [plan, h]);

  const groups = useMemo(() => {
    if (!h) return [];
    const q = query.trim().toUpperCase();
    const cards = h.cards.filter(
      (c) => !q || c.keyword.includes(q) || valueText(c).toUpperCase().includes(q) || (lookup(dict, c.keyword)?.label.toUpperCase().includes(q) ?? false),
    );
    const by = new Map<string, Card[]>();
    for (const c of cards) {
      const g = c.type === "commentary" ? "history" : (lookup(dict, c.keyword)?.group ?? "other");
      by.set(g, [...(by.get(g) ?? []), c]);
    }
    return [...GROUP_ORDER, "history"].filter((g) => by.has(g)).map((g) => ({ g, cards: by.get(g)! }));
  }, [h, dict, query]);

  if (!doc || !h) return <p className="inspector-empty">No header loaded.</p>;
  const count = groups.reduce((a, g) => a + g.cards.length, 0);
  const compressed = h.kind === "compressed_image";
  const added = [...byKey.values()].flat().filter((c) => c.kind === "added");

  const table = (
    <div className="header-scroll">
      {view === "raw" ? (
        <pre className="raw-cards">
          {h.cards
            .filter((c) => !query || c.raw.join("\n").toUpperCase().includes(query.toUpperCase()))
            .flatMap((c) => c.raw)
            .join("\n")}
          {"\nEND"}
        </pre>
      ) : (
        <table className={`htable ${editing ? "editing" : ""}`}>
          <tbody>
            {editing && added.length > 0 && [
              <tr key="added" className="grp">
                <td colSpan={4}>Added</td>
              </tr>,
              ...added.map((c) => (
                <tr key={`add-${c.key}`} className="add">
                  <td className="key">{c.key}</td>
                  <td className="val">{c.after}</td>
                  <td className="c">{lookup(dict, c.key)?.label ?? ""}</td>
                  <td className="act">
                    <button type="button" className="row-btn" title="Remove from staged" onClick={() => unstage(c.key)}>
                      ↺
                    </button>
                  </td>
                </tr>
              )),
            ]}
            {groups.map(({ g, cards }) => [
              <tr key={g} className="grp">
                <td colSpan={editing ? 4 : 3}>{g === "history" ? "History & comments" : (GROUP_LABEL[g] ?? g[0].toUpperCase() + g.slice(1))}</td>
              </tr>,
              ...cards.map((c) => {
                const info = lookup(dict, c.keyword);
                const skip = h.index === imageHdu ? ignored?.find((x) => x.key === c.keyword) : undefined;
                const isLocked = locked(info, c.keyword, compressed);
                const staged = byKey.get(c.keyword);
                const state = staged?.some((x) => x.kind === "removed" || x.kind === "renamed")
                  ? "del"
                  : staged?.some((x) => x.kind === "modified")
                    ? "mod"
                    : "";
                const newValue = staged?.find((x) => x.kind === "modified")?.after;
                const editable = editing && h.index === imageHdu && !isLocked && c.type !== "commentary" && !c.hierarch;
                return (
                  <tr key={`${g}-${c.record}`} className={state} title={skip ? skip.reason : info?.help}>
                    <td className="key">
                      {c.keyword || "(blank)"}
                      {isLocked && c.type !== "commentary" && (
                        <span className="lock" title="Locked: describes the data layout or is managed by Fittle">
                          <Icon name="lock" />
                        </span>
                      )}
                    </td>
                    <td className="val">
                      {newValue && state === "mod" ? (
                        <span className="new-value" title={`was ${valueText(c)}`}>
                          {newValue}
                        </span>
                      ) : (
                        <ValueCell card={c} editable={editable && state !== "del"} />
                      )}
                      {skip ? (
                        <span className="ft-badge warn ignored">ignored</span>
                      ) : (
                        !editing && info?.unit && c.type !== "string" && c.type !== "commentary" && <span className="unit"> {info.unit}</span>
                      )}
                    </td>
                    <td className="c">{info?.label ?? c.comment ?? ""}</td>
                    {editing && (
                      <td className="act">
                        {state ? (
                          <button type="button" className="row-btn" title="Undo this change" onClick={() => unstage(c.keyword)}>
                            ↺
                          </button>
                        ) : (
                          editable && (
                            <button type="button" className="row-btn" title={`Remove ${c.keyword}`} onClick={() => stage({ op: "unset", key: c.keyword })}>
                              ✕
                            </button>
                          )
                        )}
                      </td>
                    )}
                  </tr>
                );
              }),
            ])}
          </tbody>
        </table>
      )}
    </div>
  );

  return (
    <div className={`header-tab ${editing ? "is-editing" : ""}`}>
      <div className="header-main">
        <div className="header-tools">
          <input
            className="ft-input"
            type="search"
            placeholder="Filter keywords, values, meanings"
            value={query}
            onChange={(e) => app.set({ headerQuery: e.target.value })}
            aria-label="Filter header"
          />
          <Segmented
            label="Header view"
            value={view}
            onChange={setView}
            options={[
              { value: "grouped", label: "Grouped" },
              { value: "raw", label: "Raw" },
            ]}
          />
          {canEdit && (
            <Button variant={editing ? "secondary" : "ghost"} onClick={() => setEditing(!editing)} title={editing ? "Leave edit mode (staged changes are discarded)" : "Edit keywords"}>
              {editing ? "Done" : "Edit"}
            </Button>
          )}
        </div>
        {doc.hdus.length > 1 && (
          <div className="hdu-switch" role="group" aria-label="HDU">
            {doc.hdus.map((x) => (
              <button key={x.index} type="button" className="ft-chip" aria-pressed={x.index === h.index} onClick={() => setHdu(x.index)}>
                HDU {x.index} · {x.kind.replace("_", " ")}
              </button>
            ))}
          </div>
        )}
        {editing && <AddRow />}
        {editing && editError && (
          <p className="edit-banner" role="alert">
            {editError}
          </p>
        )}
        {table}
        <p className="header-foot">
          {count} of {h.cards.length} records · HDU {h.index} · {h.header_blocks} header block{h.header_blocks === 1 ? "" : "s"}
          {editing && h.index !== imageHdu && " · edits apply to the image HDU"}
        </p>
      </div>
      {editing && <StagedPanel />}
    </div>
  );
}
