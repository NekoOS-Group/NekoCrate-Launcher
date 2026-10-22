<script setup lang="ts">
import { computed } from "vue";
import { instanceIcons } from "../instanceIcons";

const props = defineProps<{ icon: number }>();
const material = computed(
  () =>
    instanceIcons.find((item) => item.id === props.icon) || instanceIcons[0]!,
);
</script>

<template>
  <svg
    v-if="material.faces"
    class="instance-art block-art"
    viewBox="0 0 64 64"
    aria-hidden="true"
  >
    <image
      :href="material.faces.top"
      width="16"
      height="16"
      transform="matrix(1.625 .9375 -1.625 .9375 32 4)"
    />
    <image
      :href="material.faces.side"
      width="16"
      height="16"
      transform="matrix(1.625 .9375 0 1.625 6 19)"
    />
    <path d="M6 19 32 34V60L6 45Z" fill="#000" fill-opacity=".12" />
    <image
      :href="material.faces.side"
      width="16"
      height="16"
      transform="matrix(1.625 -.9375 0 1.625 32 34)"
    />
    <path d="M32 34 58 19V45L32 60Z" fill="#000" fill-opacity=".28" />
  </svg>
  <span
    v-else
    class="instance-art legacy-block-art"
    :data-material="material.id"
    aria-hidden="true"
  />
</template>
