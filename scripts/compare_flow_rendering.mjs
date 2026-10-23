// Optional native raster regression; no browser, network, or launcher data.
// Dependencies are installed separately from app dependencies; see docs/performance/dev0.4.md.
import { writeFileSync, mkdirSync } from "node:fs";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { resolve, join } from "node:path";
const outputRoot = resolve(process.argv[2] || ".runtime/flow-reference");
const require = createRequire(
  join(resolve(process.argv[3] || ".runtime/render-tools"), "package.json"),
);
const canvasLib = require("@napi-rs/canvas");
const resvgLib = require("@resvg/resvg-js");
const createGL = require("gl");
import { createBackdropGraph } from "../app/src/flowGraph.ts";
import { createHexFlow, advanceHexFlow } from "../app/src/hexFlow.ts";
import { createFlowWebGL as createFlowCanvas } from "../app/src/flowWebGL.ts";
import { parseFlowColor } from "../app/src/flowPainter.ts";
import { createFlowPainter } from "../app/tests/helpers/svgFlowPainter.ts";
const { createCanvas, Path2D, DOMMatrix, loadImage } = canvasLib;
const { Resvg } = resvgLib;
globalThis.Path2D = Path2D;
globalThis.DOMMatrix = DOMMatrix;
mkdirSync(outputRoot, { recursive: true });
const designs = [
  ["honeycomb", "#007ea7", "#80ced7", "#9ad1d4", "#07151c"],
  ["chunks", "#4d926f", "#a5d6ad", "#d0ddad", "#0c1814"],
  ["diamonds", "#b56d91", "#e6abc4", "#e9c5b6", "#1b101b"],
  ["facets", "#8473be", "#beb2ef", "#a5c8f3", "#121322"],
  ["circuit", "#607e9c", "#b7c8db", "#cad8df", "#111820"],
];
const results = [];
for (const dpr of [1, 1.25, 2])
  for (const [kind, cyan, blue, aqua, night] of designs) {
    let seed = 12345;
    const rng = () =>
      (seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) / 4294967296;
    const graph = createBackdropGraph(kind),
      flow = createHexFlow(graph, rng);
    const palette = {
      cyan: parseFlowColor(cyan),
      blue: parseFlowColor(blue),
      aqua: parseFlowColor(aqua),
    };
    const targets = (n) =>
      Array.from({ length: n }, () => [
        {
          attrs: {},
          style: { fill: "" },
          setAttribute(k, v) {
            this.attrs[k] = v;
          },
        },
      ]);
    const edges = targets(graph.edges.length),
      cells = targets(graph.cells.length),
      oldPaint = createFlowPainter(edges, cells);
    const output = createCanvas(900 * dpr, 600 * dpr),
      gl = createGL(1, 1, {
        alpha: true,
        antialias: false,
        depth: false,
        stencil: false,
        preserveDrawingBuffer: true,
      });
    assert.ok(gl, "Native GL context");
    let w = 1,
      h = 1;
    const surface = {
      getContext: () => gl,
      get width() {
        return w;
      },
      set width(n) {
        w = n;
        gl.getExtension("STACKGL_resize_drawingbuffer").resize(
          Math.max(1, w),
          Math.max(1, h),
        );
      },
      get height() {
        return h;
      },
      set height(n) {
        h = n;
        gl.getExtension("STACKGL_resize_drawingbuffer").resize(
          Math.max(1, w),
          Math.max(1, h),
        );
      },
    };
    const renderer = createFlowCanvas(surface, graph, palette);
    const width = 900,
      height = 600;
    renderer.resize(width, height, dpr);

    for (let i = 0; i < 360; i++) {
      advanceHexFlow(flow, 1 / 60);
    }

    for (const seconds of [
      0,
      graph.width / 20,
      graph.width / 10 - 0.001,
      graph.width / 10 + 0.001,
    ]) {
      oldPaint(flow);
      renderer.paint(flow, seconds);
      gl.finish();
      assert.equal(gl.getError(), 0);
      const pixels = new Uint8Array(w * h * 4);
      gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
      for (let i = 0; i < pixels.length; i += 4) {
        const a = pixels[i + 3];
        if (a)
          for (let c = 0; c < 3; c++)
            pixels[i + c] = Math.round((pixels[i + c] * 255) / a);
      }
      const flipped = new Uint8ClampedArray(pixels.length);
      for (let y = 0; y < h; y++)
        flipped.set(
          pixels.subarray(y * w * 4, (y + 1) * w * 4),
          (h - y - 1) * w * 4,
        );
      output
        .getContext("2d")
        .putImageData(new canvasLib.ImageData(flipped, w, h), 0, 0);

      const phase = ((seconds * 10) / graph.width) % 1;
      const path = (d, attrs) => `<path d="${d}" ${attrs}/>`;
      const washes = graph.washes
        .map((d) => path(d, `fill="${aqua}" fill-opacity=".025"`))
        .join("");
      const fills = graph.cells
        .map((c, i) => {
          const cell = cells[i][0];
          let rgb = cyan;
          if (cell.style.fill) {
            const [x, y] = [...cell.style.fill.matchAll(/([\d.]+)%/g)].map(
              (m) => Number(m[1]) / 100,
            );
            rgb = `rgb(${palette.cyan
              .map(
                (v, j) =>
                  v * x +
                  (palette.blue[j] * y + palette.aqua[j] * (1 - y)) * (1 - x),
              )
              .map(Math.round)
              .join(",")})`;
          }
          return path(
            c.path,
            `fill="${rgb}" fill-opacity="${cell.attrs["fill-opacity"]}"`,
          );
        })
        .join("");
      const lines = graph.edges
        .map((e, i) =>
          path(
            e.path,
            `fill="none" stroke="${blue}" stroke-opacity="${edges[i][0].attrs["stroke-opacity"]}" stroke-width="${edges[i][0].attrs["stroke-width"]}" stroke-linecap="butt" stroke-linejoin="miter"`,
          ),
        )
        .join("");
      let tiles = "";
      for (let x = 0; x < Math.ceil(width / graph.width) + 1; x++)
        for (let y = 0; y < Math.ceil(height / graph.height) + 1; y++)
          tiles += `<g transform="translate(${x * graph.width - phase * graph.width} ${y * graph.height - phase * graph.height})" clip-path="url(#tile)">${washes}${fills}${lines}</g>`;
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"><defs><clipPath id="tile"><rect width="${graph.width}" height="${graph.height}"/></clipPath></defs><g>${tiles}</g></svg>`;
      const oldImage = await loadImage(
        new Resvg(svg, { fitTo: { mode: "zoom", value: dpr } })
          .render()
          .asPng(),
      );
      const oldCanvas = createCanvas(width * dpr, height * dpr),
        newCanvas = createCanvas(width * dpr, height * dpr);
      const oldCtx = oldCanvas.getContext("2d");
      oldCtx.fillStyle = night;
      oldCtx.fillRect(0, 0, width * dpr, height * dpr);
      oldCtx.globalAlpha = 0.42;
      oldCtx.drawImage(oldImage, 0, 0);
      const ctx = newCanvas.getContext("2d");
      ctx.fillStyle = night;
      ctx.fillRect(0, 0, width * dpr, height * dpr);
      ctx.globalAlpha = 0.42;
      ctx.drawImage(output, 0, 0);
      const a = oldCanvas
          .getContext("2d")
          .getImageData(0, 0, width * dpr, height * dpr).data,
        b = ctx.getImageData(0, 0, width * dpr, height * dpr).data;
      let total = 0,
        max = 0,
        large = 0;
      for (let i = 0; i < a.length; i += 4) {
        let local = 0;
        for (let c = 0; c < 3; c++) {
          const d = Math.abs(a[i + c] - b[i + c]);
          total += d;
          max = Math.max(max, d);
          local = Math.max(local, d);
        }
        if (local > 20) large++;
      }
      results.push({
        kind,
        seconds,
        mean_channel_error: total / ((a.length / 4) * 3),
        max_channel_error: max,
        percent_pixels_over20: (large / (a.length / 4)) * 100,
        dpr,
      });
      if (
        seconds === graph.width / 20 ||
        seconds === graph.width / 10 - 0.001
      ) {
        writeFileSync(
          `${outputRoot}/${kind}-dpr${dpr}-${seconds === graph.width / 20 ? "middle" : "seam"}-svg.png`,
          oldCanvas.toBuffer("image/png"),
        );
        writeFileSync(
          `${outputRoot}/${kind}-dpr${dpr}-${seconds === graph.width / 20 ? "middle" : "seam"}-canvas.png`,
          newCanvas.toBuffer("image/png"),
        );
      }
    }
    renderer.dispose();
  }
writeFileSync(
  join(outputRoot, "results.json"),
  JSON.stringify(results, null, 2),
);
assert.ok(
  results.every(
    (r) => r.mean_channel_error < 1 && r.percent_pixels_over20 < 0.02,
  ),
  "Visual regression; inspect output images and results.json",
);
console.log(JSON.stringify(results, null, 2));
