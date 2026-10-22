<script setup lang="ts">
import { t, translateMessage } from "../i18n";
import { computed } from "vue";

const props = defineProps<{
  progress: number;
  indeterminate: boolean;
  title: string;
  message: string;
}>();
const emit = defineEmits<{ open: [] }>();
const percent = computed(() =>
  Number.isFinite(props.progress)
    ? Math.min(100, Math.max(0, props.progress))
    : 0,
);
const outline = "M32 2 58 17V47L32 62 6 47V17Z";
const perimeter = 4 * Math.hypot(26, 15) + 60;
</script>

<template>
  <button
    class="instance-progress-button"
    :class="{ indeterminate }"
    :style="{ '--ring-length': perimeter }"
    :title="translateMessage(message)"
    :aria-label="
      t('{title}，{progress}，查看下载任务', {
        title: t(title),
        progress: indeterminate ? t('正在准备') : `${Math.round(percent)}%`,
      })
    "
    @click="emit('open')"
  >
    <svg
      viewBox="0 0 64 64"
      class="instance-progress"
      role="progressbar"
      :aria-label="t(title)"
      :aria-valuenow="indeterminate ? undefined : percent"
      aria-valuemin="0"
      aria-valuemax="100"
    >
      <path class="instance-progress-hit" :d="outline" />
      <path class="instance-progress-track" :d="outline" />
      <path
        class="instance-progress-fill"
        :d="outline"
        :stroke-dasharray="
          indeterminate
            ? `${perimeter * 0.24} ${perimeter * 0.76}`
            : `${perimeter} ${perimeter}`
        "
        :stroke-dashoffset="indeterminate ? 0 : perimeter * (1 - percent / 100)"
      />
    </svg>
  </button>
</template>
