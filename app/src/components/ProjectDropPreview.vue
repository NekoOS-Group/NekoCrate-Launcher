<script setup lang="ts">
import { computed, ref } from "vue";
import { X, Plus } from "lucide-vue-next";
import GlassDialog from "./GlassDialog.vue";
import GlassSelect from "./GlassSelect.vue";
import ProjectDetails from "./ProjectDetails.vue";
import type { DroppedProject } from "../projectDrop";
import type { ResourceRpc } from "../projectPresentation";
import type { Instance } from "../types";
import { versionName } from "../versionDescription";
import { t } from "../i18n";
const props = defineProps<{
  project: DroppedProject;
  instances: readonly Instance[];
  rpc: ResourceRpc;
}>();
const emit = defineEmits<{
  close: [];
  link: [url: string];
  select: [instance: Instance, version: string];
}>();
const id = ref(props.instances[0]?.id || ""),
  version = ref("");
const instance = computed(() => props.instances.find((i) => i.id === id.value));
const choices = computed(
  () =>
    instance.value?.versions
      .filter((v) => v.installed)
      .map((v) => ({ value: v.id, label: versionName(v) })) || [],
);
</script>
<template>
  <GlassDialog :label="project.project.name" @close="emit('close')"
    ><section class="modal glass-surface project-drop-panel">
      <header>
        <h2 class="dialog-heading" tabindex="-1">{{ project.project.name }}</h2>
        <button
          class="icon-button"
          :aria-label="t('关闭')"
          @click="emit('close')"
        >
          <X :size="18" />
        </button>
      </header>
      <div class="project-drop-target">
        <GlassSelect
          id="link-instance"
          v-model="id"
          :options="instances.map((i) => ({ value: i.id, label: i.name }))"
          :aria-label="t('游戏目录')"
          compact
          floating
          @update:model-value="version = ''"
        /><GlassSelect
          id="link-version"
          v-model="version"
          :options="choices"
          :aria-label="t('游戏版本')"
          :placeholder="t('选择游戏版本')"
          compact
          floating
        /><button
          class="icon-button"
          :disabled="!instance || !version"
          :title="t('添加资源')"
          @click="instance && emit('select', instance, version)"
        >
          <Plus :size="18" />
        </button>
      </div>
      <ProjectDetails
        :project="project.project"
        :platform="project.platform"
        :rpc="rpc"
        @link="emit('link', $event)"
      /></section
  ></GlassDialog>
</template>
<style scoped>
.project-drop-panel {
  width: min(740px, calc(100vw - 32px));
  max-height: calc(100vh - 48px);
  overflow: auto;
  padding: 24px;
  display: grid;
  gap: 18px;
}
.project-drop-panel header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 20px;
}
.project-drop-target {
  display: flex;
  gap: 14px;
  align-items: center;
}
.dialog-heading {
  font-size: 19px;
  margin: 0;
}
</style>
