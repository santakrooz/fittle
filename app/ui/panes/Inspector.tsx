import { Tabs } from "../ds";
import { app, type Tab } from "../state/app";
import { HeaderTab } from "./HeaderTab";
import { HistogramTab } from "./HistogramTab";
import { Overview } from "./Overview";

const TABS: { value: Tab; label: string }[] = [
  { value: "overview", label: "Overview" },
  { value: "header", label: "Header" },
  { value: "histogram", label: "Histogram" },
];

export function Inspector() {
  const tab = app.use((s) => s.tab);
  return (
    <aside className="inspector" aria-label="Inspector">
      <Tabs tabs={TABS} value={tab} onChange={(t) => app.set({ tab: t })} />
      {tab === "overview" && <Overview />}
      {tab === "header" && <HeaderTab />}
      {tab === "histogram" && <HistogramTab />}
    </aside>
  );
}
