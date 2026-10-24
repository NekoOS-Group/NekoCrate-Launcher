<script setup lang="ts">
import { CircleAlert, Info, X } from "lucide-vue-next";
import { activeDialog } from "../dialogSurface";
import { t, translateMessage } from "../i18n";
defineProps<{
  message: string;
  retry?: boolean;
  retrying?: boolean;
  informational?: boolean;
}>();
defineEmits<{ dismiss: []; retry: [] }>();
</script>
<template>
  <Teleport :to="activeDialog || 'body'" :disabled="!activeDialog">
    <Transition name="error-notice">
      <div
        v-if="message"
        class="error-notice glass-surface"
        :class="{ 'info-notice': informational }"
        :role="informational ? 'status' : 'alert'"
      >
        <Info v-if="informational" :size="17" /><CircleAlert
          v-else
          :size="17"
        />
        <p>{{ translateMessage(message) }}</p>
        <button
          v-if="retry"
          type="button"
          class="text-button"
          :disabled="retrying"
          @click="$emit('retry')"
        >
          {{ t("重试") }}
        </button>
        <button
          type="button"
          class="icon-button"
          :aria-label="t(informational ? '关闭提示' : '关闭错误提示')"
          @click="$emit('dismiss')"
        >
          <X :size="15" />
        </button>
      </div>
    </Transition>
  </Teleport>
</template>
