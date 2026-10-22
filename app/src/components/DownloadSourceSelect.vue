<script setup lang="ts">
import GlassSelect from "./GlassSelect.vue";
defineProps<{ modelValue: string; disabled: boolean }>();
defineEmits<{ "update:modelValue": [value: string] }>();
const sources = [
  { value: "official", label: "Minecraft 官方", detail: "仅使用官方来源" },
  { value: "auto", label: "官方优先", detail: "失败时切换至 BMCLAPI" },
  { value: "bmclapi", label: "BMCLAPI 优先", detail: "失败时切换至官方来源" },
];
</script>
<template>
  <GlassSelect
    id="download-source"
    :model-value="modelValue"
    :disabled="disabled"
    :options="sources"
    @update:model-value="$emit('update:modelValue', $event)"
  />
</template>
