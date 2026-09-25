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
  error?: string;
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

export interface Backend {
  initialPath(): Promise<string | null>;
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
  /** Development timing line (no-op unless tracing). */
  log?(msg: string): void;
}
