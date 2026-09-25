import { useMemo, useState } from "react";
import type { Card, KeywordInfo } from "../backend/types";
import { Icon, Segmented } from "../ds";
import { app } from "../state/app";

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

export function HeaderTab() {
  const doc = app.use((s) => s.header);
  const dict = app.use((s) => s.dictionary);
  const query = app.use((s) => s.headerQuery);
  const ignored = app.use((s) => s.opened?.info.fields.ignored);
  const [view, setView] = useState<"grouped" | "raw">("grouped");
  const [hdu, setHdu] = useState(0);
  const h = doc?.hdus[Math.min(hdu, (doc?.hdus.length ?? 1) - 1)];

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
  return (
    <div className="header-tab">
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
          <table className="htable">
            <tbody>
              {groups.map(({ g, cards }) => [
                <tr key={g} className="grp">
                  <td colSpan={3}>{g === "history" ? "History & comments" : (GROUP_LABEL[g] ?? g[0].toUpperCase() + g.slice(1))}</td>
                </tr>,
                ...cards.map((c) => {
                  const info = lookup(dict, c.keyword);
                  const skip = h.index === (app.get().opened?.info.image?.hdu ?? 0) ? ignored?.find((x) => x.key === c.keyword) : undefined;
                  return (
                    <tr key={`${g}-${c.record}`} title={skip ? skip.reason : info?.help}>
                      <td className="key">
                        {c.keyword || "(blank)"}
                        {info?.structural && (
                          <span className="lock" title="Structural: describes the data layout and cannot be edited">
                            <Icon name="lock" />
                          </span>
                        )}
                      </td>
                      <td className="val">
                        {valueText(c)}
                        {skip ? (
                          <span className="ft-badge warn ignored">ignored</span>
                        ) : (
                          info?.unit && c.type !== "string" && c.type !== "commentary" && <span className="unit"> {info.unit}</span>
                        )}
                      </td>
                      <td className="c">{info?.label ?? c.comment ?? ""}</td>
                    </tr>
                  );
                }),
              ])}
            </tbody>
          </table>
        )}
      </div>
      <p className="header-foot">
        {count} of {h.cards.length} records · HDU {h.index} · {h.header_blocks} header block{h.header_blocks === 1 ? "" : "s"}
      </p>
    </div>
  );
}
