<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { Shirt, UserRound } from "lucide-vue-next";
import { loadSkinImage, skinAvatar } from "../previewSkin";
const props = defineProps<{
  load: () => Promise<string | null>;
  renderModel: (image: HTMLImageElement, slim: boolean) => Promise<string>;
  slim: boolean;
  inUse?: string;
  guest?: boolean;
  live: boolean;
}>();
const host = ref<HTMLElement>();
const avatar = ref("");
const model = ref("");
let loadedImage: HTMLImageElement | undefined;
let generation = 0;
let observer: IntersectionObserver | undefined;
let disposed = false;
async function read() {
  try {
    const texture = await props.load();
    if (disposed) return;
    const image = await loadSkinImage(texture);
    if (disposed) return;
    loadedImage = image;
    avatar.value = skinAvatar(image, image.width);
    await drawModel();
  } catch {
    /* The selected preview reports invalid textures without one toast per tile. */
  }
}
async function drawModel() {
  if (!loadedImage) return;
  const request = ++generation;
  const result = await props.renderModel(loadedImage, props.slim);
  if (!disposed && request === generation) model.value = result;
}
watch(
  () => props.slim,
  () => {
    void drawModel().catch(() => {});
  },
);
onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        observer?.disconnect();
        void read();
      }
    },
    { root: host.value?.closest(".rm-scroll"), rootMargin: "64px" },
  );
  if (host.value) observer.observe(host.value);
});
onBeforeUnmount(() => {
  disposed = true;
  observer?.disconnect();
});
</script>
<template>
  <span ref="host" class="skin-library-model">
    <img
      v-if="model"
      class="skin-library-thumbnail"
      :class="{ hidden: live }"
      :src="model"
      alt=""
    />
    <img
      v-else-if="avatar"
      class="skin-library-fallback"
      :src="avatar"
      alt=""
    />
    <Shirt v-else :size="24" />
    <span v-if="inUse || guest" class="skin-library-owners" aria-hidden="true">
      <span v-if="inUse" class="skin-library-owner">
        <img v-if="avatar" :src="avatar" alt="" /><Shirt v-else :size="12" />
        <i />
      </span>
      <span v-if="guest" class="skin-library-owner guest-owner">
        <img v-if="avatar" :src="avatar" alt="" /><Shirt v-else :size="12" />
        <UserRound :size="10" class="guest-owner-mark" />
      </span>
    </span>
  </span>
</template>
<style scoped>
.skin-library-model {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100px;
  height: 100px;
  flex: none;
}
.skin-library-thumbnail {
  width: 100%;
  height: 100%;
  object-fit: contain;
  image-rendering: pixelated;
}
.skin-library-thumbnail.hidden {
  opacity: 0;
}
.skin-library-fallback {
  width: 30px;
  height: 30px;
  image-rendering: pixelated;
}
.skin-library-owners {
  position: absolute;
  right: 7px;
  bottom: 7px;
  z-index: 3;
  display: flex;
  gap: 6px;
  pointer-events: none;
}
.skin-library-owner {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: 1px solid color-mix(in srgb, var(--ink) 40%, transparent);
  background: var(--surface);
  box-shadow: 0 1px 6px #0005;
  pointer-events: none;
}
.skin-library-owner img {
  width: 14px;
  height: 14px;
  image-rendering: pixelated;
}
.skin-library-owner i {
  position: absolute;
  right: -2px;
  bottom: -2px;
  width: 4px;
  height: 4px;
  background: #75dbb8;
  box-shadow: 0 0 5px #75dbb880;
}
.guest-owner-mark {
  position: absolute;
  right: -4px;
  bottom: -4px;
  padding: 1px;
  color: var(--ink);
  background: var(--night);
  box-sizing: content-box;
}
</style>
