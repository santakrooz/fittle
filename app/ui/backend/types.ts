// The UI reaches the Rust core only through `Backend`. Tauri implements it;
// `?demo` implements it from fixtures; a WASM build could later
// (docs/decisions/0003). Types mirror the Rust JSON (fittle.info/1 etc.).

export type Value =
  | { type: "logical"; value: boolean }
  | { type: "integer"; value: number }
  | { type: "float"; value: number }
  | { type: "string"; value: string }
  | { type: "complex"; value: [number, number] }
  | { type: "undefined" }
  | { type: "unparsed"; value: string }
  | { type: "commentary"; value: string };

export type Card = Value & {
  keyword: string;
  comment?: string;
  hierarch?: boolean;
  record: number;
  raw: string[];
};

export type Issue = {
  severity: "info" | "warning" | "error";
  code: string;
  message: string;
  hdu?: number;
  record?: number;
};

export type HeaderHdu = {
  index: number;
  kind: string;
  header_blocks: number;
  data_bytes: number;
  bitpix?: number;
  shape: number[];
  cards: Card[];
};

/** `fittle header --json` (fittle.header/1). */
export type HeaderDoc = {
  schema: "fittle.header/1";
  path: string;
  file_bytes: number;
  hdus: HeaderHdu[];
  issues: Issue[];
};

// ---- fittle.info/1 --------------------------------------------------------

export type Source =
  | { kind: "keyword"; key: string }
  | { kind: "model_default"; model: string }
  | { kind: "filename" }
  | { kind: "derived"; from: string[] };

export type Fact<T> = { value: T; source: Source };

export type Fields = Partial<{
  object: Fact<string>;
  ra: Fact<number>;
  dec: Fact<number>;
  telescope: Fact<string>;
  focal_mm: Fact<number>;
  aperture_mm: Fact<number>;
  focal_ratio: Fact<number>;
  camera: Fact<string>;
  sensor: Fact<string>;
  pixel_um: Fact<number>;
  binning: Fact<number>;
  gain: Fact<number>;
  offset: Fact<number>;
  egain: Fact<number>;
  sensor_temp_c: Fact<number>;
  set_temp_c: Fact<number>;
  bayer: Fact<string>;
  row_order: Fact<string>;
  filter: Fact<string>;
  frame_type: Fact<string>;
  exposure_s: Fact<number>;
  stack_count: Fact<number>;
  total_integration_s: Fact<number>;
  date_obs: Fact<string>;
  date_local: Fact<string>;
  site_lat: Fact<number>;
  site_lon: Fact<number>;
  site_elev_m: Fact<number>;
  mount: Fact<string>;
  pier_side: Fact<string>;
  focus_pos: Fact<number>;
  focus_temp_c: Fact<number>;
}> & { serials?: { key: string; value: string }[]; ignored?: { key: string; reason: string }[] };

export type Derived = Partial<{
  pixel_scale: Fact<number>;
  wcs_scale: Fact<number>;
  fov_arcmin: Fact<[number, number]>;
  sampling: Fact<"over" | "well" | "under">;
  altitude: Fact<number>;
  azimuth: Fact<number>;
  airmass: Fact<number>;
  moon: Fact<{ illumination: number; waxing: boolean; altitude?: number; separation?: number }>;
  sun_altitude: Fact<number>;
  sky: Fact<string>;
  session_night: Fact<string>;
}> & { plate_solved: boolean };

export type FrameKind = "light" | "dark" | "flat" | "bias" | "dark_flat" | "unknown";

export type Verdict = {
  label: string;
  frame: FrameKind;
  integrated: boolean;
  confidence: number;
  evidence: { text: string; supports: string; weight: number }[];
  processing?: { name: string; evidence: string }[];
  processed: boolean;
  linear?: boolean;
};

export type Matched = { id: string; name: string; evidence: string[]; verified: boolean };

export type Info = {
  schema: "fittle.info/1";
  path: string;
  verdict: Verdict;
  origin: { scope?: Matched; software: (Matched & { role: "capture" | "processing" | "observatory" })[] };
  target?: {
    id: string;
    common_name?: string;
    aliases: string[];
    kind: string;
    ra: number;
    dec: number;
    size_arcmin?: number;
    constellation?: string;
  };
  fields: Fields;
  derived: Derived;
  image?: { hdu: number; width: number; height: number; planes: number; bitpix?: number; compressed: boolean };
  file_name: Record<string, unknown>;
  notes: { level: "info" | "warning"; message: string }[];
  health: Issue[];
};

// ---- viewing --------------------------------------------------------------

export type Mode = "raw" | "debayer";

export type Stf = { shadows: number; midtones: number; highlights: number };

export type ChannelStats = {
  min: number;
  max: number;
  mean: number;
  median: number;
  mad: number;
  clipped_low: number;
  saturated: number;
  /** 4096 bins over [0, 1]. */
  histogram: number[];
};

export type Display = {
  mode: Mode;
  width: number;
  height: number;
  channels: number;
  source_step: number;
  stats: ChannelStats[];
  stf: Stf[];
  stf_linked: Stf;
  /** Linear from the auto black point to the 99.99th percentile (viewer "Linear"). */
  stf_linear?: Stf[];
};

export type OpenedImage = {
  hdu: number;
  width: number;
  height: number;
  planes: number;
  normalized_from: [number, number];
  can_debayer: boolean;
  default_mode: Mode;
};

export type Opened = { info: Info; image?: OpenedImage; image_error?: string; wcs: boolean };

/** Pixels as half floats, channel-interleaved, row 0 first (stored order). */
export type Pixels = { width: number; height: number; channels: number; data: Uint16Array };

export type Readout = { x: number; y: number; raw: number[]; norm: number[]; ra?: number; dec?: number };

export type Entry = {
  path: string;
  name: string;
  bytes: number;
  label?: string;
  frame?: FrameKind;
  integrated: boolean;
  exposure_s?: number;
  stack_count?: number;
  filter?: string;
  object?: string;
  date_obs?: string;
  night?: string;
  gain?: number;
  error?: string;
};

export type FrameStats = {
  stars: number;
  hfr?: number;
  fwhm?: number;
  eccentricity?: number;
  background: number;
  noise: number;
  trail?: { angle_deg: number; length_px: number };
  scale: number;
};

export type Reason = "clouds" | "low_altitude" | "dawn" | "trailing" | "soft" | "satellite" | "unreadable";

export type SubGrade = {
  path: string;
  name: string;
  date_obs?: string;
  night?: string;
  exposure_s?: number;
  stats?: FrameStats;
  sun_altitude?: number;
  altitude?: number;
  reject: boolean;
  reasons: Reason[];
  badness: number;
  group: number;
};

export type Thresholds = {
  object: string;
  filter: string;
  exposure_s?: number;
  subs: number;
  median_hfr?: number;
  median_stars: number;
  median_background: number;
  hfr_max?: number;
  stars_min: number;
  background_max: number;
  eccentricity_max?: number;
};

export type Grading = {
  subs: SubGrade[];
  groups: Thresholds[];
  kept: number;
  rejected: number;
  flagged_trails: number;
  captured_s: number;
  usable_s: number;
  median_hfr?: number;
  elapsed_ms: number;
};

export type Status = "ok" | "warn" | "bad";

export type Report = {
  schema: "fittle.report/1";
  summary: {
    folder: string;
    files: number;
    frames: Record<string, number>;
    stacks: number;
    nights: string[];
    targets: { object: string; filter: string; subs: number; integration_s: number; nights: string[] }[];
    total_light_s: number;
    warnings: string[];
  };
  nights: { night: string; subs: number; captured_s: number; usable_s?: number; rejects?: number; status: Status }[];
  checks: { status: Status; text: string }[];
  grading?: Grading;
};

export type Move = { from: string; to: string; error?: string };

/** A pre-stretched blink frame; `stf` is 9 numbers (s, m, h × 3 channels). */
export type BlinkFrame = { width: number; height: number; bottomUp: boolean; stf: number[]; rgba: Uint8ClampedArray<ArrayBuffer> };

export type OrganizeMove = { from: string; to: string; companion?: boolean; note?: string; error?: string };
export type OrganizePlan = {
  schema: "fittle.organize/1";
  root: string;
  spec: { by: string; rename?: string | null };
  moves: OrganizeMove[];
  unchanged: number;
  new_folders: string[];
  missing_tokens: string[];
  manifest?: string;
};

export type MatchStatus = "ok" | "warn" | "bad" | "none";
export type CalSet = { kind: "dark" | "flat" | "bias" | "dark_flat"; name: string; master: boolean; frames: number; gain?: number; exposure_s?: number; temp_c?: number; filter?: string };
export type KindMatch = { kind: CalSet["kind"]; status: MatchStatus; set?: CalSet; notes: string[] };
export type LightGroup = {
  camera?: string;
  gain?: number;
  exposure_s?: number;
  filter?: string;
  temp_c?: number;
  subs: number;
  integration_s: number;
  nights: string[];
  darks: KindMatch;
  flats: KindMatch;
  bias: KindMatch;
  status: "ready" | "partial" | "missing";
  why?: string;
};
export type Matching = {
  schema: "fittle.calmatch/1";
  lights: string;
  library: string;
  library_frames: number;
  sets: number;
  groups: LightGroup[];
  ready: number;
  partial: number;
  missing: number;
};

export type KeywordInfo = {
  keyword: string;
  label: string;
  unit?: string;
  group: string;
  help: string;
  structural: boolean;
};

export type Thumb = { width: number; height: number; rgba: Uint8ClampedArray };

// ---- editing (fittle.edit/1) ---------------------------------------------

export type NewValue =
  | { type: "logical"; value: boolean }
  | { type: "integer"; value: number }
  | { type: "float"; value: number }
  | { type: "string"; value: string }
  | { type: "auto"; value: string };

export type Op =
  | { op: "set"; key: string; value: NewValue; comment: string | null }
  | { op: "unset"; key: string }
  | { op: "rename"; from: string; to: string }
  | { op: "history"; text: string };

export type EditOptions = { backup: boolean; history: boolean; checksum: boolean; hdu: number | null };

export type DiffRow = {
  keyword: string;
  a?: string;
  b?: string;
  status: "same" | "changed" | "only_a" | "only_b";
  impact?: { level: "ok" | "info" | "warning" | "blocker"; text: string };
  group?: string;
};
export type HeaderDiff = {
  schema: "fittle.diff/1";
  a: { path: string; hdu: number; verdict: string };
  b: { path: string; hdu: number; verdict: string };
  rows: DiffRow[];
  differences: number;
  blockers: number;
};

export type McpSnippet = { id: string; client: string; how: string; file?: string; format: "shell" | "json" | "toml"; text: string };
export type McpSetup = { bin: string | null; snippets: McpSnippet[]; version: string };
export type McpTest = { ok: boolean; server?: string; tools: string[]; elapsed_ms: number; error?: string };

export type Spread = "same" | "mixed" | "range" | "unique";
export type KeyDist = {
  keyword: string;
  present: number;
  spread: Spread;
  values: { text: string; count: number; paths?: string[] }[];
  distinct: number;
  min?: number;
  max?: number;
};
export type Distribution = { schema: "fittle.batch/1"; files: number; unreadable: string[]; keys: KeyDist[] };
export type BatchPlan = {
  files: number;
  changed: number;
  in_place: number;
  rewrite: number;
  backup_bytes: number;
  est_seconds: number;
  errors: [string, string][];
  notes: string[];
  sample: Plan[];
  cli: string;
};
export type Rig = { name: string; values: Record<string, number | string | boolean>; builtin?: boolean };

export type ChangeKind = "added" | "modified" | "removed" | "renamed" | "history";
export type Change = { kind: ChangeKind; key: string; before?: string; after?: string };

export type Plan = {
  path: string;
  hdu: number;
  changes: Change[];
  header_blocks_before: number;
  header_blocks_after: number;
  in_place: boolean;
  consequences: string[];
  warnings: string[];
};

export type WriteReport = Plan & {
  method: "none" | "in_place" | "rewrite";
  backup?: string;
  untouched_sha256: string;
  checksum?: string;
};

export type FileResult = { path: string; report?: WriteReport; error?: string };

export type ExportFormat =
  | { kind: "png"; bits: 8 | 16 }
  | { kind: "jpeg"; quality: number }
  | { kind: "webp" }
  | { kind: "avif"; quality: number }
  | { kind: "tiff"; bits: 8 | 16 | 32 }
  | { kind: "fits" };

export type ExportStretch =
  | { kind: "none" }
  | { kind: "auto"; linked: boolean }
  | { kind: "asinh" }
  | { kind: "custom"; stf: Stf[]; asinh: number };

/** Mirrors fittle-image `ExportSpec` (serde snake_case). */
export type ExportSpec = {
  format: ExportFormat;
  stretch: ExportStretch;
  debayer: boolean;
  crop: { x: number; y: number; width: number; height: number } | null;
  rotate: 0 | 90 | 180 | 270;
  flip_horizontal: boolean;
  flip_vertical: boolean;
  bin: number | null;
  bin_mode: "average" | "sum";
  long_edge: number | null;
  metadata: boolean;
  private: boolean;
  /** Share-card caption strip. */
  card: boolean;
};

export type ExportPlan = {
  file_name: string;
  missing: string[];
  width: number;
  height: number;
  channels: number;
  estimate_bytes: number;
  problem?: string;
};

export type Exported = {
  path: string;
  width: number;
  height: number;
  channels: number;
  format: ExportFormat;
  bytes: number;
  steps: string[];
  wcs: boolean;
};

/** 8-bit interleaved pixels (1 or 3 channels). */
export type Bytes = { width: number; height: number; channels: number; data: Uint8Array };

export type PackReport = {
  source: string;
  path: string;
  bytes_in: number;
  bytes_out: number;
  verified: boolean;
};

export interface Backend {
  /** File or folder the app was launched with. */
  initialPath(): Promise<{ path: string; dir: boolean } | null>;
  pickFile(): Promise<string | null>;
  pickFolder(): Promise<string | null>;
  listFolder(path: string): Promise<Entry[]>;
  thumbnail(path: string): Promise<Thumb | null>;
  openFile(path: string): Promise<Opened>;
  display(mode: Mode): Promise<Display>;
  preview(mode: Mode, maxEdge: number): Promise<Pixels>;
  /** Region of the display image in display pixels, box-averaged by `step`. */
  region(mode: Mode, x: number, y: number, w: number, h: number, step: number): Promise<Pixels>;
  /** Values at a source pixel (stored order). */
  readout(x: number, y: number): Promise<Readout | null>;
  header(path: string): Promise<HeaderDoc>;
  dictionary(): Promise<KeywordInfo[]>;
  /** Dry run: what `ops` would do. Rejects with a message on validation errors. */
  planEdit(path: string, ops: Op[], options: EditOptions): Promise<Plan>;
  /** Write `ops` to every path (validated first; nothing written on error). */
  applyEdits(paths: string[], ops: Op[], options: EditOptions): Promise<FileResult[]>;
  /** Privacy-scrub edits for a file, to stage. */
  scrubOps(path: string): Promise<Op[]>;
  /** Output name, size and estimate for exporting the open file. */
  exportPlan(spec: ExportSpec, template: string): Promise<ExportPlan>;
  /** What the export will look like, long edge ≤ maxEdge. */
  exportPreview(spec: ExportSpec, maxEdge: number): Promise<Bytes>;
  /** Export the open file into `dir` (null: next to the source). Never overwrites. */
  exportImage(spec: ExportSpec, template: string, dir: string | null): Promise<Exported>;
  /** Session report for a folder; `grade` measures every light sub (seconds). */
  sessionReport(path: string, recursive: boolean, grade: boolean, rules?: string): Promise<Report>;
  /** Save the last report into its folder: md, html, json or astrobin. Returns the new file. */
  saveReport(format: "md" | "html" | "json" | "astrobin"): Promise<string>;
  /** Move files into _rejected/ beside them; `dryRun` only plans. */
  moveRejects(paths: string[], dryRun: boolean): Promise<Move[]>;
  /** A blink frame; pass `stf` to lock the stretch (first frame: omit). */
  blinkFrame(path: string, maxEdge: number, stf?: number[]): Promise<BlinkFrame>;
  /** Star metrics for one sub. */
  subStats(path: string): Promise<FrameStats>;
  /** Setup text for AI tools; `bin` overrides the found fittle binary. */
  mcpSetup(bin: string | null, roots: string[]): Promise<McpSetup>;
  /** Start the MCP server once and list its tools. */
  mcpTest(bin: string, roots: string[]): Promise<McpTest>;
  /** Header diff of two files, with calibration impact. */
  diffFiles(a: string, b: string): Promise<HeaderDiff>;
  keywordSpread(paths: string[]): Promise<Distribution>;
  batchPlan(paths: string[], ops: Op[], options: EditOptions): Promise<BatchPlan>;
  /** Validated first; returns [path, error] for files that failed. */
  batchApply(paths: string[], ops: Op[], options: EditOptions): Promise<[string, string][]>;
  rigsList(): Promise<Rig[]>;
  rigSave(name: string, from: string, keys: string[]): Promise<Rig>;
  rigDelete(name: string): Promise<boolean>;
  /** Plan organizing a folder: folder template and/or rename template. */
  organizePlan(folder: string, by: string, rename?: string): Promise<OrganizePlan>;
  /** Apply a plan (renames only, never replaces) or undo a manifest. */
  organizeApply(plan: OrganizePlan | null, undo?: string): Promise<OrganizePlan>;
  /** Match the lights in a folder to a calibration library. */
  matchCalibration(lights: string, library: string): Promise<Matching>;
  /** fpack (unpack=false) or funpack a file into a new file beside it. */
  packFile(path: string, unpack: boolean): Promise<PackReport>;
  /** Development timing line (no-op unless tracing). */
  log?(msg: string): void;
}
