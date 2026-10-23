<script setup lang="ts">
import { Box } from "lucide-vue-next";
import { computed, onBeforeUnmount, ref, useId, watch } from "vue";
import { hexagonPoints, hexPiePoints } from "../versionProgress";

const props = defineProps<{
  selected: boolean;
  installed: boolean;
  progress: number;
  indeterminate: boolean;
}>();
const clipId = useId();
const target = computed(() =>
  props.installed
    ? 100
    : Number.isFinite(props.progress)
      ? Math.min(100, Math.max(0, props.progress))
      : 0,
);
const displayed = ref(target.value);
let frame: number | undefined;
function cancelMotion() {
  if (frame !== undefined) cancelAnimationFrame(frame);
  frame = undefined;
}
watch(target, (value) => {
  cancelMotion();
  if (
    !props.selected ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  ) {
    displayed.value = value;
    return;
  }
  const from = displayed.value;
  const start = performance.now();
  const tick = (now: number) => {
    const elapsed = Math.min(1, Math.max(0, (now - start) / 360));
    displayed.value = from + (value - from) * (1 - (1 - elapsed) ** 3);
    if (elapsed < 1) frame = requestAnimationFrame(tick);
    else frame = undefined;
  };
  frame = requestAnimationFrame(tick);
});
onBeforeUnmount(cancelMotion);
</script>

<template>
  <svg
    v-if="selected"
    class="version-menu-icon version-pie"
    :class="{ indeterminate }"
    width="16"
    height="16"
    viewBox="0 0 100 100"
    aria-hidden="true"
  >
    <defs>
      <clipPath :id="clipId" clipPathUnits="userSpaceOnUse">
        <polygon :points="hexPiePoints(displayed)" />
      </clipPath>
    </defs>
    <polygon class="version-pie-track" :points="hexagonPoints" />
    <polygon
      class="version-pie-fill"
      :points="hexagonPoints"
      :clip-path="`url(#${clipId})`"
    />
    <polygon class="version-pie-outline" :points="hexagonPoints" />
  </svg>
  <Box
    v-else
    class="version-menu-icon"
    :size="16"
    :stroke-dasharray="installed ? undefined : '2 3'"
    aria-hidden="true"
  />
</template>
