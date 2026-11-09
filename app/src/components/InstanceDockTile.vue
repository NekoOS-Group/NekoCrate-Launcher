<script setup lang="ts">
import { computed } from "vue";
import InstanceIcon from "./InstanceIcon.vue";
import InstanceProgress from "./InstanceProgress.vue";
import { resolveInstanceIcon } from "../instanceIcons";
import { versionName } from "../versionDescription";
import { t } from "../i18n";
import type { Instance } from "../types";

const props = defineProps<{
  instance: Instance;
  selected: boolean;
  running: boolean;
  task: boolean;
  dragging: boolean;
  tabIndex?: number;
  progress?: {
    title: string;
    message: string;
    progress: number;
    status: string;
    indeterminate: boolean;
  } | null;
}>();
const emit = defineEmits<{
  select: [];
  open: [event?: MouseEvent];
  edit: [event: MouseEvent | KeyboardEvent];
  start: [event: PointerEvent];
  sort: [event: KeyboardEvent];
  preview: [event: MouseEvent | FocusEvent];
  leave: [];
}>();
const label = computed(() =>
  t(
    props.running
      ? "{name}，{version}，双击查看运行日志，按 F2 设置，拖动或按 Alt 加方向键排序"
      : props.task
        ? "{name}，{version}，双击查看下载进度，按 F2 设置，拖动或按 Alt 加方向键排序"
        : "{name}，{version}，双击或按 F2 设置，拖动或按 Alt 加方向键排序",
    {
      name: props.instance.name,
      version:
        versionName(
          props.instance.versions.find(
            (v) => v.id === props.instance.selected_version,
          ),
        ) || t("尚无版本"),
    },
  ),
);
</script>

<template>
  <div
    class="instance-item"
    :data-instance-id="instance.id"
    :data-resource-instance="instance.id"
    data-drop-kind="instance"
    :data-resource-version="instance.selected_version || undefined"
    :class="{ dragging }"
  >
    <button
      class="instance-tile"
      :class="{ selected }"
      :aria-label="label"
      :aria-pressed="selected"
      :tabindex="tabIndex"
      aria-keyshortcuts="Alt+ArrowLeft Alt+ArrowRight Alt+ArrowUp Alt+ArrowDown F2"
      @pointerdown="emit('start', $event)"
      @keydown="emit('sort', $event)"
      @click="emit('select')"
      @dblclick="emit('open', $event)"
      @contextmenu.prevent="emit('edit', $event)"
      @keydown.f2.prevent="emit('edit', $event)"
      @mouseenter="emit('preview', $event)"
      @mouseleave="emit('leave')"
      @focus="emit('preview', $event)"
      @blur="emit('leave')"
    >
      <InstanceIcon :icon="resolveInstanceIcon(instance.icon, instance.id)" />
      <i v-if="running" class="running-dot" />
    </button>
    <InstanceProgress
      v-if="progress"
      :title="progress.title"
      :message="progress.message"
      :progress="progress.progress"
      :indeterminate="progress.status === 'running' && progress.indeterminate"
      @open="emit('open')"
    />
  </div>
</template>
