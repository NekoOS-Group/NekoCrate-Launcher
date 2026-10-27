<script setup lang="ts">
import { ref, watch } from "vue";
import { ArrowLeft, Archive, X, Download, LoaderCircle } from "lucide-vue-next";
import DownloadDestination from "./DownloadDestination.vue";
import { t } from "../i18n";
import { listRevealStyle } from "../listMotion";
import type { Instance, ModpackPreview } from "../types";
const props = defineProps<{
  preview: ModpackPreview;
  online?: boolean;
  target: Instance | null;
  instances: readonly Instance[];
  name: string;
  icon: number;
  location: string;
  directory: string;
  root: string;
  disabled: boolean;
  picking: boolean;
  canSubmit: boolean;
}>();
const emit = defineEmits<{
  target: [id: string | null];
  "update:name": [value: string];
  "update:icon": [value: number];
  "update:location": [value: string];
  browse: [];
  submit: [name: string, optional: string[]];
  choose: [];
  close: [];
  settings: [];
  back: [];
}>();
const destination = ref<InstanceType<typeof DownloadDestination>>();
const packName = ref("");
const optional = ref<string[]>([]);
watch(
  () => props.preview,
  () => {
    packName.value = props.preview.pack.name.slice(0, 80);
    optional.value = props.preview.pack.files
      .filter((f) => f.optional)
      .map((f) => f.key);
  },
  { immediate: true },
);
function submit() {
  if (destination.value?.finishRename() && packName.value.trim())
    emit("submit", packName.value.trim(), optional.value);
}
</script>
<template>
  <section
    class="modal glass-surface pack-import"
    :class="{ 'online-pack': online }"
  >
    <header class="modal-header">
      <h2 class="dialog-heading" tabindex="-1" autofocus>
        {{ t(online ? "安装整合包" : "导入整合包") }}
      </h2>
      <DownloadDestination
        ref="destination"
        :target="target"
        :instances="instances"
        :name="name"
        :icon="icon"
        :location="location"
        :directory="directory"
        :root="root"
        :disabled="disabled"
        :picking="picking"
        @target="$emit('target', $event)"
        @update:name="$emit('update:name', $event)"
        @update:icon="$emit('update:icon', $event)"
        @update:location="$emit('update:location', $event)"
        @browse="$emit('browse')"
      />
      <button
        class="icon-button"
        :disabled="disabled"
        :aria-label="t('关闭对话框')"
        @click="$emit('close')"
      >
        <X :size="16" />
      </button>
    </header>
    <form @submit.prevent="submit">
      <div class="pack-body">
        <div class="pack-summary list-reveal-item" :style="listRevealStyle(0)">
          <Archive :size="30" />
          <div>
            <input
              v-model="packName"
              required
              maxlength="80"
              :disabled="disabled"
              :aria-label="t('版本名称')"
            /><small
              >{{ preview.pack.minecraft }} ·
              {{
                preview.pack.loader
                  ? `${preview.pack.loader.kind} ${preview.pack.loader.version}`
                  : t("原版")
              }}</small
            >
          </div>
          <button
            v-if="!online"
            type="button"
            class="text-button"
            :disabled="disabled"
            @click="$emit('choose')"
          >
            {{ t("更换文件") }}
          </button>
        </div>
        <p
          class="pack-description list-reveal-item"
          :style="listRevealStyle(1)"
        >
          {{ preview.pack.format === "modrinth" ? "Modrinth" : "CurseForge" }} ·
          {{ preview.pack.version }} ·
          {{ t("{count} 个文件", { count: preview.pack.files.length }) }}
        </p>
        <p class="pack-description">
          {{ t("在所选游戏目录内新建独立版本，隔离 Mods、配置和存档。") }}
        </p>
        <div v-if="!preview.curseforge_key_configured" class="pack-key-hint">
          <span>{{
            t("需要配置 CurseForge API Key 才能解析文件下载地址。")
          }}</span
          ><button type="button" class="text-button" @click="$emit('settings')">
            {{ t("网络设置") }}
          </button>
        </div>
        <fieldset
          v-if="preview.pack.files.some((f) => f.optional)"
          class="pack-optional"
          :disabled="disabled"
        >
          <legend>{{ t("可选文件") }}</legend>
          <label
            v-for="file in preview.pack.files.filter((f) => f.optional)"
            :key="file.key"
            ><input
              v-model="optional"
              type="checkbox"
              :value="file.key"
            /><span>{{ file.path || `CurseForge ${file.key}` }}</span></label
          >
        </fieldset>
      </div>
      <footer class="modal-actions">
        <button
          type="button"
          class="button"
          :disabled="disabled"
          @click="online ? $emit('back') : $emit('close')"
        >
          <ArrowLeft v-if="online" :size="15" />{{
            t(online ? "返回" : "取消")
          }}</button
        ><button
          class="button primary"
          :disabled="!canSubmit || !packName.trim()"
        >
          <LoaderCircle v-if="disabled" class="spin" :size="16" /><Download
            v-else
            :size="16"
          />{{
            t(
              preview.curseforge_key_configured
                ? online
                  ? "下载并安装"
                  : "导入并安装"
                : "添加并稍后安装",
            )
          }}
        </button>
      </footer>
    </form>
  </section>
</template>
<style scoped>
.pack-import {
  width: min(760px, calc(100vw - 40px));
}
.online-pack {
  width: min(840px, calc(100vw - 64px));
  height: min(590px, calc(100dvh - 64px));
}
.online-pack form {
  flex: 1;
}
.online-pack .pack-body {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overflow-x: hidden;
}
.pack-body {
  padding: 0 var(--panel-inset) 16px;
}
.pack-summary {
  display: flex;
  align-items: center;
  gap: 14px;
}
.pack-summary > svg {
  color: var(--blue);
  flex-shrink: 0;
}
.pack-summary > div {
  flex: 1;
  min-width: 0;
}
.pack-summary input {
  width: 100%;
  border: 0;
  padding: 0;
  background: transparent;
  font-size: 16px;
}
.pack-summary small {
  display: block;
  color: var(--muted);
  font: 11px/20px var(--mono);
}
.pack-description,
.pack-key-hint {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.7;
  margin-top: 14px;
}
.pack-key-hint {
  display: flex;
  gap: 16px;
  align-items: center;
}
.pack-optional {
  border: 0;
  padding: 0;
  max-height: 220px;
  overflow: auto;
  margin-top: 18px;
}
.pack-optional legend {
  color: var(--muted);
  font-size: 11px;
  margin-bottom: 8px;
}
.pack-optional label {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  padding-block: 5px;
}
.pack-optional span {
  overflow-wrap: anywhere;
}
</style>
