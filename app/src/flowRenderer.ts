import type { FlowGraph } from "./flowGraph.ts";
import type { FlowPalette } from "./flowPainter.ts";
import { createFlowCanvas } from "./flowCanvas.ts";
import { createFlowWebGL } from "./flowWebGL.ts";

export function createFlowRenderer(
  graph: FlowGraph,
  palette: FlowPalette,
  fallback = false,
) {
  let canvas = document.createElement("canvas");
  if (!fallback) {
    try {
      return {
        ...createFlowWebGL(canvas, graph, palette),
        canvas,
        kind: "webgl" as const,
      };
    } catch (error) {
      // A canvas cannot switch context type once a WebGL context was created.
      // Release the failed context before creating a fresh 2D fallback surface.
      canvas
        .getContext("webgl")
        ?.getExtension("WEBGL_lose_context")
        ?.loseContext();
      console.warn("Background renderer: falling back to Canvas 2D", error);
      canvas = document.createElement("canvas");
    }
  }
  return {
    ...createFlowCanvas(canvas, graph, palette),
    canvas,
    kind: "2d" as const,
    restore() {},
  };
}
