<script setup lang="ts">
import { t } from "../i18n";
import type { ModpackStatus } from "../types";
defineProps<{ status: ModpackStatus; disabled: boolean }>();
defineEmits<{ supply: []; resume: []; settings: []; link: [url: string] }>();
</script>
<template>
  <div v-if="!status.completed" class="pack-missing">
    <template v-if="status.needs_key"
      ><p>{{ t("需要配置 CurseForge API Key 才能解析文件下载地址。") }}</p>
      <button
        class="text-button"
        :disabled="disabled"
        @click="$emit('settings')"
      >
        {{ t("网络设置") }}
      </button></template
    >
    <template v-else-if="status.missing.length">
      <p>
        {{ t("这些文件需从作者页面下载，选择下载后的文件即可校验并补齐。") }}
      </p>
      <ul>
        <li v-for="file in status.missing" :key="file.key">
          <button
            class="text-button"
            :disabled="!file.manual_url"
            @click="file.manual_url && $emit('link', file.manual_url)"
          >
            {{ file.path || `CurseForge ${file.key}` }}
          </button>
        </li>
      </ul>
      <button class="text-button" :disabled="disabled" @click="$emit('supply')">
        {{ t("选择已下载文件") }}
      </button>
    </template>
    <button class="text-button" :disabled="disabled" @click="$emit('resume')">
      {{ t("继续安装") }}
    </button>
  </div>
</template>
<style scoped>
.pack-missing {
  font-size: 12px;
  color: var(--muted);
  margin: 6px 0 14px;
}
.pack-missing p {
  line-height: 1.6;
  margin-bottom: 8px;
}
.pack-missing ul {
  list-style: none;
  padding: 0;
  max-height: 160px;
  overflow: auto;
}
.pack-missing li {
  margin-block: 6px;
}
.pack-missing > button + button {
  margin-left: 18px;
}
</style>
