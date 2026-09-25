import { useEffect, useRef } from "react";
import type { Preview, Stf } from "../backend/types";
import { LINEAR, type StretchMode } from "./stf";

// WebGL2 float-texture viewer. The file's values are uploaded once as
// R32F / RGB32F; the stretch runs in the fragment shader, so changing it is
// a uniform update, never a re-upload and never a React re-render.

const VERT = `#version 300 es
in vec2 pos;
out vec2 uv;
uniform vec2 scale;
void main() {
  uv = vec2(pos.x * 0.5 + 0.5, 0.5 - pos.y * 0.5);
  gl_Position = vec4(pos * scale, 0.0, 1.0);
}`;

const FRAG = `#version 300 es
precision highp float;
in vec2 uv;
out vec4 color;
uniform sampler2D img;
uniform bool mono;
uniform vec3 shadows, midtones, highlights;
vec3 mtf(vec3 m, vec3 x) {
  return ((m - 1.0) * x) / ((2.0 * m - 1.0) * x - m);
}
void main() {
  vec3 v = texture(img, uv).rgb;
  if (mono) v = v.rrr;
  vec3 x = clamp((v - shadows) / (highlights - shadows), 0.0, 1.0);
  color = vec4(clamp(mtf(midtones, x), 0.0, 1.0), 1.0);
}`;

function compile(gl: WebGL2RenderingContext, type: number, src: string) {
  const s = gl.createShader(type)!;
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? "shader");
  return s;
}

type Props = { preview: Preview; mode: StretchMode };

export function Stage({ preview, mode }: Props) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const draw = useRef<(stf: Stf[]) => void>(() => {});

  // Set up GL and upload the texture once per preview.
  useEffect(() => {
    const el = canvas.current!;
    const gl = el.getContext("webgl2", { antialias: false });
    if (!gl) throw new Error("WebGL2 unavailable");

    const prog = gl.createProgram()!;
    gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(prog);
    gl.useProgram(prog);

    const quad = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(prog, "pos");
    gl.enableVertexAttribArray(loc);
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);

    const { width, height, channels } = preview.info;
    const tex = gl.createTexture();
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    const [internal, format] = channels === 3 ? [gl.RGB32F, gl.RGB] : [gl.R32F, gl.RED];
    gl.texImage2D(gl.TEXTURE_2D, 0, internal, width, height, 0, format, gl.FLOAT, preview.pixels);
    // Float textures are not filterable without an extension; the preview is
    // already downsampled, so nearest is fine for M0.
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    const u = (n: string) => gl.getUniformLocation(prog, n);
    gl.uniform1i(u("mono"), channels === 1 ? 1 : 0);

    let last: Stf[] = [];
    const render = (stf: Stf[]) => {
      last = stf;
      const dpr = window.devicePixelRatio || 1;
      const w = Math.round(el.clientWidth * dpr);
      const h = Math.round(el.clientHeight * dpr);
      if (el.width !== w || el.height !== h) {
        el.width = w;
        el.height = h;
      }
      gl.viewport(0, 0, w, h);
      // Fit: preserve aspect ratio inside the canvas.
      const imgAspect = width / height;
      const boxAspect = w / h;
      gl.uniform2f(u("scale"), imgAspect > boxAspect ? 1 : imgAspect / boxAspect, imgAspect > boxAspect ? boxAspect / imgAspect : 1);
      const ch = (i: number) => stf[Math.min(i, stf.length - 1)];
      gl.uniform3f(u("shadows"), ch(0).shadows, ch(1).shadows, ch(2).shadows);
      gl.uniform3f(u("midtones"), ch(0).midtones, ch(1).midtones, ch(2).midtones);
      gl.uniform3f(u("highlights"), ch(0).highlights, ch(1).highlights, ch(2).highlights);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    };
    draw.current = render;

    const ro = new ResizeObserver(() => render(last));
    ro.observe(el);
    return () => {
      ro.disconnect();
      gl.deleteTexture(tex);
      gl.deleteBuffer(quad);
      gl.deleteProgram(prog);
    };
  }, [preview]);

  // Stretch changes are uniform updates only.
  useEffect(() => {
    draw.current(mode === "auto" ? preview.info.stf : [LINEAR]);
  }, [preview, mode]);

  return <canvas ref={canvas} className="stage-canvas" aria-label="Stretched image preview" />;
}
