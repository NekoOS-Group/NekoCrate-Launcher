<script setup lang="ts">
import { computed } from "vue";
import { activeDialog } from "../dialogSurface";
import { dropHint } from "../dropSurface";
const above = computed(
  () =>
    !!dropHint.value &&
    dropHint.value.top + dropHint.value.height > window.innerHeight - 48,
);
const compact = computed(
  () =>
    !!dropHint.value &&
    (dropHint.value.height < 100 || dropHint.value.width < 200),
);
</script>
<template>
  <Teleport :to="activeDialog || 'body'">
    <Transition name="drop-target">
      <div
        v-if="dropHint"
        class="drop-target"
        :class="{ invalid: dropHint.invalid, compact, above }"
        :style="{
          left: `${dropHint.left}px`,
          top: `${dropHint.top}px`,
          width: `${dropHint.width}px`,
          height: `${dropHint.height}px`,
        }"
      >
        <span role="status">{{ dropHint.message }}</span>
      </div>
    </Transition>
  </Teleport>
</template>
<style>
.drop-target {
  position: fixed;
  z-index: 160;
  pointer-events: none;
  border: 1px dashed var(--blue);
  color: var(--blue);
  box-shadow: inset 0 0 32px var(--glow);
  display: grid;
  place-items: center;
  padding: 12px;
  transition:
    left 140ms ease-out,
    top 140ms ease-out,
    width 140ms ease-out,
    height 140ms ease-out,
    color 120ms,
    border-color 120ms;
}
.drop-target span {
  max-width: 100%;
  text-align: center;
  font-size: 13px;
  line-height: 1.5;
  overflow-wrap: anywhere;
  text-shadow: 0 1px 7px var(--night);
}
.drop-target.compact span {
  position: absolute;
  top: calc(100% + 5px);
  width: max-content;
  max-width: min(300px, 80vw);
  font-size: 11px;
}
.drop-target.compact.above span {
  top: auto;
  bottom: calc(100% + 5px);
}
.drop-target.invalid {
  color: #ed9999;
  border-color: currentColor;
  box-shadow: none;
}
.drop-target-enter-active,
.drop-target-leave-active {
  transition: opacity 120ms;
}
.drop-target-enter-from,
.drop-target-leave-to {
  opacity: 0;
}
html.resource-transferring .bottom-bar {
  z-index: 100;
}
.overlay-dialog.drop-interaction {
  pointer-events: none;
  z-index: 90;
  padding-bottom: 112px;
}
.overlay-dialog.drop-interaction > section {
  pointer-events: auto;
  max-height: calc(100vh - 150px);
}
@media (prefers-reduced-motion: reduce) {
  .drop-target {
    transition: none;
  }
}
</style>
