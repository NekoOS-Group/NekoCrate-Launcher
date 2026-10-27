<script setup lang="ts">
import { leaveList, resetListLeave } from "../listMotion";
defineProps<{ contentKey: string }>();
</script>
<template>
  <div class="content-switch">
    <Transition
      name="content-switch"
      @before-enter="resetListLeave"
      @leave="leaveList"
      @leave-cancelled="resetListLeave"
    >
      <div :key="contentKey" class="content-switch-page"><slot /></div>
    </Transition>
  </div>
</template>
<style scoped>
.content-switch {
  display: grid;
  grid-template-rows: minmax(0, 1fr);
  min-width: 0;
  min-height: 0;
  flex: 1;
  overflow: hidden;
}
.content-switch-page {
  grid-area: 1 / 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}
.content-switch-enter-active:not(:has(.list-reveal-item)) {
  transition: opacity var(--motion-fade) var(--motion-ease);
}
.content-switch-enter-from:not(:has(.list-reveal-item)) {
  opacity: 0;
}
.content-switch-leave-active {
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce) {
  .content-switch-enter-active {
    transition: none;
  }
}
</style>
