<script setup lang="ts">
defineProps<{ value: number | undefined }>();
</script>

<template>
  <span
    class="animated-count"
    :style="{
      width: value === undefined ? '0ch' : `${String(value).length + 2}ch`,
    }"
    :aria-label="value === undefined ? undefined : `(${value})`"
  >
    <Transition name="count-roll">
      <span v-if="value !== undefined" :key="value" aria-hidden="true"
        >({{ value }})</span
      >
    </Transition>
  </span>
</template>

<style scoped>
.animated-count {
  position: relative;
  top: 1px;
  display: inline-block;
  flex: 0 0 auto;
  height: 20px;
  overflow: hidden;
  font-family: var(--mono);
  font-size: 11px;
  line-height: 20px;
  font-variant-numeric: tabular-nums;
  transition: width 420ms cubic-bezier(0.16, 1, 0.3, 1);
}
.animated-count > span {
  position: absolute;
  inset: 0 auto auto 0;
  white-space: nowrap;
}
.count-roll-enter-active,
.count-roll-leave-active {
  transition:
    transform 360ms cubic-bezier(0.16, 1, 0.3, 1),
    opacity 220ms ease-out;
}
.count-roll-enter-from {
  opacity: 0;
  transform: translateY(100%);
}
.count-roll-leave-to {
  opacity: 0;
  transform: translateY(-100%);
}
@media (prefers-reduced-motion: reduce) {
  .animated-count,
  .count-roll-enter-active,
  .count-roll-leave-active {
    transition: none;
  }
}
</style>
