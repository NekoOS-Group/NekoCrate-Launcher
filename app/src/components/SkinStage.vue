<script setup lang="ts">
import { t, translateMessage } from "../i18n";
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { RefreshCw, LoaderCircle } from "lucide-vue-next";
import type { SkinViewer } from "skinview3d";
import { loadSkinImage, skinAvatar } from "../previewSkin";
import { createDemandRenderer } from "../demandRenderer";
const props = defineProps<{
  texture: string | null;
  slim: boolean;
  paused: boolean;
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
const failure = ref("");
const unavailable = ref(false);
let viewer: SkinViewer | undefined;
let observer: ResizeObserver | undefined;
let disposed = false;
let generation = 0;
let drawing: ReturnType<typeof createDemandRenderer> | undefined;
let contextLost = false;
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
function updatePause() {
  drawing?.setPaused(props.paused || document.hidden || contextLost);
}
async function loadSkin() {
  if (!viewer) return;
  const request = ++generation;
  try {
    const texture = await loadSkinImage(props.texture);
    if (disposed || request !== generation) return;
    viewer.loadSkin(texture, {
      model: props.texture && props.slim ? "slim" : "default",
    });
    viewer.playerObject.visible = true;
    invalidate();
    emit("avatar", skinAvatar(texture, texture.width));
    failure.value = "";
  } catch {
    if (disposed || request !== generation) return;
    // Remove the previous account's skin when a new texture cannot be decoded.
    try {
      const fallback = await loadSkinImage(null);
      if (disposed || request !== generation) return;
      viewer.loadSkin(fallback, { model: "default" });
      viewer.playerObject.visible = true;
      invalidate();
      emit("avatar", skinAvatar(fallback, fallback.width));
    } catch {
      if (disposed || request !== generation) return;
      viewer.playerObject.visible = false;
      invalidate();
      emit("avatar", "");
    }
    failure.value = "皮肤纹理无法读取";
    emit("failed", failure.value);
  }
}
onMounted(async () => {
  try {
    const { SkinViewer } = await import("skinview3d");
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
      () => viewer?.render(),
    );
    viewer.controls.addEventListener("change", invalidate);
    canvas.value.addEventListener("webglcontextlost", loseContext);
    canvas.value.addEventListener("webglcontextrestored", restoreContext);
    reset();
    observer = new ResizeObserver(([entry]) => {
      if (!entry || !viewer) return;
      const { width, height } = entry.contentRect;
      if (
        width > 0 &&
        height > 0 &&
        (viewer.width !== width || viewer.height !== height)
      ) {
        viewer.setSize(width, height);
        invalidate();
      }
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
watch(() => [props.texture, props.slim], loadSkin);
watch(() => props.paused, updatePause);
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  observer?.disconnect();
  drawing?.dispose();
  viewer?.controls.removeEventListener("change", invalidate);
  canvas.value?.removeEventListener("webglcontextlost", loseContext);
  canvas.value?.removeEventListener("webglcontextrestored", restoreContext);
  document.removeEventListener("visibilitychange", updatePause);
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
        :aria-label="t('玩家皮肤，拖拽旋转；方向键左右旋转，Home 复位')"
        @keydown="rotate"
        @dblclick="reset"
      />
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
