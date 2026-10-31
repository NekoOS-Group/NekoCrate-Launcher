<script setup lang="ts">
import { Check, Power, PowerOff, X } from "lucide-vue-next";
import { t } from "../i18n";
defineProps<{
  count: number;
  busy?: boolean;
  mods?: boolean;
  enable?: boolean;
  disable?: boolean;
}>();
defineEmits<{ clear: []; enabled: [value: boolean] }>();
</script>
<template>
  <Transition name="resource-selection">
    <div
      v-if="count"
      class="resource-selection-bar"
      role="toolbar"
      :aria-label="t('批量操作')"
      :aria-busy="busy"
    >
      <span class="resource-selection-count"
        ><Check :size="13" />{{ t("已选 {count} 项", { count }) }}</span
      >
      <button
        v-if="mods"
        type="button"
        class="icon-button"
        :disabled="busy || !enable"
        :title="t('启用所选 Mods')"
        :aria-label="t('启用所选 Mods')"
        @click="$emit('enabled', true)"
      >
        <Power :size="15" />
      </button>
      <button
        v-if="mods"
        type="button"
        class="icon-button"
        :disabled="busy || !disable"
        :title="t('禁用所选 Mods')"
        :aria-label="t('禁用所选 Mods')"
        @click="$emit('enabled', false)"
      >
        <PowerOff :size="15" />
      </button>
      <button
        type="button"
        class="icon-button"
        :disabled="busy"
        :title="t('取消选择')"
        :aria-label="t('取消选择')"
        @click="$emit('clear')"
      >
        <X :size="14" />
      </button>
    </div>
  </Transition>
</template>
<style>
.resource-selection-bar {
  display: flex;
  align-items: center;
  gap: 5px;
  color: var(--blue);
  font-size: 11px;
  flex-shrink: 0;
}
.resource-selection-count {
  display: flex;
  align-items: center;
  gap: 5px;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}
.resource-selection-bar .icon-button {
  width: 26px;
  height: 26px;
}
.resource-selection-enter-active,
.resource-selection-leave-active {
  transition:
    opacity 140ms ease,
    transform 180ms cubic-bezier(0.2, 0.8, 0.2, 1);
}
.resource-selection-enter-from,
.resource-selection-leave-to {
  opacity: 0;
  transform: translateY(5px);
}
@media (prefers-reduced-motion: reduce) {
  .resource-selection-enter-active,
  .resource-selection-leave-active {
    transition: none;
  }
}
</style>
