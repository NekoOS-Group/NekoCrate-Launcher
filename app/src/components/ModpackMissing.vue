<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { FolderOpen, Import, ExternalLink } from "lucide-vue-next";
import { t, translateMessage } from "../i18n";
import type { ModpackStatus } from "../types";
const props = defineProps<{
  status: ModpackStatus;
  disabled: boolean;
  running?: boolean;
  dragging?: boolean;
  error?: string;
}>();
defineEmits<{ supply: []; resume: []; link: [url: string] }>();
const panel = ref<HTMLElement>();
watch(
  () => props.dragging,
  async (dragging) => {
    if (!dragging) return;
    await nextTick();
    panel.value?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    panel.value?.focus({ preventScroll: true });
  },
  { immediate: true },
);
</script>
<template>
  <section
    v-if="!status.completed && status.missing.length"
    ref="panel"
    class="pack-missing"
    data-manual-pack-drop
    tabindex="-1"
    :aria-label="t('需手动导入')"
    :aria-busy="disabled"
  >
    <header>
      <span
        ><Import :size="13" />{{ t("需手动导入")
        }}<small>{{ status.missing.length }}</small></span
      >
      <button class="text-button" :disabled="disabled" @click="$emit('supply')">
        <FolderOpen :size="13" />{{ t("选择文件") }}
      </button>
    </header>
    <ul>
      <li v-for="file in status.missing" :key="file.key">
        <button
          class="text-button"
          :disabled="!file.manual_url"
          :title="file.path || undefined"
          @click="file.manual_url && $emit('link', file.manual_url)"
        >
          {{ file.display_name || file.path || `CurseForge ${file.key}`
          }}<ExternalLink :size="10" />
        </button>
      </li>
    </ul>
    <footer>
      <span>{{
        t(dragging ? "松开以导入并校验" : "拖入文件，或点击名称下载")
      }}</span
      ><small v-if="running">{{ t("其他文件安装中") }}</small>
    </footer>
    <p v-if="error" class="pack-import-error" role="alert">
      {{ translateMessage(error) }}
    </p>
  </section>
</template>
<style scoped>
.pack-missing {
  padding: 9px 11px;
  margin: 7px 0;
  border: 1px solid color-mix(in srgb, var(--blue) 22%, transparent);
  background: color-mix(in srgb, var(--blue) 4%, transparent);
  font-size: 11px;
  color: var(--muted);
  outline: none;
  transition:
    border-color 160ms ease-out,
    box-shadow 160ms ease-out,
    background-color 160ms ease-out;
}
header,
header > span,
footer {
  display: flex;
  align-items: center;
  gap: 7px;
}
header {
  justify-content: space-between;
  color: var(--ink);
}
header small {
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}
.pack-missing .text-button {
  font-size: 11px;
  min-height: 23px;
  padding: 2px 0;
  gap: 5px;
}
ul {
  display: flex;
  flex-wrap: wrap;
  gap: 1px 16px;
  list-style: none;
  margin: 4px 0;
  padding: 0;
  max-height: 112px;
  overflow-y: auto;
}
li {
  min-width: 0;
  max-width: 100%;
}
li button {
  text-align: left;
  overflow-wrap: anywhere;
}
footer {
  flex-wrap: wrap;
  justify-content: space-between;
  font-size: 10px;
  line-height: 1.5;
}
footer small {
  font-size: inherit;
  opacity: 0.75;
}
.pack-import-error {
  margin: 5px 0 0;
  color: var(--danger, #e28282);
  line-height: 1.4;
  overflow-wrap: anywhere;
}
</style>
