<script setup lang="ts">
import { ref, computed } from "vue";
import { useHorizontalLayout } from "../useHorizontalLayout";
import InstanceTarget from "./InstanceTarget.vue";
import InstanceHeader from "./InstanceHeader.vue";
import { t } from "../i18n";
import type { Instance } from "../types";
const props = defineProps<{
  target: Instance | null;
  instances: readonly Instance[];
  name: string;
  icon: number;
  location: string;
  directory: string;
  root: string;
  disabled: boolean;
  picking: boolean;
}>();
defineEmits<{
  target: [id: string | null];
  "update:name": [value: string];
  "update:icon": [value: number];
  "update:location": [value: string];
  browse: [];
}>();
const container = ref<HTMLElement>();
useHorizontalLayout(container);
const heading = ref<InstanceType<typeof InstanceHeader>>();
const destination = computed(() =>
  props.location === "external"
    ? props.directory || t("选择外部文件夹")
    : t("启动器数据目录"),
);
defineExpose({ finishRename: () => heading.value?.finishRename() ?? true });
</script>
<template>
  <div ref="container" class="download-destination">
    <InstanceTarget
      v-if="target"
      :target="target"
      :instances="instances"
      :disabled="disabled || picking"
      @select="$emit('target', $event)"
    />
    <template v-else>
      <span class="destination-prefix" data-slide-key="instance-prefix">{{
        t("到")
      }}</span>
      <InstanceHeader
        ref="heading"
        :name="name"
        :icon="icon"
        :directory="destination"
        :directory-loading="false"
        :disabled="disabled"
        :directory-action-label="t('选择存放位置')"
        @update:name="$emit('update:name', $event)"
        @update:icon="$emit('update:icon', $event)"
        @open-directory="$emit('browse')"
      />
      <InstanceTarget
        :target="null"
        :instances="instances"
        :disabled="disabled || picking"
        icon-only
        @select="$emit('target', $event)"
      />
    </template>
  </div>
</template>
<style scoped>
.download-destination {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  height: 36px;
  max-width: 54%;
  margin-left: auto;
}
.download-destination :deep(.instance-target) {
  max-width: 100%;
  min-width: 0;
  flex-shrink: 1;
}
.destination-prefix {
  color: var(--muted);
  font-size: 12px;
}
.download-destination :deep(.settings-heading) {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: 26px minmax(0, 1fr);
  gap: 12px;
}
.download-destination :deep(.instance-heading-icon),
.download-destination :deep(.instance-heading-art) {
  width: 26px;
  height: 26px;
}
.download-destination :deep(.instance-heading-icon .instance-art) {
  width: 26px;
  height: 26px;
}
.download-destination :deep(.settings-heading-details) {
  width: clamp(96px, 15vw, 150px);
  min-width: 0;
  height: 36px;
}
.download-destination :deep(h2),
.download-destination :deep(.instance-name-button),
.download-destination :deep(.instance-name-input) {
  height: 20px;
  font-size: 13px;
  line-height: 20px;
}
.download-destination :deep(.settings-directory-link) {
  height: 16px;
  font-size: 10px;
  margin-top: 0;
  gap: 5px;
}
.download-destination :deep(.settings-directory-link span) {
  font-size: 10px;
  line-height: 16px;
}
</style>
