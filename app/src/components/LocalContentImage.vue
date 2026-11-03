<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { Package } from "lucide-vue-next";
import ProjectImage from "./ProjectImage.vue";
import {
  localContentMedia,
  type ContentPresentation,
} from "../localContentPresentation";
import type { ResourceRpc } from "../projectPresentation";
import type { LocalContentFile, ContentKind } from "../types";
import { modFileName } from "../contentIdentity";
const props = withDefaults(
  defineProps<{
    rpc: ResourceRpc;
    instanceId: string;
    version: string;
    kind: ContentKind;
    file: LocalContentFile;
    size?: number;
    onlineIcon?: string | null;
  }>(),
  { size: 32 },
);
const emit = defineEmits<{ presentation: [value: ContentPresentation] }>();
const root = ref<HTMLElement>();
const source = ref("");
const remote = ref<string>();
let observer: IntersectionObserver | undefined,
  generation = 0,
  visible = false,
  disposed = false;
async function load() {
  const request = ++generation;
  if (!visible) return;
  try {
    const result = await localContentMedia(props.rpc).load(
      { id: props.instanceId, version: props.version, kind: props.kind },
      props.file,
    );
    if (!disposed && request === generation) {
      source.value = result.icon || "";
      emit("presentation", result);
      if (!source.value && result.project) {
        const url = await localContentMedia(props.rpc).onlineIcon(
          result.project,
        );
        if (!disposed && request === generation) remote.value = url;
      }
    }
  } catch {
    /* Invalid/missing artwork keeps the category icon, without a toast. */
  }
}
watch(
  () => [
    props.instanceId,
    props.version,
    props.kind,
    props.file.file_name,
    props.file.size,
    props.file.declaration?.status,
  ],
  (current, previous) => {
    if (
      !previous ||
      current.slice(0, 3).join() !== previous.slice(0, 3).join() ||
      modFileName(String(current[3])) !== modFileName(String(previous[3])) ||
      current[4] !== previous[4]
    ) {
      source.value = "";
      remote.value = undefined;
    }
    void load();
  },
);
onMounted(() => {
  if (typeof IntersectionObserver === "undefined") {
    visible = true;
    void load();
    return;
  }
  observer = new IntersectionObserver((entries) => {
    if (entries.some((e) => e.isIntersecting)) {
      visible = true;
      observer?.disconnect();
      void load();
    }
  });
  if (root.value) observer.observe(root.value);
});
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  observer?.disconnect();
});
</script>
<template>
  <span
    ref="root"
    class="local-content-image"
    :style="{ width: `${size}px`, height: `${size}px` }"
  >
    <img
      v-if="source"
      :src="source"
      alt=""
      decoding="async"
      draggable="false"
      @error="source = ''"
    />
    <ProjectImage
      v-else-if="onlineIcon || remote"
      :url="onlineIcon || remote"
      :rpc="rpc"
      :size="size"
      ><slot><Package :size="size * 0.65" /></slot
    ></ProjectImage>
    <slot v-else><Package :size="size * 0.65" /></slot>
  </span>
</template>
<style scoped>
.local-content-image {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: var(--tab-color, var(--blue));
}
img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  user-select: none;
  -webkit-user-drag: none;
}
</style>
