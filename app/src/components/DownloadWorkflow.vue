<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  Check,
  Circle,
  CircleAlert,
  LoaderCircle,
  ChevronDown,
  X,
  Pause,
  Play,
} from "lucide-vue-next";
import { t } from "../i18n";
import DownloadFiles from "./DownloadFiles.vue";
import type { Task } from "../types";
import { createFileOrder } from "../fileOrder";
const emptyFiles = createFileOrder();
const props = defineProps<{
  task: Task;
  name: string;
  version?: string;
  controlPending?: boolean;
}>();
defineEmits<{ close: []; cancel: []; pause: []; resume: [] }>();
const active = computed(
  () =>
    props.task.workflow.steps.find((step) =>
      ["running", "paused"].includes(step.status),
    )?.id,
);
const expanded = ref<string | null>(null);
watch(
  [() => props.task.id, active],
  () => {
    expanded.value = active.value || null;
  },
  { immediate: true },
);
const labels = {
  pending: "等待中",
  running: "进行中",
  paused: "已暂停",
  done: "已完成",
  error: "失败",
  cancelled: "已取消",
};
</script>
<template>
  <div class="download-workflow">
    <header class="workflow-header">
      <div>
        <h2 class="dialog-heading" :title="name" tabindex="-1" autofocus>
          {{ name }}
        </h2>
        <small v-if="version">{{ version }}</small>
      </div>
      <div class="workflow-header-actions">
        <template
          v-if="['running', 'paused'].includes(task.status) && task.cancellable"
        >
          <button
            v-if="task.version_id"
            type="button"
            class="text-button"
            :disabled="controlPending"
            @click="task.status === 'paused' ? $emit('resume') : $emit('pause')"
          >
            <Play v-if="task.status === 'paused'" :size="14" /><Pause
              v-else
              :size="14"
            />
            {{ t(task.status === "paused" ? "继续下载" : "暂停下载") }}
          </button>
          <button
            type="button"
            class="text-button"
            :disabled="controlPending"
            @click="$emit('cancel')"
          >
            {{ t("取消下载") }}
          </button>
        </template>
        <button
          type="button"
          class="icon-button"
          :aria-label="t('关闭面板')"
          @click="$emit('close')"
        >
          <X :size="16" />
        </button>
      </div>
    </header>
    <ol class="workflow-steps">
      <li
        v-for="step in task.workflow.steps"
        :key="step.id"
        class="workflow-step"
        :data-status="step.status"
      >
        <span class="workflow-node" aria-hidden="true"
          ><Check v-if="step.status === 'done'" :size="15" /><LoaderCircle
            v-else-if="step.status === 'running'"
            class="spin"
            :size="16" /><Pause
            v-else-if="step.status === 'paused'"
            :size="15" /><CircleAlert
            v-else-if="step.status === 'error'"
            :size="16" /><X
            v-else-if="step.status === 'cancelled'"
            :size="14" /><Circle v-else :size="12"
        /></span>
        <div class="workflow-step-body">
          <button
            type="button"
            class="workflow-step-heading"
            :disabled="!step.fileCount"
            :aria-expanded="step.fileCount ? expanded === step.id : undefined"
            @click="expanded = expanded === step.id ? null : step.id"
          >
            <span>{{ t(step.label) }}</span
            ><small
              v-if="
                step.id === 'files' &&
                (step.fileCount || task.download_progress > 0)
              "
              :title="
                t('已校验 {done} / {total} 个文件', {
                  done: step.filesDone,
                  total: step.fileCount,
                })
              "
              >{{
                step.status === "done"
                  ? 100
                  : Math.min(99, Math.floor(task.download_progress))
              }}%</small
            ><small v-else-if="step.fileCount"
              >{{ step.filesDone }} / {{ step.fileCount }}</small
            ><span class="workflow-step-state">{{
              t(
                step.status === "pending" &&
                  !["running", "paused"].includes(task.status)
                  ? "未执行"
                  : labels[step.status],
              )
            }}</span
            ><ChevronDown
              v-if="step.fileCount"
              :size="14"
              :class="{ expanded: expanded === step.id }"
            />
          </button>
          <slot name="step-details" :step="step" />
          <Transition name="network-options"
            ><DownloadFiles
              v-if="expanded === step.id && step.fileCount"
              :key="`${task.id}-${step.id}`"
              :order="task.workflow.fileOrder.get(step.id) || emptyFiles"
              :paused="task.status === 'paused'"
          /></Transition>
        </div>
      </li>
    </ol>
  </div>
</template>
