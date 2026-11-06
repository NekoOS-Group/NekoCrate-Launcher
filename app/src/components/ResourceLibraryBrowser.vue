<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef } from "vue";
import { listen } from "@tauri-apps/api/event";
import { X } from "lucide-vue-next";
import GlassSelect from "./GlassSelect.vue";
import ResourceBrowser from "./ResourceBrowser.vue";
import { formatBytes } from "../downloadWorkflow";
import { t } from "../i18n";
import type { LauncherEvent, VersionManifest } from "../types";
import type { ResourceKind, ResourceCatalogScope } from "../resources";
const props = defineProps<{
  rpc: <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
  manifest: VersionManifest | null;
  kind: ResourceKind;
  busy: boolean;
  embedded?: boolean;
}>();
const emit = defineEmits<{
  back: [];
  error: [error: unknown];
  link: [url: string];
  busy: [value: boolean];
  downloaded: [kind: ResourceKind];
}>();
const catalog = shallowRef(props.manifest);
const game = ref(props.manifest?.latest.release || "");
const loader = ref("fabric");
const active = ref(false);
const downloading = ref(false);
const progress = shallowRef<LauncherEvent>();
const status = computed(() => {
  const e = progress.value;
  if (!e) return t("下载到资源库");
  if (e.total_bytes)
    return `${formatBytes(e.bytes || 0)} / ${formatBytes(e.total_bytes)}`;
  return e.message || t("下载到资源库");
});
const fraction = computed(() =>
  progress.value?.total_bytes
    ? Math.min(1, (progress.value.bytes || 0) / progress.value.total_bytes)
    : 0,
);
let disposed = false;
let stop: (() => void) | undefined;
onBeforeUnmount(() => {
  disposed = true;
  stop?.();
});
if (!catalog.value)
  void props
    .rpc<VersionManifest>("versions")
    .then((value) => {
      if (disposed) return;
      catalog.value = value;
      game.value = value.latest.release || value.versions[0]?.id || "";
    })
    .catch((error) => {
      if (!disposed) emit("error", error);
    });
async function download(
  scope: ResourceCatalogScope,
  project: string,
  version: string,
) {
  if (active.value || props.busy) return false;
  active.value = true;
  downloading.value = true;
  progress.value = undefined;
  emit("busy", true);
  try {
    stop = await listen<LauncherEvent>("resource-library-progress", (event) => {
      if (event.payload.kind === "progress") progress.value = event.payload;
    });
    if (disposed) {
      stop();
      return false;
    }
    await props.rpc("resource_manager", {
      action: { kind: "download_cached_resource", scope, project, version },
    });
    emit("downloaded", scope.kind);
    return true;
  } catch (error) {
    emit("error", error);
    return false;
  } finally {
    stop?.();
    stop = undefined;
    active.value = false;
    downloading.value = false;
    emit("busy", false);
  }
}
async function importResources(inputs: { path: string; kind: ResourceKind }[]) {
  if (active.value || props.busy) return false;
  active.value = true;
  progress.value = undefined;
  emit("busy", true);
  try {
    await props.rpc("resource_manager", {
      action: { kind: "cache_resources", inputs },
    });
    for (const kind of new Set(inputs.map((input) => input.kind)))
      emit("downloaded", kind);
    return true;
  } catch (error) {
    emit("error", error);
    return false;
  } finally {
    active.value = false;
    emit("busy", false);
  }
}
async function cancel() {
  try {
    await props.rpc("cancel");
  } catch (error) {
    emit("error", error);
  }
}
</script>

<template>
  <ResourceBrowser
    class="library-browser"
    :embedded="embedded"
    :library="{
      minecraft: game,
      loader: loader ? { kind: loader, version: '' } : null,
      kind,
    }"
    :busy="active || busy"
    :rpc="rpc"
    :download="download"
    :import-resources="importResources"
    @close="emit('back')"
    @error="emit('error', $event)"
    @link="emit('link', $event)"
  >
    <template #destination>
      <div class="library-filters">
        <GlassSelect
          id="library-game"
          v-model="game"
          :options="
            catalog?.versions.map((v) => ({ value: v.id, label: v.id })) || []
          "
          :disabled="active || busy"
          :aria-label="t('游戏版本')"
          :placeholder="t('选择版本')"
          compact
          floating
          searchable
          literal-labels
        />
        <GlassSelect
          id="library-loader"
          v-model="loader"
          :options="[
            { value: '', label: t('原版') },
            { value: 'fabric', label: 'Fabric' },
            { value: 'forge', label: 'Forge' },
            { value: 'neoforge', label: 'NeoForge' },
            { value: 'quilt', label: 'Quilt' },
          ]"
          :disabled="active || busy"
          :aria-label="t('加载器')"
          compact
          floating
          literal-labels
        />
      </div>
    </template>
    <template #progress>
      <div v-if="downloading" class="library-progress" role="status">
        <progress :value="fraction" max="1" :aria-label="t('下载到资源库')" />
        <small>{{ status }}</small>
        <button
          class="icon-button"
          :aria-label="t('取消')"
          :title="t('取消')"
          @click="cancel"
        >
          <X :size="15" />
        </button>
      </div>
    </template>
  </ResourceBrowser>
</template>
<style scoped>
.library-filters {
  display: flex;
  align-items: center;
  gap: 14px;
  margin-left: auto;
  color: var(--muted);
}
.library-filters :deep(.source-select) {
  width: 120px;
}
.library-progress {
  display: flex;
  align-items: center;
  gap: 12px;
  color: var(--muted);
}
.library-progress progress {
  flex: 1;
  height: 3px;
  accent-color: var(--blue);
}
.library-progress small {
  font-variant-numeric: tabular-nums;
}
</style>
