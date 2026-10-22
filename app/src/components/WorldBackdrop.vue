<script setup lang="ts">
import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import {
  createHexFlow,
  advanceHexFlow,
  restingIntensity,
  restingWidth,
  type HexFlow,
} from "../hexFlow";

import { createBackdropGraph } from "../flowGraph";
import { createFlowPainter } from "../flowPainter";
import { backdropForTheme } from "../backdropThemes";

const props = defineProps<{ paused: boolean; theme: string }>();
// App keys this component by theme: changing geometry disposes its old RAF and observer.
const design = backdropForTheme(props.theme);
const graph = createBackdropGraph(design.kind);
const backdrop = ref<HTMLDivElement>();
const mesh = ref<SVGSVGElement>();
const columns = ref(2);
const rows = ref(2);
const tiles = computed(() =>
  Array.from({ length: columns.value * rows.value }, (_, index) => ({
    x: (index % columns.value) * graph.width,
    y: Math.floor(index / columns.value) * graph.height,
  })),
);
let flow: HexFlow | undefined;
let paths: SVGPathElement[][] = [];
let cellPaths: SVGPathElement[][] = [];
let repaint: ReturnType<typeof createFlowPainter> | undefined;
let frame = 0;
let last = 0;
let motionPreference: MediaQueryList | undefined;
let resizeObserver: ResizeObserver | undefined;

function stop() {
  cancelAnimationFrame(frame);
  frame = 0;
  last = 0;
}
function paint() {
  if (flow) repaint?.(flow, motionPreference?.matches);
}
function collectPaths() {
  paths = Array.from({ length: graph.edges.length }, () => []);
  for (const path of mesh.value?.querySelectorAll<SVGPathElement>(
    ".flow-edge",
  ) || [])
    paths[Number(path.dataset.edge)]!.push(path);
  cellPaths = Array.from({ length: graph.cells.length }, () => []);
  for (const path of mesh.value?.querySelectorAll<SVGPathElement>(
    ".flow-cell",
  ) || [])
    cellPaths[Number(path.dataset.cell)]!.push(path);
  repaint = createFlowPainter(paths, cellPaths);
  paint();
}
function tick(now: number) {
  frame = requestAnimationFrame(tick);
  if (!last) {
    last = now;
    return;
  }
  const elapsed = (now - last) / 1000;
  // Ease on every display frame; only the Markov state uses the 300 ms clock.
  if (elapsed <= 0 || !flow) return;
  last = now;
  advanceHexFlow(flow, elapsed);
  paint();
}
function sync() {
  if (!flow) return;
  if (props.paused || document.hidden || motionPreference?.matches) {
    stop();
    if (motionPreference?.matches) paint();
  } else if (!frame) {
    paint();
    frame = requestAnimationFrame(tick);
  }
}
onMounted(() => {
  flow = createHexFlow(graph);
  motionPreference = matchMedia(
    "(prefers-reduced-motion: reduce), (forced-colors: active)",
  );
  collectPaths();
  resizeObserver = new ResizeObserver(([entry]) => {
    if (!entry) return;
    columns.value = Math.ceil(entry.contentRect.width / graph.width) + 1;
    rows.value = Math.ceil(entry.contentRect.height / graph.height) + 1;
  });
  if (backdrop.value) resizeObserver.observe(backdrop.value);
  motionPreference.addEventListener("change", sync);
  document.addEventListener("visibilitychange", sync);
  sync();
});
watch(() => props.paused, sync);
watch([columns, rows], collectPaths, { flush: "post" });
onBeforeUnmount(() => {
  stop();
  resizeObserver?.disconnect();
  motionPreference?.removeEventListener("change", sync);
  document.removeEventListener("visibilitychange", sync);
});
</script>

<template>
  <div
    ref="backdrop"
    class="world-backdrop"
    :data-pattern="design.kind"
    :data-paused="paused"
    :style="{
      '--tile-width': `${graph.width}px`,
      '--tile-height': `${graph.height}px`,
      '--drift-duration': `${graph.width / 10}s`,
    }"
    aria-hidden="true"
  >
    <svg
      ref="mesh"
      class="world-mesh"
      xmlns="http://www.w3.org/2000/svg"
      focusable="false"
    >
      <!-- Real visible paths: repaint the edges directly, not a cached pattern. -->
      <svg
        v-for="tile in tiles"
        :key="`${tile.x}:${tile.y}`"
        :x="tile.x"
        :y="tile.y"
        :width="graph.width"
        :height="graph.height"
        overflow="hidden"
      >
        <path
          v-for="(wash, index) in graph.washes"
          :key="`wash-${index}`"
          :d="wash"
          class="flow-wash"
        />
        <path
          v-for="(cell, index) in graph.cells"
          :key="`cell-${index}`"
          :d="cell.path"
          :data-cell="index"
          class="flow-cell"
          fill-opacity="0"
        />
        <path
          v-for="(edge, index) in graph.edges"
          :key="index"
          :d="edge.path"
          :data-edge="index"
          class="flow-edge"
          :stroke-width="restingWidth()"
          :stroke-opacity="restingIntensity()"
        />
      </svg>
    </svg>
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
  top: 0;
  left: 0;
  width: calc(100% + var(--tile-width));
  height: calc(100% + var(--tile-height));
  opacity: 0.42;
  animation: world-drift var(--drift-duration) linear infinite;
}
.flow-edge {
  fill: none;
  stroke-linejoin: miter;
  stroke-linecap: butt;
  stroke: var(--blue);
}
.flow-wash {
  fill: var(--aqua);
  fill-opacity: 0.025;
}
.flow-cell {
  fill: var(--cyan);
  stroke: none;
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
.world-backdrop[data-paused="true"] .world-mesh {
  animation-play-state: paused;
}
@keyframes world-drift {
  to {
    transform: translate(
      calc(-1 * var(--tile-width)),
      calc(-1 * var(--tile-height))
    );
  }
}
@media (prefers-reduced-motion: reduce) {
  .world-mesh {
    animation: none;
  }
}
@media (forced-colors: active) {
  .world-backdrop {
    display: none;
  }
}
</style>
