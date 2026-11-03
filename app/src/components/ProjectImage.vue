<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { Package } from "lucide-vue-next";
import {
  projectImageUrl,
  projectMedia,
  type ResourceRpc,
} from "../projectPresentation";
const props = withDefaults(
  defineProps<{
    url?: string | null;
    rpc: ResourceRpc;
    size?: number;
    alt?: string;
    cover?: boolean;
  }>(),
  { size: 36, alt: "" },
);
const root = ref<HTMLElement>();
const source = ref("");
let generation = 0,
  visible = false,
  disposed = false;
let observer: IntersectionObserver | undefined;
async function load() {
  const request = ++generation;
  source.value = "";
  if (!visible || !props.url || !projectImageUrl(props.url)) return;
  try {
    const result = await projectMedia(props.rpc).image(props.url);
    if (!disposed && request === generation) source.value = result;
  } catch {
    /* Keep the category icon when a remote image is missing. */
  }
}
onMounted(() => {
  if (typeof IntersectionObserver === "undefined") {
    visible = true;
    void load();
    return;
  }
  observer = new IntersectionObserver((entries) => {
    if (entries.some((entry) => entry.isIntersecting)) {
      visible = true;
      observer?.disconnect();
      void load();
    }
  });
  if (root.value) observer.observe(root.value);
});
watch(
  () => props.url,
  () => void load(),
);
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  observer?.disconnect();
});
</script>
<template>
  <span
    ref="root"
    class="project-image"
    :class="{ cover }"
    :style="{ '--project-image-size': `${size}px` }"
  >
    <img
      v-if="source"
      :src="source"
      :alt="alt"
      decoding="async"
      draggable="false"
      @error="source = ''"
    />
    <slot v-else><Package :size="size * 0.65" /></slot>
  </span>
</template>
<style scoped>
.project-image {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: var(--project-image-size);
  height: var(--project-image-size);
  flex-shrink: 0;
  overflow: hidden;
  color: var(--blue);
}
.project-image img {
  display: block;
  max-width: 100%;
  max-height: 100%;
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.project-image.cover {
  width: 100%;
  height: 144px;
  background: color-mix(in srgb, var(--ink) 4%, transparent);
}
</style>
