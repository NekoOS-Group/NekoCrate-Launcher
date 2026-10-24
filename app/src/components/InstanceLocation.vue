<script setup lang="ts">
import { computed } from "vue";
import { FolderOpen, LoaderCircle, ChevronRight } from "lucide-vue-next";
import GlassSelect from "./GlassSelect.vue";
import { t } from "../i18n";

const props = defineProps<{
  modelValue: string;
  directory: string;
  root: string;
  disabled: boolean;
  picking: boolean;
}>();
defineEmits<{ "update:modelValue": [value: string]; browse: [] }>();
const external = computed(() => props.modelValue === "external");
const destination = computed(() => {
  const path = external.value ? props.directory : props.root;
  if (!path) return "";
  const parent = path.replace(/[\\/]+$/, "");
  if (
    external.value &&
    parent.split(/[\\/]/).at(-1)?.toLowerCase() === ".minecraft"
  )
    return parent;
  const separator = path.includes("\\") ? "\\" : "/";
  return `${parent}${separator}${external.value ? ".minecraft" : "instances"}`;
});
</script>

<template>
  <div class="field instance-location">
    <div class="instance-location-heading">
      <label id="instance-location-label" for="instance-location-trigger">{{
        t("存放位置")
      }}</label>
      <GlassSelect
        id="instance-location"
        :model-value="modelValue"
        :disabled="disabled"
        :options="[
          { value: 'managed', label: '启动器数据目录' },
          { value: 'external', label: '外部文件夹' },
        ]"
        floating
        compact
        @update:model-value="$emit('update:modelValue', $event)"
      />
    </div>
    <button
      v-if="external"
      type="button"
      class="instance-location-path"
      :disabled="disabled"
      :title="destination || undefined"
      :aria-label="t('选择外部文件夹')"
      @click="$emit('browse')"
    >
      <LoaderCircle v-if="picking" :size="16" class="spin" />
      <FolderOpen v-else :size="16" />
      <span>{{ destination || t("选择文件夹…") }}</span>
      <ChevronRight :size="14" />
    </button>
    <div
      v-else
      class="instance-location-path"
      :title="destination || undefined"
    >
      <FolderOpen :size="16" />
      <span>{{ destination || t("自动创建独立目录") }}</span>
    </div>
  </div>
</template>

<style scoped>
.instance-location {
  gap: 6px;
  margin-bottom: 4px;
}
.instance-location-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-height: 36px;
}
.instance-location-heading .source-select {
  width: 210px;
  max-width: 65%;
}
.instance-location-path {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-width: 0;
  height: 34px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--muted);
  text-align: left;
  transition: color 180ms var(--motion-ease);
}
.instance-location-path > svg {
  flex-shrink: 0;
}
.instance-location-path > span {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font: 11px/20px var(--mono);
}
button.instance-location-path:hover:not(:disabled),
button.instance-location-path:focus-visible {
  color: var(--blue);
}
</style>
