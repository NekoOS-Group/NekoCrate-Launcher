<script setup lang="ts">
import { t, translateMessage } from "../i18n";
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { RefreshCw, LoaderCircle } from "lucide-vue-next";
import type { SkinViewer } from "skinview3d";
import { loadSkinImage, skinAvatar } from "../previewSkin";
import { createDemandRenderer } from "../demandRenderer";
import { createSkinIdleMotion } from "../skinIdleMotion";
import { createSkinCarousel } from "../skinCarousel";
import type { PatFrame } from "../skinReactions";
const props = defineProps<{
  texture: string | null;
  slim: boolean;
  paused: boolean;
  idle?: boolean;
  playful?: boolean;
  identity?: string;
  direction?: number;
  caption: string;
  loading: boolean;
  issue: string;
}>();
const emit = defineEmits<{
  avatar: [url: string];
  failed: [message: string];
  retry: [];
}>();
const host = ref<HTMLDivElement>();
const canvas = ref<HTMLCanvasElement>();
const patLayer = ref<HTMLDivElement>();
const failure = ref("");
const unavailable = ref(false);
let viewer: SkinViewer | undefined;
let observer: ResizeObserver | undefined;
let disposed = false;
let generation = 0;
let drawing: ReturnType<typeof createDemandRenderer> | undefined;
let contextLost = false;
let idleMotion: ReturnType<typeof createSkinIdleMotion> | undefined;
let reducedMotion: MediaQueryList | undefined;
let carousel: ReturnType<typeof createSkinCarousel> | undefined;
let displayedIdentity: string | undefined;
let hasSkin = false;
let padded = false;
let paddingX = 0,
  paddingTop = 0;
let reactions:
  ReturnType<typeof import("../skinReactions").createSkinReactions> | undefined;
let outline:
  ReturnType<typeof import("../skinOutline").createSkinOutline> | undefined;
function invalidate() {
  drawing?.invalidate();
}
function loseContext() {
  contextLost = true;
  updatePause();
}
function restoreContext() {
  contextLost = false;
  updatePause();
}
function reset() {
  if (!viewer) return;
  viewer.resetCameraPose();
  viewer.camera.position.set(18, 7, 60);
  viewer.adjustCameraDistance();
  viewer.controls.update();
  invalidate();
}
function rotate(event: KeyboardEvent) {
  if (!viewer || !["ArrowLeft", "ArrowRight", "Home"].includes(event.key))
    return;
  event.preventDefault();
  if (event.key === "Home") {
    reset();
    return;
  }
  viewer.camera.position.applyAxisAngle(
    viewer.camera.up,
    event.key === "ArrowLeft" ? -0.2 : 0.2,
  );
  viewer.controls.update();
}
function showPat(frame: PatFrame | null) {
  const layer = patLayer.value;
  if (!layer) return;
  layer.hidden = !frame;
  if (!frame) return;
  const p = frame.progress,
    wave = frame.reduced ? 0 : Math.sin((p * 1.45 * Math.PI * 2) / 0.46);
  layer.style.transform = `translate3d(${frame.x - paddingX}px,${frame.y - paddingTop}px,0)`;
  layer.style.opacity = String(
    frame.reduced ? 1 : Math.min(1, p / 0.08, (1 - p) / 0.15),
  );
  layer.style.setProperty("--pat-x", `${wave * 6}px`);
  layer.style.setProperty(
    "--pat-y",
    `${frame.reduced ? 0 : (1 - Math.cos((p * 1.45 * Math.PI * 2) / 0.46)) * 2}px`,
  );
  layer.style.setProperty("--pat-turn", `${wave * 7}deg`);
}
function updatePause() {
  const paused = props.paused || document.hidden || contextLost;
  drawing?.setPaused(paused);
  idleMotion?.setPaused(paused || !props.idle || !!reducedMotion?.matches);
  // Account selection happens in a dialog: its short transition remains visible
  // behind that dialog, while continuous idle rendering stays paused.
  carousel?.setPaused(document.hidden || contextLost);
  if (paused || reducedMotion?.matches) {
    const active = reactions?.active;
    reactions?.cancel();
    if (active && !document.hidden && !contextLost) renderScene();
  }
}
function resizeViewport() {
  if (!viewer || !host.value || !canvas.value) return;
  const { width, height, left, right, top, bottom } =
    host.value.getBoundingClientRect();
  if (width <= 0 || height <= 0) return;
  padded = !!reactions?.active;
  paddingX = padded
    ? Math.floor(Math.min(100, Math.max(0, Math.min(left, innerWidth - right))))
    : 0;
  paddingTop = padded ? Math.floor(Math.min(150, Math.max(0, top))) : 0;
  const below = padded
    ? Math.floor(Math.min(32, Math.max(0, innerHeight - bottom)))
    : 0;
  viewer.setSize(width + paddingX * 2, height + paddingTop + below);
  canvas.value.style.left = `${-paddingX}px`;
  canvas.value.style.top = `${-paddingTop}px`;
  if (padded) {
    // Extend the frustum around the original viewport. The character's screen
    // position and pixel size stay identical; only effect drawing space grows.
    viewer.camera.setViewOffset(
      width,
      height,
      -paddingX,
      -paddingTop,
      viewer.width,
      viewer.height,
    );
  } else viewer.camera.clearViewOffset();
}
function renderScene() {
  const now = performance.now();
  if (padded !== !!reactions?.active) resizeViewport();
  reactions?.restore();
  idleMotion?.update(now);
  if (reactions?.update(now)) invalidate();
  if (padded && !reactions?.active) resizeViewport();
  outline?.resize();
  viewer?.render();
}
async function loadSkin() {
  if (!viewer) return;
  const request = ++generation;
  // Keep the displayed skin until the next account's actual texture is ready.
  if (props.loading) return;
  reactions?.cancel();
  const identity = props.identity;
  const direction = props.direction || 1;
  const model = props.texture && props.slim ? "slim" : "default";
  const show = async (texture: HTMLImageElement, slim = model === "slim") => {
    if (disposed || request !== generation) return;
    await carousel?.show(
      () => {
        if (!viewer) return;
        viewer.loadSkin(texture, { model: slim ? "slim" : "default" });
        viewer.playerObject.visible = true;
        displayedIdentity = identity;
        hasSkin = true;
        // Texture exchange also needs one redraw while the account dialog is open.
        if (!contextLost) renderScene();
        if (request === generation)
          emit("avatar", skinAvatar(texture, texture.width));
      },
      direction,
      hasSkin &&
        !!identity &&
        (identity !== displayedIdentity || carousel.active) &&
        !reducedMotion?.matches &&
        !contextLost,
    );
  };
  try {
    const texture = await loadSkinImage(props.texture);
    if (disposed || request !== generation) return;
    failure.value = "";
    await show(texture);
  } catch {
    if (disposed || request !== generation) return;
    // Remove the previous account's skin when a new texture cannot be decoded.
    try {
      const fallback = await loadSkinImage(null);
      if (disposed || request !== generation) return;
      await show(fallback, false);
    } catch {
      if (disposed || request !== generation) return;
      viewer.playerObject.visible = false;
      invalidate();
      emit("avatar", "");
    }
    if (disposed || request !== generation) return;
    failure.value = "皮肤纹理无法读取";
    emit("failed", failure.value);
  }
}
onMounted(async () => {
  try {
    const [{ SkinViewer }, { createSkinOutline }, { createSkinReactions }] =
      await Promise.all([
        import("skinview3d"),
        import("../skinOutline"),
        import("../skinReactions"),
      ]);
    if (disposed || !canvas.value || !host.value) return;
    viewer = new SkinViewer({
      canvas: canvas.value,
      width: host.value.clientWidth,
      height: host.value.clientHeight,
      zoom: 0.91,
      fov: 32,
      pixelRatio: Math.min(devicePixelRatio, 2),
      renderPaused: true,
    });
    // The scene must composite over the continuous launcher background.
    viewer.background = null;
    viewer.renderer.setClearColor(0x000000, 0);
    viewer.controls.enablePan = false;
    viewer.controls.enableZoom = false;
    viewer.controls.enableDamping = true;
    viewer.controls.rotateSpeed = 0.65;
    viewer.controls.minPolarAngle = Math.PI / 3;
    viewer.controls.maxPolarAngle = Math.PI * 0.64;
    viewer.globalLight.intensity = 2.5;
    viewer.cameraLight.intensity = 0.7;
    drawing = createDemandRenderer(
      () => viewer?.controls.update() ?? false,
      renderScene,
    );
    carousel = createSkinCarousel(canvas.value);
    idleMotion = createSkinIdleMotion(viewer.playerObject.skin, invalidate);
    reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
    reducedMotion.addEventListener("change", updatePause);
    reactions = createSkinReactions(viewer, canvas.value, {
      enabled: () =>
        !!props.playful &&
        hasSkin &&
        !props.paused &&
        !props.loading &&
        !document.hidden &&
        !contextLost &&
        !carousel?.active,
      reduced: () => !!reducedMotion?.matches,
      invalidate,
      pat: showPat,
    });
    outline = createSkinOutline(viewer, canvas.value, invalidate);
    viewer.controls.addEventListener("change", invalidate);
    canvas.value.addEventListener("webglcontextlost", loseContext);
    canvas.value.addEventListener("webglcontextrestored", restoreContext);
    reset();
    observer = new ResizeObserver(() => {
      resizeViewport();
      invalidate();
    });
    observer.observe(host.value);
    document.addEventListener("visibilitychange", updatePause);
    updatePause();
    await loadSkin();
  } catch {
    unavailable.value = true;
    failure.value = "当前设备无法创建 3D 皮肤预览";
  }
});
watch(
  () => [props.texture, props.slim, props.loading, props.identity],
  loadSkin,
);
watch(() => [props.paused, props.idle], updatePause);
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  observer?.disconnect();
  idleMotion?.dispose();
  reactions?.dispose();
  carousel?.dispose();
  reducedMotion?.removeEventListener("change", updatePause);
  drawing?.dispose();
  viewer?.controls.removeEventListener("change", invalidate);
  canvas.value?.removeEventListener("webglcontextlost", loseContext);
  canvas.value?.removeEventListener("webglcontextrestored", restoreContext);
  document.removeEventListener("visibilitychange", updatePause);
  outline?.dispose();
  viewer?.dispose();
});
</script>

<template>
  <section class="skin-stage" :aria-label="caption || t('玩家皮肤')">
    <div ref="host" class="skin-viewport" :class="{ loading }">
      <canvas
        ref="canvas"
        v-show="!unavailable"
        tabindex="0"
        :aria-label="
          t(
            playful
              ? '玩家皮肤，单击压扁，双击摸头；拖拽或方向键旋转，Home 复位，Enter 压扁，Shift+Enter 摸头'
              : '玩家皮肤，拖拽旋转；方向键左右旋转，Home 复位',
          )
        "
        @keydown="rotate"
        @dblclick.prevent="!playful && reset()"
      />
      <div ref="patLayer" class="skin-pat" hidden aria-hidden="true">
        <svg
          class="skin-pat-hand"
          viewBox="0 0 34 24"
          shape-rendering="crispEdges"
        >
          <path
            class="pat-hand-outline"
            d="M22 2h8v2h4v12h-5v2h-7v3h-8v-2H8v-2H3v-2H1V9h3V7h5V5h7V3h6z"
          />
          <path
            class="pat-hand-palm"
            d="M22 4h6v12h-7v3h-6v-2H9v-2H4v-2H3v-3h3V9h4V7h7V5h5z"
          />
          <path
            class="pat-hand-shade"
            d="M5 12h9v2H5zm5 3h8v2h-8zm6 2h5v2h-5z"
          />
          <path class="pat-hand-cuff" d="M28 5h4v9h-4z" />
        </svg>
      </div>
      <p v-if="unavailable" class="skin-unavailable" role="status">
        {{ t(failure) }}
      </p>
    </div>
    <div
      v-if="!unavailable && (loading || issue || failure)"
      class="skin-tools"
      :class="{ visible: loading || issue || failure }"
    >
      <span
        v-if="loading"
        class="skin-loading"
        role="status"
        :aria-label="t('正在加载皮肤')"
        ><LoaderCircle :size="16" class="spin"
      /></span>
      <button
        v-else-if="issue || failure"
        class="icon-button"
        :title="
          t('{error}，点击重试', { error: translateMessage(issue || failure) })
        "
        :aria-label="t('重新加载皮肤')"
        @click="emit('retry')"
      >
        <RefreshCw :size="16" />
      </button>
    </div>
  </section>
</template>
