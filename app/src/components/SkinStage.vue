<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { RefreshCw, LoaderCircle } from "lucide-vue-next";
import type { SkinViewer } from "skinview3d";
import { loadSkinImage, skinAvatar } from "../previewSkin";
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
function reset() {
  if (!viewer) return;
  viewer.resetCameraPose();
  viewer.camera.position.set(18, 7, 60);
  viewer.adjustCameraDistance();
  viewer.controls.update();
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
  if (viewer) viewer.renderPaused = props.paused || document.hidden;
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
      emit("avatar", skinAvatar(fallback, fallback.width));
    } catch {
      if (disposed || request !== generation) return;
      viewer.playerObject.visible = false;
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
    reset();
    observer = new ResizeObserver(([entry]) => {
      if (entry && entry.contentRect.width > 0 && entry.contentRect.height > 0)
        viewer?.setSize(entry.contentRect.width, entry.contentRect.height);
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
  document.removeEventListener("visibilitychange", updatePause);
  viewer?.dispose();
});
</script>

<template>
  <section class="skin-stage" :aria-label="caption || '玩家皮肤'">
    <div ref="host" class="skin-viewport" :class="{ loading }">
      <canvas
        ref="canvas"
        v-show="!unavailable"
        tabindex="0"
        aria-label="玩家皮肤，拖拽旋转；方向键左右旋转，Home 复位"
        :title="[caption, '拖拽旋转，双击复位'].filter(Boolean).join(' · ')"
        @keydown="rotate"
        @dblclick="reset"
      />
      <p v-if="unavailable" class="skin-unavailable" role="status">
        {{ failure }}
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
        aria-label="正在加载皮肤"
        ><LoaderCircle :size="16" class="spin"
      /></span>
      <button
        v-else-if="issue || failure"
        class="icon-button"
        :title="`${issue || failure}，点击重试`"
        aria-label="重新加载皮肤"
        @click="emit('retry')"
      >
        <RefreshCw :size="16" />
      </button>
    </div>
  </section>
</template>
