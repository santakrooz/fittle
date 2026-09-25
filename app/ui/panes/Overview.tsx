import type { Fact, Info, Source } from "../backend/types";
import { Badge, Card, EvidenceChip, Field, Fields, Icon, type Tone } from "../ds";
import { decDms, duration, group, minus, num, raHms } from "../format";
import { app } from "../state/app";

const sourceText = (s: Source) =>
  s.kind === "keyword" ? s.key : s.kind === "model_default" ? `${s.model} default` : s.kind === "filename" ? "file name" : `derived from ${s.from.join(", ")}`;
const isDefault = (f?: Fact<unknown>) => f?.source.kind === "model_default";
const isDerived = (f?: Fact<unknown>) => f?.source.kind === "derived";

function kindTone(kind: string): Tone {
  if (/supernova/i.test(kind)) return "orange";
  if (/emission|hii|nebula/i.test(kind) && !/planetary|reflection|dark/i.test(kind)) return "rose";
  if (/planetary|reflection/i.test(kind)) return "teal";
  return "accent";
}

function Verdict({ info }: { info: Info }) {
  const v = info.verdict;
  const f = info.fields;
  const icon = v.integrated ? "stack" : v.frame === "light" || v.frame === "unknown" ? "sub" : "cal";
  const who = info.origin.scope?.name ?? info.origin.software.find((s) => s.role === "capture")?.name ?? f.camera?.value;
  const detail = v.integrated
    ? [info.origin.software.find((s) => s.role === "processing")?.name, f.stack_count && `${group(f.stack_count.value)} frames`]
    : [f.exposure_s && `Single ${num(f.exposure_s.value, 3)} s exposure`, who];
  const evidence = v.evidence
    .filter((e) => (e.supports === "integrated" ? v.integrated : e.supports === "single" ? !v.integrated : true))
    .slice(0, 6);
  return (
    <section className="ft-card" aria-label="Verdict">
      <div className="ft-verdict">
        <div className={`ft-vicon ${v.integrated ? "stack" : ""}`}>
          <Icon name={icon} />
        </div>
        <div>
          <div className="ft-vt">{v.label}</div>
          <div className="ft-vs">{detail.filter(Boolean).join(" · ")}</div>
        </div>
        <span className="ft-conf" title="Confidence, 0–100, from the evidence below">
          {v.confidence}
        </span>
      </div>
      <div className="ft-evid">
        {evidence.map((e) => (
          <EvidenceChip key={e.text} text={e.text} />
        ))}
      </div>
    </section>
  );
}

function Integration({ info }: { info: Info }) {
  const f = info.fields;
  const v = info.verdict;
  if (!v.integrated || !f.total_integration_s) return null;
  return (
    <Card title="Integration" badge={<span className="big-number">{duration(f.total_integration_s.value)}</span>}>
      <div className="integration-bar" aria-hidden="true" />
      <Fields>
        <Field
          label="Sub length"
          value={f.exposure_s ? `${num(f.exposure_s.value, 3)} s` : "—"}
          unit={f.stack_count ? `× ${group(f.stack_count.value)}` : undefined}
          derived={isDerived(f.total_integration_s)}
          title={sourceText(f.total_integration_s.source)}
        />
        <Field label="Filter" value={f.filter?.value ?? "—"} />
      </Fields>
    </Card>
  );
}

function Processing({ info }: { info: Info }) {
  const steps = info.verdict.processing ?? [];
  if (!steps.length) return null;
  const linear = info.verdict.linear;
  return (
    <Card title="Processing state" badge={<Badge tone={linear === false ? "orange" : "teal"}>{linear === false ? "Stretched" : "Linear"}</Badge>}>
      <ul className="steps">
        {steps.map((s) => (
          <li key={s.name} title={s.evidence}>
            <span className="check" aria-hidden="true">
              ✓
            </span>
            <span className="step-name">{s.name[0].toUpperCase() + s.name.slice(1)}</span>
            <span className="step-why">{s.evidence}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

function Target({ info }: { info: Info }) {
  const f = info.fields;
  const t = info.target;
  if (!f.object && !t && !f.ra) return null;
  return (
    <Card title="Target" badge={t && <Badge tone={kindTone(t.kind)}>{t.kind}</Badge>}>
      <Fields>
        <Field label="Object" value={f.object?.value ?? t?.id ?? "—"} unit={t?.common_name} />
        <Field label="Constellation" value={t?.constellation ?? "—"} derived={!!t?.constellation} title="From the catalogue entry for this object" />
        {f.ra && <Field label="RA" value={raHms(f.ra.value)} title={sourceText(f.ra.source)} />}
        {f.dec && <Field label="Dec" value={decDms(f.dec.value)} title={sourceText(f.dec.source)} />}
      </Fields>
    </Card>
  );
}

function Equipment({ info }: { info: Info }) {
  const f = info.fields;
  const d = info.derived;
  const made = [info.origin.scope?.name, ...info.origin.software.filter((s) => s.id !== "seestar-app" || !info.origin.scope).map((s) => s.name)]
    .filter(Boolean)
    .join(" → ");
  const optics = [f.focal_mm && `${num(f.focal_mm.value, 1)} mm`].filter(Boolean).join("");
  const opticsUnit = [f.focal_ratio && `ƒ/${num(f.focal_ratio.value, 1)}`, f.aperture_mm && `${num(f.aperture_mm.value, 1)} mm`]
    .filter(Boolean)
    .join(" · ");
  const scale = d.wcs_scale ?? d.pixel_scale;
  return (
    <Card title="Equipment" badge={<span className="ft-label">{f.telescope?.value ?? ""}</span>}>
      <Fields>
        {optics && <Field label="Optics" value={optics} unit={opticsUnit} fallback={isDefault(f.focal_mm)}
            title={[f.focal_mm && `Focal length: ${sourceText(f.focal_mm.source)}`, f.aperture_mm && `Aperture: ${sourceText(f.aperture_mm.source)}`].filter(Boolean).join("\n")} />}
        {f.camera && <Field label="Camera" value={f.camera.value} title={sourceText(f.camera.source)} />}
        {(f.sensor || f.pixel_um) && (
          <Field
            label="Sensor"
            value={f.sensor?.value ?? "—"}
            unit={f.pixel_um ? `${num(f.pixel_um.value, 2)} µm` : undefined}
            fallback={isDefault(f.sensor)}
            title={f.sensor ? sourceText(f.sensor.source) : undefined}
          />
        )}
        {(f.gain || f.sensor_temp_c) && (
          <Field
            label="Gain / temp"
            value={f.gain ? num(f.gain.value, 2) : "—"}
            unit={f.sensor_temp_c ? `· ${minus(num(f.sensor_temp_c.value, 1))} °C` : undefined}
          />
        )}
        {f.filter && <Field label="Filter" value={f.filter.value} />}
        {scale && <Field label="Scale" value={`${num(scale.value, 2)}″/px`} derived unit={d.plate_solved ? "solved" : undefined} />}
        {d.fov_arcmin && <Field label="Field" value={`${Math.round(d.fov_arcmin.value[0])}′ × ${Math.round(d.fov_arcmin.value[1])}′`} derived />}
        {f.mount && <Field label="Mount" value={f.mount.value} unit={f.pier_side ? `pier ${f.pier_side.value.toLowerCase()}` : undefined} wide />}
        {made && <Field label="Made by" value={made} wide />}
      </Fields>
    </Card>
  );
}

function Conditions({ info }: { info: Info }) {
  const f = info.fields;
  const d = info.derived;
  if (!f.date_obs && !f.site_lat) return null;
  const moon = d.moon?.value;
  return (
    <Card title="Conditions">
      <Fields>
        {f.date_obs && <Field label="Time (UTC)" value={f.date_obs.value.split(".")[0].replace("T", " ")} wide />}
        {d.session_night && <Field label="Night of" value={d.session_night.value} derived />}
        {f.site_lat && f.site_lon && (
          <Field label="Site" value={`${minus(f.site_lat.value.toFixed(2))}, ${minus(f.site_lon.value.toFixed(2))}`} unit={f.site_elev_m ? `${group(f.site_elev_m.value)} m` : undefined} />
        )}
        {d.altitude && <Field label="Altitude" value={`${Math.round(d.altitude.value)}°`} unit={d.airmass ? `airmass ${num(d.airmass.value, 2)}` : undefined} derived />}
        {moon && (
          <Field
            label="Moon"
            value={`${Math.round(moon.illumination * 100)}%`}
            unit={[moon.separation != null && `${Math.round(moon.separation)}° away`, moon.altitude != null && moon.altitude < 0 && "set"].filter(Boolean).join(" · ")}
            derived
          />
        )}
        {d.sky && <Field label="Sky" value={d.sky.value} derived />}
      </Fields>
    </Card>
  );
}

function Notes({ info }: { info: Info }) {
  const items = [
    ...info.health.map((h) => ({ level: h.severity === "error" ? "bad" : "warn", text: `${h.code}: ${h.message}` })),
    ...info.notes.map((n) => ({ level: n.level === "warning" ? "warn" : "info", text: n.message })),
    ...(info.fields.serials?.length ? [{ level: "info", text: `Serial number in ${info.fields.serials.map((s) => s.key).join(", ")}: scrub before sharing` }] : []),
  ];
  if (!items.length) return null;
  return (
    <Card title="Notes">
      <ul className="notes">
        {items.map((n, i) => (
          <li key={i} className={`note ${n.level}`}>
            {n.text}
          </li>
        ))}
      </ul>
    </Card>
  );
}

export function Overview() {
  const info = app.use((s) => s.opened?.info);
  if (!info) return <p className="inspector-empty">Open a file to see what it is.</p>;
  return (
    <div className="inspector-body">
      <Verdict info={info} />
      <Integration info={info} />
      <Processing info={info} />
      <Target info={info} />
      <Equipment info={info} />
      <Conditions info={info} />
      <Notes info={info} />
    </div>
  );
}
