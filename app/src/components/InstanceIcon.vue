<script setup lang="ts">
import { computed, useId } from "vue";
import { instanceIcons } from "../instanceIcons";

const props = defineProps<{ icon: number }>();
const material = computed(
  () =>
    instanceIcons.find((item) => item.id === props.icon) || instanceIcons[0]!,
);
const tintId = useId();
const tintMatrix = computed(() => {
  const tint = material.value.faces?.tint;
  if (!tint) return undefined;
  const [red, green, blue] = tint.map((channel) => channel / 255);
  return `${red} 0 0 0 0  0 ${green} 0 0 0  0 0 ${blue} 0 0  0 0 0 1 0`;
});
const cubeFaces = computed(() => {
  const faces = material.value.faces;
  if (!faces) return [];
  return [
    {
      id: "top",
      texture: faces.top,
      transform: "matrix(1.625 .9375 -1.625 .9375 32 4)",
      shade: 0,
      tint: !!faces.tint,
      overlay: undefined,
    },
    {
      id: "side",
      texture: faces.side,
      transform: "matrix(1.625 .9375 0 1.625 6 19)",
      shade: 0.12,
      tint: false,
      overlay: faces.sideOverlay,
    },
    {
      id: "front",
      texture: faces.front || faces.side,
      transform: "matrix(1.625 -.9375 0 1.625 32 34)",
      shade: 0.28,
      tint: false,
      overlay: faces.sideOverlay,
    },
  ];
});
</script>

<template>
  <svg
    v-if="material.faces"
    class="instance-art block-art"
    viewBox="0 0 64 64"
    aria-hidden="true"
  >
    <defs v-if="tintMatrix">
      <filter :id="tintId" color-interpolation-filters="sRGB">
        <feColorMatrix type="matrix" :values="tintMatrix" />
      </filter>
    </defs>
    <!-- Vanilla slime has a 10px inner cube (UV 3–13) inside its 16px shell. -->
    <g v-if="material.faces.inset" transform="translate(12 12) scale(.625)">
      <svg
        v-for="face in cubeFaces"
        :key="face.id"
        :transform="face.transform"
        width="16"
        height="16"
        viewBox="3 3 10 10"
        overflow="hidden"
      >
        <image :href="face.texture" width="16" height="16" />
        <rect
          v-if="face.shade"
          x="3"
          y="3"
          width="10"
          height="10"
          fill="#000"
          :fill-opacity="face.shade"
        />
      </svg>
    </g>
    <g v-for="face in cubeFaces" :key="face.id" :transform="face.transform">
      <image
        :href="face.texture"
        width="16"
        height="16"
        :filter="face.tint ? `url(#${tintId})` : undefined"
      />
      <image
        v-if="face.overlay"
        :href="face.overlay"
        width="16"
        height="16"
        :filter="tintMatrix ? `url(#${tintId})` : undefined"
      />
      <rect
        v-if="face.shade"
        width="16"
        height="16"
        fill="#000"
        :fill-opacity="face.shade"
      />
    </g>
  </svg>
  <span
    v-else
    class="instance-art legacy-block-art"
    :data-material="material.id"
    aria-hidden="true"
  />
</template>
