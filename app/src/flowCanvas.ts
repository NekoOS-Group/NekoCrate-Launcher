import type { FlowGraph } from "./flowGraph.ts";
import type { HexFlow } from "./hexFlow.ts";
import {
  createFlowPaintState,
  flowCellColor,
  type FlowPalette,
} from "./flowPainter.ts";

type Bounds = readonly [number, number, number, number];
type PaintPath = { shape: Path2D; bounds: Bounds[] };

function compilePath(source: string): PaintPath {
  // FlowGraph emits only absolute M/L/Z polygons. Keep a separate bound for
  // each seam copy: their combined box could span the entire periodic tile.
  const bounds = source
    .split("M")
    .filter(Boolean)
    .map((part): Bounds => {
      const coordinates = part.match(/-?\d+(?:\.\d+)?/g)!.map(Number);
      let left = Infinity,
        top = Infinity,
        right = -Infinity,
        bottom = -Infinity;
      for (let i = 0; i < coordinates.length; i += 2) {
        left = Math.min(left, coordinates[i]!);
        right = Math.max(right, coordinates[i]!);
        top = Math.min(top, coordinates[i + 1]!);
        bottom = Math.max(bottom, coordinates[i + 1]!);
      }
      return [left, top, right, bottom];
    });
  return { shape: new Path2D(source), bounds };
}

function intersects(
  path: PaintPath,
  left: number,
  top: number,
  right: number,
  bottom: number,
  margin: number,
) {
  for (const box of path.bounds) {
    if (
      box[0] - margin < right &&
      box[2] + margin > left &&
      box[1] - margin < bottom &&
      box[3] + margin > top
    )
      return true;
  }
  return false;
}

/** Simulate the complete periodic graph, but rasterize only visible paths.
 * Drawing directly into the viewport avoids a full-tile buffer and bitmap
 * copies every frame. Culling never changes simulation or paint-state updates.
 */
export function createFlowCanvas(
  canvas: HTMLCanvasElement,
  graph: FlowGraph,
  palette: FlowPalette,
) {
  const output = canvas.getContext("2d");
  if (!output) throw new Error("Canvas 2D is unavailable");
  const edges = graph.edges.map((edge) => compilePath(edge.path));
  const cells = graph.cells.map((cell) => compilePath(cell.path));
  const washes = graph.washes.map(compilePath);
  const state = createFlowPaintState(edges.length, cells.length);
  const colors = cells.map(() => `rgb(${palette.cyan.join(" ")})`);
  const cyanMix = new Int16Array(cells.length).fill(-1);
  const blueMix = new Int16Array(cells.length).fill(-1);
  const edgeColor = `rgb(${palette.blue.join(" ")})`;
  const washColor = `rgb(${palette.aqua.join(" ")})`;
  let width = 0,
    height = 0,
    ratio = 0;

  return {
    resize(nextWidth: number, nextHeight: number, nextRatio: number) {
      if (width === nextWidth && height === nextHeight && ratio === nextRatio)
        return false;
      width = Math.max(1, nextWidth);
      height = Math.max(1, nextHeight);
      ratio = Math.max(1, nextRatio);
      canvas.width = Math.ceil(width * ratio);
      canvas.height = Math.ceil(height * ratio);
      return true;
    },
    paint(flow: HexFlow, driftSeconds: number, still = false) {
      if (!width || !height) return;
      state.update(flow, still);
      for (let i = 0; i < cells.length; i++) {
        if (
          cyanMix[i] !== state.cyanMix[i] ||
          blueMix[i] !== state.blueMix[i]
        ) {
          colors[i] = flowCellColor(
            palette,
            state.cyanMix[i]!,
            state.blueMix[i]!,
          );
          cyanMix[i] = state.cyanMix[i]!;
          blueMix[i] = state.blueMix[i]!;
        }
      }
      const phase = still ? 0 : ((driftSeconds * 10) / graph.width) % 1;
      const scaleX = canvas.width / width,
        scaleY = canvas.height / height;
      const fringe = 1 / Math.min(scaleX, scaleY);
      output.setTransform(scaleX, 0, 0, scaleY, 0, 0);
      output.clearRect(0, 0, width, height);
      output.lineCap = "butt";
      output.lineJoin = "miter";
      output.strokeStyle = edgeColor;
      for (let y = -phase * graph.height; y < height; y += graph.height) {
        for (let x = -phase * graph.width; x < width; x += graph.width) {
          const left = Math.max(0, -x),
            top = Math.max(0, -y);
          const right = Math.min(graph.width, width - x),
            bottom = Math.min(graph.height, height - y);
          output.save();
          output.setTransform(scaleX, 0, 0, scaleY, x * scaleX, y * scaleY);
          // Preserve the old SVG viewport clip, including partial paths at
          // periodic seams. Keep fractional periods and device resolution.
          output.beginPath();
          output.rect(0, 0, graph.width, graph.height);
          output.clip();
          output.fillStyle = washColor;
          output.globalAlpha = 0.025;
          for (const wash of washes) {
            if (intersects(wash, left, top, right, bottom, fringe))
              output.fill(wash.shape);
          }
          for (let i = 0; i < cells.length; i++) {
            if (
              !state.opacity[i] ||
              !intersects(cells[i]!, left, top, right, bottom, fringe)
            )
              continue;
            output.fillStyle = colors[i]!;
            output.globalAlpha = state.opacity[i]!;
            output.fill(cells[i]!.shape);
          }
          for (let i = 0; i < edges.length; i++) {
            if (
              !intersects(
                edges[i]!,
                left,
                top,
                right,
                bottom,
                state.width[i]! / 2 + fringe,
              )
            )
              continue;
            output.globalAlpha = state.intensity[i]!;
            output.lineWidth = state.width[i]!;
            output.stroke(edges[i]!.shape);
          }
          output.restore();
        }
      }
    },
    dispose() {
      canvas.width = canvas.height = 0;
    },
  };
}
