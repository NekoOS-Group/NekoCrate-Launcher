<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { createHexFlow, advanceHexFlow, type HexFlow } from "../hexFlow";
import { createBackdropGraph } from "../flowGraph";
import { createFlowRenderer } from "../flowRenderer";
import { parseFlowColor, type FlowPalette } from "../flowPainter";
import { backdropForTheme } from "../backdropThemes";

const props = defineProps<{ paused: boolean; theme: string }>();
// App keys this component by theme; each geometry owns one complete simulation.
const design = backdropForTheme(props.theme);
const graph = createBackdropGraph(design.kind);
const backdrop = ref<HTMLDivElement>();
const mesh = ref<HTMLDivElement>();
let flow: HexFlow | undefined;
let painter: ReturnType<typeof createFlowRenderer> | undefined;
let palette: FlowPalette;
let contextLost = false;
let frame = 0;
let last: number | undefined;
let driftSeconds = 0;
let width = 0,
  height = 0;
let motionPreference: MediaQueryList | undefined;
let densityPreference: MediaQueryList | undefined;
let resizeObserver: ResizeObserver | undefined;

function stop() {
  cancelAnimationFrame(frame);
  frame = 0;
  last = undefined;
}
function paint() {
  if (flow) painter?.paint(flow, driftSeconds, motionPreference?.matches);
}
function resize() {
  if (painter?.resize(width, height, window.devicePixelRatio || 1)) paint();
}
function watchDensity() {
  densityPreference?.removeEventListener("change", watchDensity);
  densityPreference = matchMedia(
    `(resolution: ${window.devicePixelRatio || 1}dppx)`,
  );
  densityPreference.addEventListener("change", watchDensity);
  resize();
}
function tick(now: number) {
  frame = requestAnimationFrame(tick);
  if (last === undefined) {
    last = now;
    return;
  }
  const elapsed = (now - last) / 1000;
  last = now;
  if (elapsed <= 0 || !flow) return;
  driftSeconds = (driftSeconds + elapsed) % (graph.width / 10);
  // Preserve display-rate easing and the independent 300 ms Markov clock.
  advanceHexFlow(flow, elapsed);
  paint();
}
function sync() {
  if (!flow) return;
  if (
    contextLost ||
    props.paused ||
    document.hidden ||
    motionPreference?.matches
  ) {
    stop();
    if (motionPreference?.matches) {
      // Like removing/reapplying the old CSS animation, start drift at its origin.
      driftSeconds = 0;
      paint();
    }
  } else if (!frame) {
    paint();
    frame = requestAnimationFrame(tick);
  }
}
function loseContext(event: Event) {
  if (event.type === "webglcontextlost") event.preventDefault();
  contextLost = true;
  stop();
}
function detachPainter() {
  if (!painter) return;
  for (const event of ["contextlost", "webglcontextlost"])
    painter.canvas.removeEventListener(event, loseContext);
  for (const event of ["contextrestored", "webglcontextrestored"])
    painter.canvas.removeEventListener(event, restoreContext);
  painter.dispose();
}
function mountPainter(fallback = false) {
  detachPainter();
  painter = createFlowRenderer(graph, palette, fallback);
  mesh.value!.replaceChildren(painter.canvas);
  for (const event of ["contextlost", "webglcontextlost"])
    painter.canvas.addEventListener(event, loseContext);
  for (const event of ["contextrestored", "webglcontextrestored"])
    painter.canvas.addEventListener(event, restoreContext);
  painter.resize(width, height, window.devicePixelRatio || 1);
}
function restoreContext() {
  try {
    painter?.restore();
  } catch {
    mountPainter(true);
  }
  contextLost = false;
  paint();
  sync();
}
onMounted(() => {
  if (!backdrop.value || !mesh.value) return;
  const theme = getComputedStyle(backdrop.value);
  palette = {
    cyan: parseFlowColor(theme.getPropertyValue("--cyan")),
    blue: parseFlowColor(theme.getPropertyValue("--blue")),
    aqua: parseFlowColor(theme.getPropertyValue("--aqua")),
  };
  mountPainter();
  flow = createHexFlow(graph);
  motionPreference = matchMedia(
    "(prefers-reduced-motion: reduce), (forced-colors: active)",
  );
  resizeObserver = new ResizeObserver(([entry]) => {
    if (!entry) return;
    width = entry.contentRect.width;
    height = entry.contentRect.height;
    resize();
  });
  resizeObserver.observe(backdrop.value);
  watchDensity();
  motionPreference.addEventListener("change", sync);
  document.addEventListener("visibilitychange", sync);
  sync();
});
watch(() => props.paused, sync);
onBeforeUnmount(() => {
  stop();
  resizeObserver?.disconnect();
  motionPreference?.removeEventListener("change", sync);
  densityPreference?.removeEventListener("change", watchDensity);
  document.removeEventListener("visibilitychange", sync);
  detachPainter();
});
</script>

<template>
  <div
    ref="backdrop"
    class="world-backdrop"
    :data-pattern="design.kind"
    aria-hidden="true"
  >
    <div ref="mesh" class="world-mesh" />
    <div class="field-vignette" />
    <div class="field-scan" />
  </div>
</template>

<style scoped>
.world-backdrop {
  position: absolute;
  inset: 0;
  z-index: -1;
  overflow: hidden;
  pointer-events: none;
  background: var(--night);
}
.world-mesh {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  opacity: 0.42;
}
.world-mesh :deep(canvas) {
  display: block;
  width: 100%;
  height: 100%;
}
.field-vignette {
  position: absolute;
  inset: 0;
  box-shadow: inset 0 0 160px 40px var(--night);
}
.field-scan {
  position: absolute;
  inset: 0;
  opacity: 0.5;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='4' height='4'%3E%3Cpath d='M0 3.5H4' stroke='white' stroke-opacity='.025'/%3E%3C/svg%3E");
}
@media (forced-colors: active) {
  .world-backdrop {
    display: none;
  }
}
</style>
