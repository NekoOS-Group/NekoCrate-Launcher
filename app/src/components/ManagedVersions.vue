<script setup lang="ts">
import { ref } from "vue";
import VersionManager from "./VersionManager.vue";
import { createVersionManagerActions } from "../versionManagerActions";
import type { createVersionContentLoader } from "../versionContentLoader";
import type { ContentKind, Instance, Task } from "../types";
import type { ModActions } from "../modManagement";
import type { ResourceRpc } from "../projectPresentation";
const props = defineProps<{
  instance: Instance;
  instances: Instance[];
  initialVersion?: string;
  initialTab?: ContentKind;
  selectionRevision?: number;
  rpc: ResourceRpc;
  busy: boolean;
  running: string[];
  current: Task | null;
  modActions: ModActions;
  sync: () => Promise<void>;
  contentLoader: ReturnType<typeof createVersionContentLoader>;
}>();
const emit = defineEmits<{
  close: [];
  error: [error: unknown];
  busy: [busy: boolean];
  link: [url: string];
  "select-instance": [id: string];
  "add-resources": [version: string, kind: string];
}>();
const manager = ref<InstanceType<typeof VersionManager>>();
const actions = createVersionManagerActions(
  () => props.instance.id,
  props.rpc,
  props.contentLoader,
  props.sync,
);
defineExpose({ refresh: () => manager.value?.refresh() });
</script>
<template>
  <VersionManager
    ref="manager"
    :rpc="rpc"
    :instance="instance"
    :instances="instances"
    :initial-version="initialVersion"
    :initial-tab="initialTab"
    :selection-revision="selectionRevision"
    :locked="busy || running.includes(instance.id)"
    :downloading="
      busy &&
      current?.instance_id === instance.id &&
      current?.status === 'running'
    "
    :download-version="
      current?.version_id ||
      (current?.operation === 'download_declared_mod'
        ? String(current.arguments?.version || '')
        : undefined)
    "
    :mod-actions="modActions"
    :load-content="actions.readVersionContent"
    :prefetch-content="actions.prefetchVersionContent"
    :load-counts="actions.readVersionContentCounts"
    :load-metadata="actions.readVersionMetadata"
    :load-content-metadata="actions.readContentMetadata"
    :open-content-folder="actions.openContentFolder"
    :load-content-source="actions.readContentSource"
    :rename="actions.renameVersion"
    :remove="actions.deleteVersion"
    @close="emit('close')"
    @error="emit('error', $event)"
    @busy="emit('busy', $event)"
    @link="emit('link', $event)"
    @select-instance="emit('select-instance', $event)"
    @add-resources="(version, kind) => emit('add-resources', version, kind)"
  />
</template>
