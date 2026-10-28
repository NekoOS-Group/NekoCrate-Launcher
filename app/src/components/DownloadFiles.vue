<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, CircleAlert, RotateCcw } from "lucide-vue-next";
import { t } from "../i18n";
import { fileRowHeight, filePercent, formatBytes } from "../downloadWorkflow";
import { indexedFileWindow, type FileOrder } from "../fileOrder";
const props = defineProps<{
  order: FileOrder;
  paused?: boolean;
  retryEnabled?: boolean;
  retrying?: string;
}>();
defineEmits<{ retry: [file: string] }>();
const scrollTop = ref(0);
const visible = computed(() => indexedFileWindow(props.order, scrollTop.value));
const labels = {
  queued: "等待中",
  downloading: "下载中",
  verifying: "校验中",
  reusing: "复用中",
  installing: "安装中",
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
        height: `${order.files.length * fileRowHeight}px`,
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
          :aria-setsize="order.files.length"
          :data-status="file.status"
        >
          <div class="workflow-file-copy">
            <span class="workflow-file-name" :title="file.error || file.id">{{
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
                  ? t("已复用")
                  : paused &&
                      [
                        "downloading",
                        "verifying",
                        "reusing",
                        "installing",
                      ].includes(file.status)
                    ? t("已暂停")
                    : file.status === "downloading" && file.total
                      ? `${t("下载中")} ${filePercent(file)}%`
                      : t(labels[file.status])
              }}</span
            >
            <button
              v-if="file.status === 'error' && file.retryable"
              type="button"
              class="file-retry icon-button"
              :disabled="!retryEnabled || !!retrying"
              :aria-label="`${t('重试此文件')} · ${file.name}`"
              :title="file.error || t('重试此文件')"
              @click="$emit('retry', file.id)"
            >
              <RotateCcw :size="12" />
            </button>
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

<style scoped>
.file-retry {
  padding: 3px;
  width: 22px;
  height: 22px;
  flex: 0 0 auto;
}
</style>
