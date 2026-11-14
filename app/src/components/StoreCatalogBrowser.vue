<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import {
  ArrowLeft,
  Check,
  Copy,
  Download,
  ExternalLink,
  LoaderCircle,
  Map,
  Plug,
  Plus,
  Search,
  Server,
  X,
} from "lucide-vue-next";
import { save } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { t } from "../i18n";
import { categoryLabel } from "../catalogFilters";
import { useCatalogFeed } from "../useCatalogFeed";
import { listRevealStyle } from "../listMotion";
import {
  dashboard,
  normalizeServerAddress,
  saveDashboard,
} from "../homeDashboard";
import {
  storeFilename,
  storeScope,
  type StoreKind,
  type StoreServer,
  type StoreVersion,
} from "../storeCatalog";
import type { ResourcePlatform, ResourceProject } from "../resources";
import type { ResourceRpc } from "../projectPresentation";
import type { LauncherEvent, VersionManifest } from "../types";
import { formatBytes } from "../downloadWorkflow";
import CatalogTail from "./CatalogTail.vue";
import CatalogProjectRow from "./CatalogProjectRow.vue";
import ContentSwitch from "./ContentSwitch.vue";
import GlassSelect from "./GlassSelect.vue";
import ProjectImage from "./ProjectImage.vue";
import ProjectDetails from "./ProjectDetails.vue";
const props = defineProps<{
  kind: StoreKind;
  rpc: ResourceRpc;
  manifest: VersionManifest | null;
  busy: boolean;
}>();
const emit = defineEmits<{
  error: [error: unknown];
  link: [url: string];
  busy: [value: boolean];
  downloaded: [];
}>();
const query = ref(""),
  platform = ref<ResourcePlatform>("modrinth"),
  game = ref(""),
  loader = ref(""),
  category = ref("");
const scope = computed(() =>
  storeScope(
    props.kind,
    platform.value,
    game.value,
    loader.value,
    category.value,
  ),
);
const categories = shallowRef<{ id: string; name: string }[]>([]);
const categoriesLoading = ref(false),
  categoriesFailed = ref(false);
const categoryOptions = computed(() => [
  { value: "", label: t("全部标签") },
  ...categories.value.map((tag) => ({
    value: tag.id,
    label: categoryLabel(tag.name),
    keywords: tag.name,
  })),
]);
const project = shallowRef<ResourceProject>();
const versions = shallowRef<StoreVersion[]>([]),
  server = shallowRef<StoreServer>();
const chosen = ref(""),
  loading = ref(false),
  active = ref(false),
  complete = ref(false),
  copied = ref(false);
const progress = shallowRef<LauncherEvent>();
const icon = computed(() =>
  props.kind === "plugins" ? Plug : props.kind === "maps" ? Map : Server,
);
const blocked = computed(() => active.value || props.busy);
const selected = computed(() =>
  versions.value.find((v) => v.id === chosen.value),
);
const filename = computed(
  () => selected.value && storeFilename(selected.value),
);
const address = computed(() =>
  normalizeServerAddress(server.value?.address || ""),
);
const savedServer = computed(
  () =>
    !!address.value &&
    dashboard.value.servers.some((s) => s.address === address.value),
);
function action<T>(
  kind: string,
  data: Record<string, unknown> = {},
): Promise<T> {
  return props.rpc<T>("resource_manager", { action: { kind, ...data } });
}
const feed = useCatalogFeed<ResourceProject>(
  (offset) =>
    action("store_search", { scope: scope.value, query: query.value, offset }),
  (error) => emit("error", error),
);
const { hits, loading: searching, failure, more, revision, scrollTop } = feed;
let generation = 0,
  categoryGeneration = 0,
  disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined,
  stop: (() => void) | undefined;
async function loadCategories() {
  const request = ++categoryGeneration;
  categoriesLoading.value = true;
  categoriesFailed.value = false;
  try {
    const result = await action<{ id: string; name: string }[]>(
      "store_categories",
      { scope: scope.value },
    );
    if (!disposed && request === categoryGeneration) categories.value = result;
  } catch (error) {
    if (!disposed && request === categoryGeneration) {
      categoriesFailed.value = true;
      emit("error", error);
    }
  } finally {
    if (!disposed && request === categoryGeneration)
      categoriesLoading.value = false;
  }
}
watch(
  () => props.kind,
  (kind) => {
    categoryGeneration++;
    category.value = "";
    categories.value = [];
    categoriesLoading.value = categoriesFailed.value = false;
    if (kind !== "plugins") void loadCategories();
  },
  { immediate: true },
);
function reset() {
  generation++;
  project.value = undefined;
  server.value = undefined;
  versions.value = [];
  chosen.value = "";
  loading.value = false;
  complete.value = false;
  copied.value = false;
}
watch(
  scope,
  () => {
    reset();
    clearTimeout(timer);
    void feed.search();
  },
  { immediate: true },
);
watch(query, () => {
  feed.invalidate(true);
  clearTimeout(timer);
  timer = setTimeout(() => void feed.search(), 250);
});
watch(chosen, () => {
  complete.value = false;
});
async function select(item: ResourceProject) {
  if (blocked.value) return;
  reset();
  project.value = item;
  loading.value = true;
  const request = ++generation;
  try {
    if (props.kind === "servers") {
      const result = await action<StoreServer>("store_server", {
        project: item.id,
      });
      if (disposed || request !== generation) return;
      server.value = result;
    } else {
      const result = await action<StoreVersion[]>("store_versions", {
        scope: scope.value,
        project: item.id,
      });
      if (disposed || request !== generation) return;
      versions.value = result;
      chosen.value =
        result.find((v) => v.release_type === "release")?.id ||
        result[0]?.id ||
        "";
    }
  } catch (error) {
    if (!disposed && request === generation) emit("error", error);
  } finally {
    if (request === generation) loading.value = false;
  }
}
const detailRpc: ResourceRpc = <T,>(
  op: string,
  data?: Record<string, unknown>,
): Promise<T> => {
  if (props.kind === "servers" && op === "resource_project" && server.value)
    return Promise.resolve(server.value.details as T);
  return props.rpc<T>(op, data);
};
async function download() {
  if (blocked.value || !project.value || !selected.value || !filename.value)
    return;
  active.value = true;
  emit("busy", true);
  complete.value = false;
  progress.value = undefined;
  try {
    const path = await save({
      defaultPath: filename.value,
      filters: [
        {
          name: props.kind === "maps" ? "ZIP" : "JAR",
          extensions: [props.kind === "maps" ? "zip" : "jar"],
        },
      ],
    });
    if (!path || disposed) return;
    stop = await listen<LauncherEvent>("store-download-progress", (event) => {
      if (event.payload.kind === "progress") progress.value = event.payload;
    });
    await action("store_download", {
      scope: scope.value,
      project: project.value.id,
      version: selected.value.id,
      path,
    });
    if (!disposed) {
      complete.value = true;
      emit("downloaded");
    }
  } catch (error) {
    if (!disposed) emit("error", error);
  } finally {
    stop?.();
    stop = undefined;
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
async function copyAddress() {
  if (!address.value) return;
  try {
    await props.rpc("copy_text", { text: address.value });
    copied.value = true;
  } catch (error) {
    emit("error", error);
  }
}
function collectServer() {
  if (!address.value || !project.value || savedServer.value) return;
  try {
    if (dashboard.value.servers.length >= 32)
      throw new Error(t("最多添加 32 个服务器"));
    saveDashboard({
      ...dashboard.value,
      servers: [
        ...dashboard.value.servers,
        {
          id: crypto.randomUUID(),
          instance: "",
          version: "",
          name: project.value.name,
          address: address.value,
          icon: project.value.icon_url,
        },
      ],
    });
  } catch (error) {
    emit("error", error);
  }
}
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  categoryGeneration++;
  clearTimeout(timer);
  stop?.();
});
</script>

<template>
  <section class="store-catalog">
    <div v-if="!project" class="catalog-toolbar store-catalog-tools">
      <label class="resource-search"
        ><Search :size="14" /><input
          v-model="query"
          :placeholder="
            t(
              kind === 'servers'
                ? '搜索服务器'
                : kind === 'maps'
                  ? '搜索地图'
                  : '搜索插件',
            )
          "
          :aria-label="t('搜索资源')"
      /></label>
      <div class="catalog-filter-group">
        <GlassSelect
          v-if="kind === 'plugins'"
          id="store-catalog-source"
          :model-value="scope.platform"
          @update:model-value="platform = $event as ResourcePlatform"
          :disabled="blocked"
          :options="[
            { value: 'modrinth', label: 'Modrinth' },
            { value: 'curseforge', label: 'CurseForge' },
          ]"
          compact
          floating
          :aria-label="t('来源')"
        />
        <GlassSelect
          v-else
          id="store-catalog-category"
          v-model="category"
          class="store-tag-filter"
          :options="categoryOptions"
          :loading="categoriesLoading"
          :disabled="blocked"
          :aria-label="t('搜索标签')"
          :search-placeholder="t('搜索标签')"
          :menu-min-width="220"
          compact
          floating
          searchable
          literal-labels
          @open="categoriesFailed && loadCategories()"
        />
        <GlassSelect
          id="store-catalog-game"
          v-model="game"
          :options="[
            { value: '', label: t('全部版本') },
            ...(manifest?.versions || []).map((v) => ({
              value: v.id,
              label: v.id,
            })),
          ]"
          compact
          floating
          searchable
          literal-labels
          :aria-label="t('游戏版本')"
        />
        <GlassSelect
          v-if="kind === 'plugins'"
          id="store-catalog-loader"
          v-model="loader"
          :disabled="scope.platform !== 'modrinth'"
          :options="[
            { value: '', label: t('全部加载器') },
            ...[
              'paper',
              'spigot',
              'bukkit',
              'purpur',
              'folia',
              'sponge',
              'velocity',
              'waterfall',
              'bungeecord',
            ].map((v) => ({
              value: v,
              label:
                v === 'bungeecord'
                  ? 'BungeeCord'
                  : v[0]!.toUpperCase() + v.slice(1),
            })),
          ]"
          compact
          floating
          :aria-label="t('加载器')"
        />
      </div>
    </div>
    <ContentSwitch
      :content-key="project?.id || `results:${revision}`"
      class="store-catalog-content"
    >
      <div
        class="store-catalog-scroll"
        data-list-viewport
        data-catalog-viewport
      >
        <template v-if="project">
          <div
            class="store-project-title list-reveal-item"
            :style="listRevealStyle(0)"
          >
            <button
              class="icon-button"
              :disabled="blocked"
              :aria-label="t('返回')"
              @click="reset"
            >
              <ArrowLeft :size="18" />
            </button>
            <ProjectImage :url="project.icon_url" :rpc="rpc" :size="36"
              ><component :is="icon" :size="28"
            /></ProjectImage>
            <h3>{{ project.name }}</h3>
            <GlassSelect
              v-if="kind !== 'servers' && versions.length"
              id="store-artifact-version"
              v-model="chosen"
              :disabled="blocked"
              :options="
                versions.map((v) => ({
                  value: v.id,
                  label: v.number,
                  detail: v.games.join(' · '),
                }))
              "
              compact
              floating
              literal-labels
              :aria-label="t('版本')"
            />
            <LoaderCircle v-if="loading" class="spin" :size="16" />
            <button
              class="icon-button"
              :title="t('项目主页')"
              :aria-label="t('项目主页')"
              @click="emit('link', project.url)"
            >
              <ExternalLink :size="16" />
            </button>
            <button
              v-if="kind !== 'servers'"
              class="icon-button store-download"
              :disabled="blocked || !filename"
              :title="t(complete ? '已下载' : '下载')"
              :aria-label="t('下载')"
              @click="download"
            >
              <LoaderCircle v-if="active" class="spin" :size="18" /><Check
                v-else-if="complete"
                :size="18"
              /><Download v-else :size="18" />
            </button>
          </div>
          <div
            v-if="kind === 'servers' && server"
            class="store-server-address list-reveal-item"
            :style="listRevealStyle(1)"
          >
            <span>{{ address || t("未提供服务器地址") }}</span>
            <small v-if="server.online != null"
              >{{ t("在线") }} {{ server.online.toLocaleString() }}</small
            >
            <button
              v-if="address"
              class="icon-button"
              :title="t('复制地址')"
              :aria-label="t('复制地址')"
              @click="copyAddress"
            >
              <Check v-if="copied" :size="15" /><Copy v-else :size="15" />
            </button>
            <button
              v-if="address"
              class="text-button"
              :disabled="savedServer"
              @click="collectServer"
            >
              <Check v-if="savedServer" :size="15" /><Plus
                v-else
                :size="15"
              />{{ t(savedServer ? "已收藏" : "收藏服务器") }}
            </button>
          </div>
          <p
            v-if="!loading && kind !== 'servers' && !versions.length"
            class="store-catalog-empty"
          >
            {{ t("未找到兼容版本") }}
          </p>
          <ProjectDetails
            v-if="kind !== 'servers' || server"
            :key="`${scope.platform}/${project.id}`"
            :project="project"
            :hide-downloads="kind === 'servers'"
            :platform="scope.platform"
            :rpc="kind === 'servers' ? detailRpc : rpc"
            @link="emit('link', $event)"
          />
        </template>
        <template v-else>
          <CatalogProjectRow
            v-for="(item, index) in hits"
            :key="item.id"
            :project="item"
            :rpc="rpc"
            class="list-reveal-item"
            :style="listRevealStyle(index % 20)"
            :disabled="blocked"
            :hide-downloads="kind === 'servers'"
            @open="select(item)"
          >
            <template #icon><component :is="icon" :size="24" /></template>
          </CatalogProjectRow>
          <p
            v-if="!searching && !failure && !hits.length"
            class="store-catalog-empty"
          >
            {{ t("暂无结果") }}
          </p>
          <CatalogTail
            v-model:position="scrollTop"
            :loading="searching"
            :failure="failure"
            :more="more"
            :disabled="blocked"
            @load="feed.search(true)"
            @retry="feed.retry()"
          />
        </template>
      </div>
    </ContentSwitch>
    <div v-if="active && progress" class="store-catalog-progress">
      <progress
        :value="progress.bytes || 0"
        :max="progress.total_bytes || 1"
      /><small
        >{{ formatBytes(progress.bytes || 0) }} /
        {{ formatBytes(progress.total_bytes || 0) }}</small
      ><button class="icon-button" :aria-label="t('取消')" @click="cancel">
        <X :size="15" />
      </button>
    </div>
  </section>
</template>

<style scoped>
.store-catalog {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
  gap: 10px;
  padding: 0 var(--panel-inset) var(--panel-inset);
}
.store-catalog-tools {
  flex: none;
}
.store-tag-filter {
  grid-column: span 2;
}
.resource-search {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  color: var(--muted);
}
.resource-search input {
  width: 100%;
  background: transparent;
  color: var(--ink);
}
.store-catalog-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}
.store-catalog-empty {
  display: grid;
  place-items: center;
  min-height: 180px;
  color: var(--muted);
  font-size: 13px;
}
.store-project-title {
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 6px 0 16px;
}
.store-project-title h3 {
  margin: 0;
  font-size: 19px;
  font-weight: 550;
  min-width: 0;
  overflow-wrap: anywhere;
}
.store-project-title :deep(.source-select) {
  min-width: 80px;
  max-width: 180px;
}
.store-project-title :deep(.source-select-trigger) {
  border-color: transparent;
  background: transparent;
}
.store-download {
  color: var(--blue);
  margin-left: auto;
}
.store-server-address {
  display: flex;
  align-items: center;
  gap: 10px;
  color: var(--blue);
  font-size: 13px;
  padding-bottom: 12px;
}
.store-server-address small {
  color: var(--muted);
  font-size: 11px;
}
.store-server-address .text-button {
  margin-left: auto;
}
.store-catalog-progress {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 11px;
  color: var(--muted);
}
.store-catalog-progress progress {
  flex: 1;
  height: 3px;
  accent-color: var(--blue);
}
</style>
