<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import bitmap from "../assets/fonts/minecraft-bitmap.json";
import sample from "../assets/fonts/unifont-sample.json";
import {
  paintSplash,
  splashPulse,
  splashRotation,
  splashScale,
  unicodeGlyph,
  type PixelGlyph,
} from "../splashText";
import { t } from "../i18n";
import type { WidgetRpc } from "../widgetClient";
const props = defineProps<{
  text: string;
  source: string;
  angle?: number;
  scale?: number;
  rpc: WidgetRpc;
}>();
const canvas = ref<HTMLCanvasElement>();
const credit = ref<HTMLCanvasElement>();
const lines = ref<HTMLElement>();
const stage = ref<HTMLElement>();
const error = ref(false);
const glyphs: Record<string, PixelGlyph> = {
  ...(bitmap as unknown as Record<string, PixelGlyph>),
};
const cacheKey = "nekocrate.splash-glyphs.v1";
let cached: Record<string, string> = { ...sample };
try {
  const data = JSON.parse(localStorage.getItem(cacheKey) || "{}");
  for (const [c, bits] of Object.entries(data).slice(-2000))
    if (
      typeof bits === "string" &&
      Array.from(c).length === 1 &&
      unicodeGlyph(c, bits)
    )
      cached[c] = bits;
} catch {
  /* Keep the bundled example glyphs. */
}
for (const [c, bits] of Object.entries(cached)) {
  const glyph = unicodeGlyph(c, bits);
  if (glyph) glyphs[c] = glyph;
}
let generation = 0,
  disposed = false,
  animation: Animation | undefined,
  observer: ResizeObserver | undefined;
const reduced = window.matchMedia("(prefers-reduced-motion: reduce)");
const creditScale = 0.65;
let fontWidth = 0,
  fontHeight = 8,
  creditWidth = 0,
  creditHeight = 0;
function size() {
  if (!canvas.value || !stage.value || !lines.value) return;
  let scale =
    splashScale(fontWidth) *
    1.8 *
    Math.min(1.5, stage.value.clientWidth / (props.scale ?? 1) / 200) *
    (props.scale ?? 1);
  const angle = ((props.angle ?? splashRotation) * Math.PI) / 180,
    c = Math.abs(Math.cos(angle)),
    s = Math.abs(Math.sin(angle));
  const lineWidth = Math.max(fontWidth + 2, creditWidth * creditScale),
    lineHeight =
      fontHeight + (creditHeight ? 3 + creditHeight * creditScale : 0),
    width = lineWidth * scale,
    height = lineHeight * scale;
  scale *= Math.min(
    1,
    stage.value.clientWidth / Math.max(1, width * c + height * s),
    stage.value.clientHeight / Math.max(1, width * s + height * c),
  );
  lines.value.style.rotate = `${props.angle ?? splashRotation}deg`;
  lines.value.style.width = `${lineWidth * scale}px`;
  lines.value.style.gap = `${3 * scale}px`;
  canvas.value.style.width = `${(fontWidth + 2) * scale}px`;
  canvas.value.style.height = `${fontHeight * scale}px`;
  if (credit.value) {
    credit.value.style.width = `${creditWidth * creditScale * scale}px`;
    credit.value.style.height = `${creditHeight * creditScale * scale}px`;
  }
}
function motion() {
  animation?.cancel();
  animation = undefined;
  if (!lines.value) return;
  if (!reduced.matches)
    animation = lines.value.animate(
      Array.from({ length: 101 }, (_, i) => ({
        scale: splashPulse(i * 10) / 1.8,
        offset: i / 100,
      })),
      { duration: 1000, iterations: Infinity },
    );
  visibility();
}
function visibility() {
  if (document.hidden) animation?.pause();
  else animation?.play();
}
async function draw() {
  const run = ++generation;
  error.value = false;
  const missing = [
    ...new Set(Array.from(props.text + props.source).filter((c) => !glyphs[c])),
  ];
  if (missing.length)
    try {
      const data = await props.rpc<Record<string, string>>(
        "home_widget_glyphs",
        { text: missing.join("") },
      );
      for (const [c, bits] of Object.entries(data)) {
        const glyph = unicodeGlyph(c, bits);
        if (glyph) {
          glyphs[c] = glyph;
          cached[c] = bits;
        }
      }
      try {
        localStorage.setItem(
          cacheKey,
          JSON.stringify(
            Object.fromEntries(Object.entries(cached).slice(-2000)),
          ),
        );
      } catch {
        /* Retain in-memory glyphs. */
      }
    } catch {
      if (!disposed && run === generation) error.value = true;
    }
  if (disposed || run !== generation) return;
  await nextTick();
  if (!canvas.value) return;
  const metrics = paintSplash(canvas.value, props.text, glyphs);
  fontWidth = metrics.width;
  fontHeight = metrics.height;
  if (credit.value && props.source) {
    const metrics = paintSplash(credit.value, props.source, glyphs);
    creditWidth = metrics.width + 2;
    creditHeight = metrics.height;
  } else creditWidth = creditHeight = 0;
  size();
  motion();
}
watch(() => [props.angle, props.scale], size);
watch(
  () => [props.text, props.source],
  () => void draw(),
);
onMounted(() => {
  void draw();
  observer = new ResizeObserver(size);
  if (stage.value) observer.observe(stage.value);
  document.addEventListener("visibilitychange", visibility);
  reduced.addEventListener("change", motion);
});
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  animation?.cancel();
  observer?.disconnect();
  document.removeEventListener("visibilitychange", visibility);
  reduced.removeEventListener("change", motion);
});
</script>
<template>
  <div
    ref="stage"
    class="minecraft-splash"
    :title="source ? `${text}\n${source}` : text"
  >
    <div ref="lines" class="splash-lines">
      <canvas ref="canvas" class="splash-text" role="img" :aria-label="text" />
      <canvas
        v-if="source"
        ref="credit"
        class="splash-credit"
        role="img"
        :aria-label="source"
      />
    </div>
    <button v-if="error" class="splash-font-retry" @click="draw">
      {{ t("重试加载字体") }}
    </button>
  </div>
</template>
<style>
.minecraft-splash {
  width: 100%;
  height: 100%;
  display: grid;
  place-items: center;
  position: relative;
}
.minecraft-splash canvas {
  width: 1px;
  height: 1px;
  image-rendering: pixelated;
  transform-origin: center;
  max-width: none;
}
.splash-lines {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  transform-origin: center;
  transition: rotate 100ms linear;
}
.splash-text {
  align-self: center;
}
.splash-font-retry {
  position: absolute;
  bottom: -4px;
  left: 0;
  right: 0;
  border: 0;
  background: none;
  color: var(--muted);
  font-size: 10px;
}
</style>
