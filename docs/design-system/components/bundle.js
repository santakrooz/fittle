/* @ds-bundle: {"format":4,"namespace":"Fittle","components":[{"name":"Button"},{"name":"Chip"},{"name":"Segmented"},{"name":"Tabs"},{"name":"Toolbar"},{"name":"Card"},{"name":"Badge"},{"name":"Field"},{"name":"StatTile"},{"name":"VerdictCard"},{"name":"Kbd"},{"name":"FileRow"}]} */
(function () {
  var React = window.React;
  var h = React.createElement;
  function cx() { return Array.prototype.filter.call(arguments, Boolean).join(" "); }

  var ICONS = {
    sub: "M3 7h18v10H3zM8 7l1.5-2h5L16 7M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
    stack: "M12 3l9 5-9 5-9-5 9-5zM3 13l9 5 9-5",
    zoomIn: "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM21 21l-5-5M11 8v6M8 11h6",
    zoomOut: "M11 4a7 7 0 1 1 0 14 7 7 0 0 1 0-14zM21 21l-5-5M8 11h6",
    star: "M12 3l2.6 5.6 6 .7-4.5 4.1 1.3 6-5.4-3.1-5.4 3.1 1.3-6L3.4 9.3l6-.7z",
    export: "M12 4v11M7 10l5 5 5-5M5 20h14"
  };
  function Icon(props) {
    return h("svg", { viewBox: "0 0 24 24", "aria-hidden": "true" }, h("path", { d: ICONS[props.name] || "" }));
  }

  function Button(p) {
    return h("button", { type: "button", className: cx("ft-btn", p.variant || "secondary"), disabled: p.disabled, onClick: p.onClick },
      p.icon ? h(Icon, { name: p.icon }) : null, p.children);
  }

  function Chip(p) {
    return h("button", { type: "button", className: "ft-chip", "aria-pressed": !!p.on, onClick: p.onClick }, p.children);
  }

  function Segmented(p) {
    var st = React.useState(p.value != null ? p.value : p.options[0]);
    var value = p.value != null ? p.value : st[0];
    return h("div", { className: "ft-seg", role: "radiogroup", "aria-label": p.label },
      p.options.map(function (o) {
        return h("button", { key: o, type: "button", role: "radio", "aria-checked": o === value,
          onClick: function () { st[1](o); if (p.onChange) p.onChange(o); } }, o);
      }));
  }

  function Tabs(p) {
    var st = React.useState(p.value != null ? p.value : p.tabs[0]);
    var value = p.value != null ? p.value : st[0];
    return h("div", { className: "ft-tabs", role: "tablist" },
      p.tabs.map(function (t) {
        return h("button", { key: t, type: "button", role: "tab", className: "ft-tab", "aria-selected": t === value,
          onClick: function () { st[1](t); if (p.onChange) p.onChange(t); } }, t);
      }));
  }

  function ToolButton(p) {
    return h("button", { type: "button", className: "ft-tool", "aria-pressed": !!p.on, "aria-label": p.label, title: p.label, onClick: p.onClick },
      p.icon ? h(Icon, { name: p.icon }) : null, p.children);
  }
  function Toolbar(p) {
    return h("div", { className: "ft-toolbar", role: "toolbar", "aria-label": p.label }, p.children);
  }
  Toolbar.Button = ToolButton;
  Toolbar.Separator = function () { return h("span", { className: "ft-sep", "aria-hidden": "true" }); };

  function Card(p) {
    return h("section", { className: "ft-card", "aria-label": p.title },
      p.title || p.badge ? h("div", { className: "ft-card-head" },
        h("span", { className: "ft-card-title" }, p.title), p.badge || null) : null,
      p.children);
  }

  function Badge(p) {
    return h("span", { className: cx("ft-badge", p.tone || "accent") }, p.children);
  }

  function Field(p) {
    return h("div", null,
      h("div", { className: "ft-label" }, p.label, p.derived ? h("span", { className: "ft-derived" }, "DERIVED") : null),
      h("div", { className: "ft-value" }, p.value, p.unit ? h("small", null, " " + p.unit) : null));
  }
  function Fields(p) { return h("div", { className: "ft-fields" }, p.children); }

  function StatTile(p) {
    return h("div", { className: "ft-tile" },
      h("div", { className: "ft-tile-n" }, p.value, p.unit ? h("small", null, " " + p.unit) : null),
      h("span", { className: "ft-label" }, p.label));
  }

  function VerdictCard(p) {
    return h("section", { className: "ft-card", "aria-label": "Verdict" },
      h("div", { className: "ft-verdict" },
        h("div", { className: cx("ft-vicon", p.kind === "stack" && "stack") }, h(Icon, { name: p.kind === "stack" ? "stack" : "sub" })),
        h("div", null, h("div", { className: "ft-vt" }, p.label), p.detail ? h("div", { className: "ft-vs" }, p.detail) : null),
        h("span", { className: "ft-conf", title: "Confidence 0–100" }, String(p.confidence))),
      p.evidence && p.evidence.length ? h("div", { className: "ft-evid" },
        p.evidence.map(function (e) {
          var i = e.indexOf("=");
          return h("span", { key: e, className: "ft-ev" },
            i > 0 ? [e.slice(0, i + 1), h("b", { key: "v" }, e.slice(i + 1))] : e);
        })) : null);
  }

  function Kbd(p) { return h("kbd", { className: "ft-kbd" }, p.children); }

  function FileRow(p) {
    return h("button", { type: "button", className: cx("ft-file", p.status === "rejected" && "rejected"), "aria-selected": !!p.selected, onClick: p.onClick },
      h("span", { className: "ft-thumb", "aria-hidden": "true" }),
      h("span", { style: { minWidth: 0 } },
        h("span", { className: "ft-fn", style: { display: "block" } }, p.name),
        h("span", { className: "ft-fs" },
          h("i", { className: cx("ft-dot", p.status === "warn" && "warn", p.status === "rejected" && "bad") }), p.detail)));
  }

  window.Fittle = { Button: Button, Chip: Chip, Segmented: Segmented, Tabs: Tabs, Toolbar: Toolbar, Card: Card, Badge: Badge,
    Field: Field, Fields: Fields, StatTile: StatTile, VerdictCard: VerdictCard, Kbd: Kbd, FileRow: FileRow, Icon: Icon };
})();
