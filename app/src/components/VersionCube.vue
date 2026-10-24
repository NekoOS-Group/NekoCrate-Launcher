<script setup lang="ts">
import type { VersionCategory } from "../versionCatalog";

withDefaults(defineProps<{ variant: VersionCategory; size?: number }>(), {
  size: 20,
});
</script>

<template>
  <svg
    class="version-cube"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-linejoin="round"
    stroke-linecap="round"
    aria-hidden="true"
    focusable="false"
  >
    <!-- A lifted top distinguishes snapshots from the closed release cube. -->
    <template v-if="variant === 'snapshot'">
      <path
        d="m12 1.5 8.66 5L12 11.5l-8.66-5Z"
        fill="currentColor"
        fill-opacity=".2"
      />
      <path
        d="m3.34 10.5 8.66 5 8.66-5v7L12 22.5l-8.66-5Z"
        fill="currentColor"
        fill-opacity=".1"
      />
      <path d="M12 15.5v7" />
    </template>
    <template v-else>
      <g fill="currentColor" stroke="none">
        <path
          d="m12 2 8.66 5L12 12 3.34 7Z"
          :fill-opacity="variant === 'development' ? 0.05 : 0.22"
        />
        <path d="M3.34 7 12 12v10l-8.66-5Z" fill-opacity=".06" />
        <path
          d="m12 12 8.66-5v10L12 22Z"
          :fill-opacity="variant === 'development' ? 0.03 : 0.13"
        />
      </g>
      <path
        d="m12 2 8.66 5v10L12 22l-8.66-5V7Z"
        :stroke-dasharray="variant === 'development' ? '2 2.3' : undefined"
      />
      <path d="m3.34 7 8.66 5 8.66-5M12 12v10" />
      <!-- Wireframe vertices for early builds; dice faces for April Fools. -->
      <g v-if="variant === 'development'" fill="currentColor" stroke="none">
        <rect x="11" y="1" width="2" height="2" />
        <rect x="2.34" y="16" width="2" height="2" />
        <rect x="19.66" y="16" width="2" height="2" />
      </g>
      <g v-else-if="variant === 'april'" fill="currentColor" stroke="none">
        <path d="m12 5.2 1.8 1.04L12 7.28l-1.8-1.04Z" />
        <path d="m6.6 11.7 1.8 1.04v2.08L6.6 13.78Z" />
        <path
          d="m15 12.5 1.6-.92v1.84l-1.6.92Zm2.4 3.4 1.6-.92v1.84l-1.6.92Z"
        />
      </g>
    </template>
  </svg>
</template>

<style scoped>
.version-cube {
  display: block;
  flex-shrink: 0;
  stroke-width: 1.35;
}
</style>
