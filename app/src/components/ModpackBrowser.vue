<script setup lang="ts">
import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  watch,
} from "vue";
import {
  Archive,
  ArrowLeft,
  ArrowRight,
  Download,
  LoaderCircle,
  Search,
  Users,
} from "lucide-vue-next";
import GlassSelect from "./GlassSelect.vue";
import ProjectImage from "./ProjectImage.vue";
import ProjectDetails from "./ProjectDetails.vue";
import ProjectLinks from "./ProjectLinks.vue";
import DownloadWorkflow from "./DownloadWorkflow.vue";
import ContentSwitch from "./ContentSwitch.vue";
import CatalogTail from "./CatalogTail.vue";
import { useCatalogFeed } from "../useCatalogFeed";
import { listRevealStyle } from "../listMotion";
import { locale, t } from "../i18n";
import { categoryLabel } from "../catalogFilters";
import type { Task } from "../types";
import type {
  ResourcePlatform,
  ResourceProject,
  ResourceVersion,
} from "../resources";

type PackVersion = ResourceVersion & { games: string[]; loaders: string[] };
const props = defineProps<{
  searchTarget?: HTMLElement;
  filterTarget?: HTMLElement;
  busy: boolean;
  task?: Task;
  controlPending?: boolean;
  retrying?: string;
  retryEnabled?: boolean;
  rpc: <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
  download: (
    platform: ResourcePlatform,
    project: string,
    version: string,
  ) => Promise<boolean>;
}>();
const emit = defineEmits<{
  error: [value: unknown];
  link: [url: string];
  pause: [];
  resume: [];
  cancel: [];
  retry: [file: string];
  continue: [];
  close: [];
  detail: [active: boolean];
}>();
const platform = ref<ResourcePlatform>("modrinth");
const query = ref("");
const sort = ref("relevance");
const category = ref("");
const downloadFormat = computed(
  () =>
    new Intl.NumberFormat(locale.value, {
      notation: "compact",
      maximumFractionDigits: 1,
    }),
);
const categories = shallowRef<{ id: string; name: string }[]>([]);
const categoriesLoading = ref(false);
const categoriesFailed = ref(false);
let categoryRequest = 0;
const categoryOptions = computed(() => [
  { value: "", label: t("全部类型") },
  ...categories.value.map((c) => ({
    value: c.id,
    label: categoryLabel(c.name),
    keywords: c.name,
  })),
]);
const categoryIds = computed(
  () =>
    new Map(categories.value.map((item) => [item.name.toLowerCase(), item.id])),
);
function tagCategory(tag: string) {
  return (
    categoryIds.value.get(tag.toLowerCase()) ||
    (platform.value === "modrinth" ? tag : "")
  );
}
function tagSelected(tag: string) {
  return !!category.value && tagCategory(tag) === category.value;
}
function filterTag(tag: string) {
  if (blocked.value) return;
  const id = tagCategory(tag);
  if (id) category.value = category.value === id ? "" : id;
}
function visibleTags(item: ResourceProject) {
  const tags = item.categories || [];
  const active = tags.find(tagSelected);
  return active
    ? [active, ...tags.filter((tag) => tag !== active)].slice(0, 3)
    : tags.slice(0, 3);
}
function hiddenTags(item: ResourceProject) {
  const shown = visibleTags(item);
  return (item.categories || []).filter((tag) => !shown.includes(tag));
}
async function loadCategories() {
  const generation = ++categoryRequest;
  categoriesLoading.value = true;
  categoriesFailed.value = false;
  try {
    const result = await props.rpc<{ id: string; name: string }[]>(
      "modpack_categories",
      { platform: platform.value },
    );
    if (!disposed && generation === categoryRequest) categories.value = result;
  } catch (error) {
    if (!disposed && generation === categoryRequest) {
      categoriesFailed.value = true;
      emit("error", error);
    }
  } finally {
    if (!disposed && generation === categoryRequest)
      categoriesLoading.value = false;
  }
}
const project = shallowRef<ResourceProject>();
watch(project, (value) => emit("detail", !!value), { immediate: true });
const versions = shallowRef<PackVersion[]>([]);
const chosen = ref("");
const latest = shallowRef<PackVersion>();
const loadingLatest = ref(false);
const latestFailed = ref(false);
const versionsLoaded = ref(false);
let latestRequest = 0;
const versionOffset = ref(0);
const moreVersions = ref(false);
const loadingVersions = ref(false);
const versionFailed = ref(false);
const versionAppendFailed = ref(false);
const downloading = ref(false);
const showTask = ref(
  !!props.task && ["running", "paused"].includes(props.task.status),
);
const feed = useCatalogFeed<ResourceProject>(
  (offset) =>
    props.rpc("search_modpacks", {
      platform: platform.value,
      query: query.value,
      filters: { sort: sort.value, category: category.value },
      offset,
    }),
  (error) => emit("error", error),
);
const {
  hits,
  loading: searching,
  failure,
  more,
  revision: resultGeneration,
  scrollTop,
} = feed;
const selected = computed(
  () =>
    versions.value.find((v) => v.id === chosen.value) ||
    (latest.value?.id === chosen.value ? latest.value : undefined),
);
function displayDate(value?: string | null) {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(+date) ? "" : date.toLocaleDateString(locale.value);
}
function versionDate(updated?: string | null) {
  return (
    selected.value?.published ||
    (selected.value?.id === project.value?.latest_version ? updated : null)
  );
}
const compatibility = computed(() => {
  if (!selected.value) return "";
  const loaders = selected.value.loaders.map(
    (name) =>
      ({
        fabric: "Fabric",
        forge: "Forge",
        neoforge: "NeoForge",
        quilt: "Quilt",
      })[name.toLowerCase()] || name,
  );
  const games = selected.value.games.filter(
    (game) =>
      !selected.value!.loaders.some(
        (loader) => loader.toLowerCase() === game.toLowerCase(),
      ),
  );
  return [...games, ...loaders].join(" · ");
});
const blocked = computed(() => props.busy || downloading.value);
let versionRequest = 0,
  disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;

async function search(append = false) {
  clearTimeout(timer);
  await feed.search(append);
}
const versionOptions = computed(() => {
  const choices = [...versions.value];
  if (latest.value && !choices.some((v) => v.id === latest.value?.id))
    choices.unshift(latest.value);
  return choices.map((v) => ({
    value: v.id,
    label: v.number || v.name,
    detail: [...v.games, ...v.loaders, displayDate(v.published)]
      .filter(Boolean)
      .join(" · "),
  }));
});
async function loadLatest() {
  if (!project.value?.latest_version) return;
  const generation = ++latestRequest;
  loadingLatest.value = true;
  latestFailed.value = false;
  try {
    const result = await props.rpc<PackVersion>("modpack_version", {
      platform: platform.value,
      project: project.value.id,
      version: project.value.latest_version,
    });
    if (disposed || generation !== latestRequest) return;
    latest.value = result;
    if (!chosen.value) chosen.value = result.id;
  } catch (error) {
    if (!disposed && generation === latestRequest) {
      latestFailed.value = true;
      emit("error", error);
    }
  } finally {
    if (!disposed && generation === latestRequest) loadingLatest.value = false;
  }
}
async function loadVersions(append = false) {
  if (
    !project.value ||
    loadingVersions.value ||
    (!append && versionsLoaded.value && !versionFailed.value)
  )
    return;
  const generation = ++versionRequest;
  loadingVersions.value = true;
  versionFailed.value = false;
  const nextOffset = append ? versionOffset.value + 50 : 0;
  try {
    const result = await props.rpc<{ versions: PackVersion[]; more: boolean }>(
      "modpack_versions",
      {
        platform: platform.value,
        project: project.value.id,
        offset: nextOffset,
      },
    );
    if (disposed || generation !== versionRequest) return;
    versions.value = append
      ? [
          ...versions.value,
          ...result.versions.filter(
            (v) => !versions.value.some((old) => old.id === v.id),
          ),
        ]
      : result.versions;
    versionsLoaded.value = true;
    moreVersions.value = result.more;
    versionOffset.value = nextOffset;
    if (!chosen.value)
      chosen.value = latest.value?.id || versions.value[0]?.id || "";
  } catch (error) {
    if (!disposed && generation === versionRequest) {
      versionFailed.value = true;
      versionAppendFailed.value = append;
      emit("error", error);
    }
  } finally {
    if (!disposed && generation === versionRequest)
      loadingVersions.value = false;
  }
}
function choose(item: ResourceProject) {
  if (blocked.value) return;
  project.value = item;
  versions.value = [];
  chosen.value = "";
  latest.value = undefined;
  latestFailed.value = false;
  versionFailed.value = false;
  loadingLatest.value = false;
  loadingVersions.value = false;
  versionsLoaded.value = false;
  moreVersions.value = false;
  void loadLatest();
}
function back() {
  versionRequest++;
  latestRequest++;
  project.value = undefined;
  showTask.value = false;
}
async function download() {
  if (!project.value || !chosen.value || blocked.value) return;
  downloading.value = true;
  showTask.value = true;
  try {
    await props.download(platform.value, project.value.id, chosen.value);
  } finally {
    downloading.value = false;
  }
}
watch(query, () => {
  feed.invalidate(true);
  clearTimeout(timer);
  timer = setTimeout(() => void search(), 250);
});
watch(
  platform,
  () => {
    category.value = "";
    categories.value = [];
    void loadCategories();
  },
  { flush: "sync" },
);
watch([platform, sort, category], () => {
  back();
  void search();
});
onMounted(() => {
  void search();
  void loadCategories();
});
onBeforeUnmount(() => {
  emit("detail", false);
  disposed = true;
  versionRequest++;
  latestRequest++;
  categoryRequest++;
  clearTimeout(timer);
});
</script>

<template>
  <Teleport v-if="searchTarget" :to="searchTarget">
    <Transition name="pack-search" appear>
      <label v-if="!project && !(showTask && task)" class="pack-search">
        <Search :size="14" />
        <input
          v-model="query"
          :disabled="blocked"
          :placeholder="t('搜索整合包')"
          :aria-label="t('搜索整合包')"
          @keydown.enter.prevent="search()"
        />
      </label>
    </Transition>
  </Teleport>
  <Teleport v-if="filterTarget" :to="filterTarget">
    <Transition name="pack-search" appear>
      <div v-if="!project && !(showTask && task)" class="pack-filters">
        <GlassSelect
          id="modpack-platform"
          v-model="platform"
          :options="[
            { value: 'modrinth', label: 'Modrinth' },
            { value: 'curseforge', label: 'CurseForge' },
          ]"
          :disabled="blocked"
          floating
          compact
          literal-labels
          :aria-label="t('来源')"
        />
        <GlassSelect
          id="modpack-sort"
          v-model="sort"
          :aria-label="t('排序方式')"
          :options="[
            { value: 'relevance', label: '综合排序' },
            { value: 'downloads', label: '下载最多' },
            { value: 'updated', label: '最近更新' },
            { value: 'newest', label: '最新发布' },
          ]"
          :disabled="blocked"
          floating
          compact
          :menu-min-width="160"
        />
        <GlassSelect
          id="modpack-category"
          v-model="category"
          :aria-label="t('类型')"
          :options="categoryOptions"
          :disabled="blocked"
          :loading="categoriesLoading"
          floating
          compact
          literal-labels
          searchable
          :menu-min-width="220"
          :search-placeholder="t('搜索类型名称')"
          @open="categoriesFailed && loadCategories()"
        />
      </div>
    </Transition>
  </Teleport>
  <div class="pack-browser">
    <ContentSwitch
      class="pack-flow"
      :content-key="showTask && task ? 'task' : 'browse'"
    >
      <template v-if="showTask && task">
        <DownloadWorkflow
          :task="task"
          :name="project?.name || t('整合包')"
          :version="selected?.number"
          :control-pending="controlPending"
          :retrying="retrying"
          :retry-enabled="retryEnabled"
          @retry="emit('retry', $event)"
          @continue="emit('continue')"
          @close="emit('close')"
          @pause="emit('pause')"
          @resume="emit('resume')"
          @cancel="emit('cancel')"
        />
        <footer
          v-if="!downloading"
          class="pack-footer list-reveal-item"
          :style="listRevealStyle(0)"
          data-list-footer
        >
          <button class="text-button" @click="showTask = false">
            <ArrowLeft :size="15" />{{ t("返回") }}
          </button>
          <button
            v-if="!task.operation"
            class="button primary"
            :disabled="blocked"
            @click="download"
          >
            {{ t("重试") }}
          </button>
        </footer>
      </template>
      <template v-else>
        <ContentSwitch
          class="pack-main"
          :content-key="project?.id || `${platform}:${resultGeneration}`"
          :aria-busy="searching || loadingVersions"
        >
          <div class="pack-results" data-list-viewport data-catalog-viewport>
            <template v-if="project">
              <ProjectDetails
                :key="`${platform}/${project.id}`"
                :project="project"
                :platform="platform"
                :rpc="rpc"
                compact
                @link="emit('link', $event)"
              >
                <template #leading>
                  <button
                    class="icon-button"
                    :disabled="blocked"
                    :aria-label="t('返回')"
                    @click="back"
                  >
                    <ArrowLeft :size="18" />
                  </button>
                  <ProjectImage :url="project.icon_url" :rpc="rpc" :size="42"
                    ><Archive :size="25"
                  /></ProjectImage>
                </template>
                <template #heading="{ updated }">
                  <div class="pack-title">
                    <div class="pack-title-line">
                      <h3 :title="project.name">{{ project.name }}</h3>
                      <GlassSelect
                        id="online-pack-version"
                        v-model="chosen"
                        :options="versionOptions"
                        :aria-label="t('整合包版本')"
                        :placeholder="t('最新版')"
                        :disabled="blocked"
                        :loading="loadingVersions"
                        text-only
                        literal-labels
                        floating
                        compact
                        :menu-min-width="260"
                        @open="loadVersions()"
                      >
                        <template #trigger-suffix>
                          <span
                            v-if="compatibility"
                            class="pack-version-compatibility"
                            :title="t('适用游戏版本')"
                          >
                            <span aria-hidden="true">·</span>{{ compatibility }}
                          </span>
                          <time
                            v-if="displayDate(versionDate(updated))"
                            class="pack-version-date"
                            :datetime="versionDate(updated) || undefined"
                            :title="`${t('更新于')} ${displayDate(versionDate(updated))}`"
                          >
                            <span aria-hidden="true">·</span
                            >{{ displayDate(versionDate(updated)) }}
                          </time>
                        </template>
                        <template #footer>
                          <button
                            v-if="versionFailed || moreVersions"
                            class="text-button pack-more"
                            :disabled="loadingVersions"
                            @click="
                              loadVersions(
                                versionAppendFailed || !versionFailed,
                              )
                            "
                          >
                            {{ t(versionFailed ? "重试" : "更多版本") }}
                          </button>
                          <span
                            v-else-if="versionsLoaded && !versions.length"
                            class="pack-muted"
                            >{{ t("暂无可用版本") }}</span
                          >
                        </template>
                      </GlassSelect>
                      <LoaderCircle
                        v-if="loadingLatest"
                        class="spin"
                        :size="12"
                      />
                      <button
                        v-else-if="latestFailed && !chosen"
                        class="text-button"
                        @click="loadLatest()"
                      >
                        {{ t("重试") }}
                      </button>
                    </div>
                  </div>
                </template>
                <template #actions="{ links, license }">
                  <div class="pack-title-actions">
                    <ProjectLinks
                      :links="links"
                      :license="license"
                      @link="emit('link', $event)"
                    />
                    <button
                      class="icon-button pack-download"
                      :aria-label="t('下载')"
                      :title="t('下载')"
                      :disabled="blocked || !chosen || loadingVersions"
                      @click="download"
                    >
                      <Download :size="17" />
                    </button>
                  </div>
                </template>
              </ProjectDetails>
            </template>
            <template v-else>
              <article
                v-for="(item, index) in hits"
                :key="item.id"
                class="pack-result list-reveal-item"
                :style="listRevealStyle(index % 20)"
                :class="{ disabled: blocked }"
              >
                <ProjectImage :url="item.icon_url" :rpc="rpc" :size="42"
                  ><Archive :size="24"
                /></ProjectImage>
                <div class="pack-result-copy">
                  <div class="pack-result-heading">
                    <div class="pack-result-title">
                      <button
                        type="button"
                        class="pack-result-open"
                        :disabled="blocked"
                        :title="item.name"
                        @click="choose(item)"
                      >
                        <strong>{{ item.name }}</strong>
                      </button>
                      <span
                        v-if="item.categories?.length"
                        class="pack-result-tags"
                      >
                        <button
                          v-for="tag in visibleTags(item)"
                          :key="tag"
                          type="button"
                          class="project-badge"
                          :class="{ active: tagSelected(tag) }"
                          :aria-pressed="tagSelected(tag)"
                          :disabled="blocked || !tagCategory(tag)"
                          :title="categoryLabel(tag)"
                          @click="filterTag(tag)"
                        >
                          {{ categoryLabel(tag) }}
                        </button>
                        <span
                          v-if="item.categories.length > 3"
                          class="project-badge pack-result-more-tags"
                          :title="
                            hiddenTags(item).map(categoryLabel).join(' · ')
                          "
                          >+{{ item.categories.length - 3 }}</span
                        >
                      </span>
                    </div>
                    <span
                      class="pack-result-downloads"
                      :title="`${t('下载量')}: ${item.downloads.toLocaleString(locale)}`"
                    >
                      <Download :size="12" />{{
                        downloadFormat.format(item.downloads)
                      }}
                    </span>
                    <span
                      v-if="item.authors?.length"
                      class="pack-result-authors"
                      :title="`${t('作者')}: ${item.authors.join(', ')}`"
                    >
                      <Users :size="12" /><span>{{
                        item.authors.join(", ")
                      }}</span>
                    </span>
                  </div>
                  <p>{{ item.description }}</p>
                </div>
                <ArrowRight :size="16" />
              </article>
              <div
                v-if="!searching && !failure && !hits.length"
                class="pack-empty"
              >
                {{ t("未找到整合包") }}
              </div>
              <CatalogTail
                v-model:position="scrollTop"
                :loading="searching"
                :more="more"
                :failure="failure"
                :disabled="blocked"
                @load="search(true)"
                @retry="feed.retry()"
              />
            </template>
          </div>
        </ContentSwitch>
      </template>
    </ContentSwitch>
  </div>
</template>

<style scoped>
.pack-browser {
  display: flex;
  flex-direction: column;
  gap: 18px;
  flex: 1;
  min-height: 0;
  padding: 12px 24px 20px;
  overflow: hidden;
}
.pack-flow > :deep(.content-switch-page) {
  gap: 18px;
}
.pack-footer {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-shrink: 0;
}
.pack-filters {
  display: grid;
  grid-template-columns: 1.05fr 1fr 1fr;
  gap: 8px;
  width: 100%;
  min-width: 0;
}
.pack-filters :deep(.source-select) {
  width: 100%;
  min-width: 0;
}
.pack-filters :deep(.source-select-trigger) {
  font-size: 12px;
  padding: 8px;
}
.pack-search {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
  color: var(--muted);
  border-bottom: 1px solid transparent;
}
.pack-search > svg {
  flex-shrink: 0;
}
.pack-search input {
  height: 32px;
  width: 100%;
  min-width: 0;
  padding: 8px 0;
  background: transparent;
  color: var(--ink);
  border: 0;
  border-radius: 0;
  outline: none;
  box-shadow: none;
  font-size: 12px;
}
.pack-search:focus-within {
  color: var(--blue);
  border-color: var(--blue);
}
.pack-search-enter-active,
.pack-search-leave-active {
  transition:
    opacity 160ms var(--motion-ease),
    transform 260ms var(--motion-ease);
}
.pack-search-enter-from,
.pack-search-leave-to {
  opacity: 0;
  transform: translateX(8px);
}
@media (prefers-reduced-motion: reduce) {
  .pack-search-enter-active,
  .pack-search-leave-active {
    transition: none;
  }
}
.pack-main {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overflow-x: hidden;
  overscroll-behavior: contain;
}
.pack-results {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overflow-x: hidden;
}
.pack-result {
  position: relative;
  display: flex;
  gap: 15px;
  align-items: center;
  width: 100%;
  text-align: left;
  background: transparent;
  color: var(--ink);
  border: 0;
  border-bottom: 1px solid var(--line);
  padding: 16px 8px;
}
.pack-result.disabled {
  opacity: 0.5;
}
.pack-result > svg:first-child {
  color: var(--blue);
}
.pack-result > svg {
  flex-shrink: 0;
}
.pack-result > svg:last-child {
  color: var(--muted);
}
.pack-result-copy {
  flex: 1;
  min-width: 0;
}
.pack-result-heading {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 68px minmax(0, 16%);
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.pack-result-title {
  grid-column: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.pack-result-open {
  min-width: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}
.pack-result-open::after {
  content: "";
  position: absolute;
  inset: 0;
}
.pack-result-open:focus-visible {
  outline: none;
  color: var(--blue);
  text-shadow: 0 0 10px var(--glow);
}
.pack-result-heading strong {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
  font-weight: 500;
}
.pack-result-downloads,
.pack-result-authors {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--muted);
  font-size: 11px;
  white-space: nowrap;
}
.pack-result-downloads > svg,
.pack-result-authors > svg {
  flex-shrink: 0;
}
.pack-result-downloads {
  grid-column: 2;
  font-variant-numeric: tabular-nums;
}
.pack-result-authors {
  grid-column: 3;
  min-width: 0;
}
.pack-result-authors > span {
  overflow: hidden;
  text-overflow: ellipsis;
}
.pack-result-tags {
  position: relative;
  z-index: 1;
  display: flex;
  gap: 4px;
  min-width: 0;
  max-width: 55%;
  flex-shrink: 0;
}
.pack-result-tags .project-badge {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: inherit;
  transition:
    color 160ms,
    background-color 160ms,
    box-shadow 160ms;
}
.pack-result-tags button {
  cursor: pointer;
}
.pack-result-tags button:hover:not(:disabled) {
  border-color: var(--blue);
}
.pack-result-tags button.active {
  background: var(--blue);
  color: var(--night);
  border-color: var(--blue);
}
.pack-result-tags button:focus-visible {
  outline: 1px solid var(--blue);
  outline-offset: 2px;
}
.pack-result-tags button:disabled {
  cursor: default;
}
.pack-result-tags .pack-result-more-tags {
  flex-shrink: 0;
}
.pack-result p {
  margin: 6px 0 0;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
}
.pack-result:has(.pack-result-open:hover) strong {
  color: var(--blue);
  text-shadow: 0 0 10px var(--glow);
}
.pack-footer {
  justify-content: flex-end;
}
.pack-empty {
  min-height: 180px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--muted);
  font-size: 13px;
}
.pack-title-line {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 2px 12px;
  min-width: 0;
}
.pack-title-line > :deep(.source-select) {
  max-width: 100%;
}
.pack-title h3 {
  margin: 0;
  max-width: 100%;
  font-size: 18px;
  line-height: 1.4;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pack-title small,
.pack-muted {
  font-size: 12px;
  color: var(--muted);
}
.pack-more {
  margin-top: 12px;
  font-size: 12px;
}
.pack-version-compatibility,
.pack-version-date {
  display: inline-flex;
  align-items: baseline;
  gap: 7px;
  flex-shrink: 0;
  font-size: 11px;
  color: var(--muted);
  opacity: 0.72;
  font-variant-numeric: tabular-nums;
}
.pack-title-actions {
  display: flex;
  align-items: center;
  align-self: flex-start;
  gap: 8px;
  margin-left: auto;
  flex-shrink: 0;
}
.pack-download {
  width: 34px;
  height: 34px;
}
.pack-download:hover:not(:disabled) {
  color: var(--blue);
  background: transparent;
  filter: drop-shadow(0 0 6px var(--glow));
}
.pack-title-line :deep(.source-select-trigger) {
  gap: 7px;
  flex-wrap: wrap;
  justify-content: flex-start;
}
</style>
