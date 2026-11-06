<script setup lang="ts">
import { inject } from "vue";
import { Maximize2, Minimize2 } from "lucide-vue-next";
import { windowControlsKey } from "../toolWindows";
import { t } from "../i18n";
const controls = inject(windowControlsKey, undefined);
</script>
<template>
  <button
    v-if="controls"
    type="button"
    class="icon-button window-maximize"
    :aria-label="t(controls.expanded.value ? '还原窗口' : '最大化窗口')"
    :title="t(controls.expanded.value ? '还原窗口' : '最大化窗口')"
    @click="controls.toggle"
  >
    <Minimize2 v-if="controls.expanded.value" :size="15" /><Maximize2
      v-else
      :size="15"
    />
  </button>
</template>
