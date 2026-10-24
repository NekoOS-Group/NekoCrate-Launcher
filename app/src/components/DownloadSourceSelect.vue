<script setup lang="ts">
import GlassSelect from "./GlassSelect.vue";
import { computed } from "vue";
const props = defineProps<{
  modelValue: string;
  kind: "game" | "mod";
  disabled?: boolean;
}>();
defineEmits<{ "update:modelValue": [value: string] }>();
const sources = computed(() =>
  props.kind === "game"
    ? [
        { value: "auto", label: "自动选择" },
        { value: "official", label: "Minecraft 官方" },
        { value: "bmclapi", label: "BMCLAPI" },
      ]
    : [
        { value: "auto", label: "自动选择" },
        { value: "modrinth", label: "Modrinth" },
        { value: "curseforge", label: "CurseForge" },
        { value: "bbsmc", label: "BBSMC" },
      ],
);
</script>
<template>
  <GlassSelect
    floating
    compact
    :id="`${kind}-download-source`"
    :model-value="modelValue"
    :disabled="disabled"
    :options="sources"
    @update:model-value="$emit('update:modelValue', $event)"
  />
</template>
