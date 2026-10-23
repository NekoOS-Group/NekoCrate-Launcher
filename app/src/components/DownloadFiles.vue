<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, CircleAlert } from "lucide-vue-next";
import { t } from "../i18n";
import {
  fileWindow,
  fileRowHeight,
  filePercent,
  formatBytes,
  activeFilesFirst,
} from "../downloadWorkflow";
import type { DownloadFile } from "../types";
const props = defineProps<{ files: DownloadFile[]; paused?: boolean }>();
const scrollTop = ref(0);
// Track only membership/status here; byte updates do not reorder the list.
const ordered = computed(() => activeFilesFirst(props.files));
const visible = computed(() => fileWindow(ordered.value, scrollTop.value));
const labels = {
  queued: "等待中",
  downloading: "下载中",
  verifying: "校验中",
  done: "已完成",
  error: "失败",
  cancelled: "已取消",
};
</script>
<template>
  <div
    class="workflow-files"
    role="list"
    :aria-label="t('文件下载进度')"
    tabindex="0"
    :style="{ height: `${visible.height}px` }"
    @scroll="scrollTop = ($event.target as HTMLElement).scrollTop"
  >
    <div
      :style="{
        height: `${files.length * fileRowHeight}px`,
        position: 'relative',
      }"
    >
      <div
        class="workflow-file-window"
        :style="{ transform: `translateY(${visible.start * fileRowHeight}px)` }"
      >
        <div
          v-for="(file, index) in visible.items"
          :key="file.id"
          class="workflow-file"
          role="listitem"
          :aria-posinset="visible.start + index + 1"
          :aria-setsize="files.length"
          :data-status="file.status"
        >
          <div class="workflow-file-copy">
            <span class="workflow-file-name" :title="file.id">{{
              file.name
            }}</span>
            <span class="workflow-file-amount"
              >{{ formatBytes(file.downloaded)
              }}<template v-if="file.total">
                / {{ formatBytes(file.total) }}</template
              ></span
            >
            <span class="workflow-file-state"
              ><Check v-if="file.status === 'done'" :size="12" /><CircleAlert
                v-else-if="file.status === 'error'"
                :size="12"
              />{{
                file.status === "done" && file.cached
                  ? t("已缓存")
                  : paused && ["downloading", "verifying"].includes(file.status)
                    ? t("已暂停")
                    : file.status === "downloading" && file.total
                      ? `${filePercent(file)}%`
                      : t(labels[file.status])
              }}</span
            >
          </div>
          <div
            class="workflow-file-track"
            role="progressbar"
            :aria-label="file.name"
            :aria-valuenow="
              file.total || file.status === 'done'
                ? filePercent(file)
                : undefined
            "
            aria-valuemin="0"
            aria-valuemax="100"
          >
            <i :style="{ width: `${filePercent(file)}%` }" />
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
