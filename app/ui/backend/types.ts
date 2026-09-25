// The UI reaches the Rust core only through this interface. Tauri implements
// it today; a WASM build could implement it later (docs/decisions/0003).

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

export type Hdu = {
  index: number;
  kind: string;
  header_offset: number;
  header_blocks: number;
  data_offset: number;
  data_bytes: number;
  bitpix?: number;
  shape: number[];
  cards: Card[];
};

export type Issue = {
  severity: "info" | "warning" | "error";
  code: string;
  message: string;
  hdu?: number;
  record?: number;
};

/** `fittle header --json` document (schema fittle.header/1). */
export type HeaderDoc = {
  schema: "fittle.header/1";
  path: string;
  file_bytes: number;
  hdus: Hdu[];
  issues: Issue[];
};

export type Stf = { shadows: number; midtones: number; highlights: number };

export type PreviewInfo = {
  hdu: number;
  source_width: number;
  source_height: number;
  width: number;
  height: number;
  channels: 1 | 3;
  normalized_from: [number, number];
  stf: Stf[];
};

export type Preview = { info: PreviewInfo; pixels: Float32Array };

export interface Backend {
  header(path: string): Promise<HeaderDoc>;
  /** Display preview of the first image HDU, long edge capped at maxEdge. */
  preview(path: string, maxEdge: number): Promise<Preview>;
  pickFile(): Promise<string | null>;
  /** File the app was launched with, if any. */
  initialPath(): Promise<string | null>;
}
