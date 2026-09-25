// WebGL2 image renderer. Pixels arrive once as half-float textures (a
// whole-image preview, plus an optional full-resolution detail patch for the
// visible area); stretch, clipping and channel isolation are uniforms, so
// changing them never re-uploads or re-renders React.
import type { Pixels } from "../backend/types";
import type { ShaderStretch } from "../state/stretch";

const VERT = `#version 300 es
in vec2 corner;
out vec2 uv;
uniform vec4 rect;      // stored display coords x0, y0, x1, y1
uniform vec3 view;      // scale (device px per display px), centre x, centre y (visual)
uniform vec2 canvas;    // device px
uniform float imgH;     // display height, for flipping
uniform bool flip;      // stored rows run bottom-up
void main() {
  uv = corner;
  vec2 p = mix(rect.xy, rect.zw, corner);
  if (flip) p.y = imgH - p.y;
  vec2 s = (p - view.yz) * view.x + canvas * 0.5;
  gl_Position = vec4(s.x / canvas.x * 2.0 - 1.0, 1.0 - s.y / canvas.y * 2.0, 0.0, 1.0);
}`;

const FRAG = `#version 300 es
precision highp float;
in vec2 uv;
out vec4 color;
uniform sampler2D img;
uniform bool mono;
uniform vec3 shadows, midtones, highlights;
uniform float asinhK;
uniform bool clipping;
uniform int channel;    // 0 = all, 1 = R, 2 = G, 3 = B
vec3 mtf(vec3 m, vec3 x) { return ((m - 1.0) * x) / ((2.0 * m - 1.0) * x - m); }
void main() {
  vec3 v = texture(img, uv).rgb;
  if (mono) v = v.rrr;
  if (channel > 0) v = vec3(v[channel - 1]);
  vec3 x = clamp((v - shadows) / (highlights - shadows), 0.0, 1.0);
  vec3 s = asinhK > 0.0 ? asinh(asinhK * x) / asinh(asinhK) : mtf(midtones, x);
  s = clamp(s, 0.0, 1.0);
  if (clipping) {
    if (any(greaterThanEqual(v, vec3(0.9999)))) s = vec3(0.878, 0.337, 0.478);      // nebula-rose: saturated
    else if (all(lessThanEqual(v, shadows))) s = vec3(0.25, 0.45, 0.95);          // blue: crushed to black
  }
  color = vec4(s, 1.0);
}`;

export type ViewState = { scale: number; cx: number; cy: number };
type Layer = { tex: WebGLTexture; rect: [number, number, number, number]; channels: number };

export class Renderer {
  private gl: WebGL2RenderingContext;
  private prog: WebGLProgram;
  private u: Record<string, WebGLUniformLocation | null> = {};
  private base: Layer | null = null;
  private detail: Layer | null = null;
  private imgH = 0;
  private flip = false;
  view: ViewState = { scale: 1, cx: 0, cy: 0 };
  stretch: ShaderStretch | null = null;
  clipping = false;
  channel = 0;
  private frame = 0;

  constructor(private canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", { antialias: false, premultipliedAlpha: false });
    if (!gl) throw new Error("WebGL2 is not available");
    this.gl = gl;
    const sh = (type: number, src: string) => {
      const s = gl.createShader(type)!;
      gl.shaderSource(s, src);
      gl.compileShader(s);
      if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? "shader");
      return s;
    };
    const p = gl.createProgram()!;
    gl.attachShader(p, sh(gl.VERTEX_SHADER, VERT));
    gl.attachShader(p, sh(gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(p);
    if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p) ?? "link");
    this.prog = p;
    gl.useProgram(p);
    for (const n of ["rect", "view", "canvas", "imgH", "flip", "img", "mono", "shadows", "midtones", "highlights", "asinhK", "clipping", "channel"]) {
      this.u[n] = gl.getUniformLocation(p, n);
    }
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(p, "corner");
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
  }

  private upload(px: Pixels): WebGLTexture {
    const gl = this.gl;
    const tex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 2);
    const [internal, format] = px.channels === 3 ? [gl.RGB16F, gl.RGB] : [gl.R16F, gl.RED];
    gl.texImage2D(gl.TEXTURE_2D, 0, internal, px.width, px.height, 0, format, gl.HALF_FLOAT, px.data);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    return tex;
  }

  /** New image: the preview covers the whole display image (width × height). */
  setImage(preview: Pixels, width: number, height: number, flip: boolean) {
    this.dispose();
    this.imgH = height;
    this.flip = flip;
    this.base = { tex: this.upload(preview), rect: [0, 0, width, height], channels: preview.channels };
    this.request();
  }

  /** Full-resolution patch covering display rect [x, y, x+w, y+h). */
  setDetail(px: Pixels | null, x = 0, y = 0, w = 0, h = 0) {
    if (this.detail) this.gl.deleteTexture(this.detail.tex);
    this.detail = px ? { tex: this.upload(px), rect: [x, y, x + w, y + h], channels: px.channels } : null;
    this.request();
  }

  hasImage() {
    return this.base !== null;
  }

  /** Schedule one draw on the next animation frame. */
  request() {
    if (!this.frame) this.frame = requestAnimationFrame(() => ((this.frame = 0), this.draw()));
  }

  private draw() {
    const gl = this.gl;
    const { canvas } = this;
    gl.viewport(0, 0, canvas.width, canvas.height);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    if (!this.base || !this.stretch) return;
    const u = this.u;
    gl.useProgram(this.prog);
    gl.uniform3f(u.view, this.view.scale, this.view.cx, this.view.cy);
    gl.uniform2f(u.canvas, canvas.width, canvas.height);
    gl.uniform1f(u.imgH, this.imgH);
    gl.uniform1i(u.flip, this.flip ? 1 : 0);
    gl.uniform3fv(u.shadows, this.stretch.shadows);
    gl.uniform3fv(u.midtones, this.stretch.midtones);
    gl.uniform3fv(u.highlights, this.stretch.highlights);
    gl.uniform1f(u.asinhK, this.stretch.asinh);
    gl.uniform1i(u.clipping, this.clipping ? 1 : 0);
    gl.uniform1i(u.channel, this.channel);
    gl.uniform1i(u.img, 0);
    for (const layer of [this.base, this.detail]) {
      if (!layer) continue;
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, layer.tex);
      gl.uniform1i(u.mono, layer.channels === 1 ? 1 : 0);
      gl.uniform4f(u.rect, ...layer.rect);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    }
  }

  dispose() {
    if (this.base) this.gl.deleteTexture(this.base.tex);
    if (this.detail) this.gl.deleteTexture(this.detail.tex);
    this.base = null;
    this.detail = null;
  }
}
