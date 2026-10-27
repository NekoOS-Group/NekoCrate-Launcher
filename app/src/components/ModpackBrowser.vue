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
  ExternalLink,
  LoaderCircle,
  Search,
  Upload,
} from "lucide-vue-next";
import GlassSelect from "./GlassSelect.vue";
import DownloadWorkflow from "./DownloadWorkflow.vue";
import ContentSwitch from "./ContentSwitch.vue";
import { listRevealStyle } from "../listMotion";
import { t } from "../i18n";
import type { Task } from "../types";
import type {
  ResourcePlatform,
  ResourceProject,
  ResourceVersion,
} from "../resources";

type PackVersion = ResourceVersion & { games: string[]; loaders: string[] };
const props = defineProps<{
  busy: boolean;
  task?: Task;
  controlPending?: boolean;
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
  import: [];
  settings: [];
  pause: [];
  resume: [];
  cancel: [];
  close: [];
}>();
const platform = ref<ResourcePlatform>("modrinth");
const query = ref("");
const offset = ref(0);
const total = ref(0);
const hits = shallowRef<ResourceProject[]>([]);
const project = shallowRef<ResourceProject>();
const versions = shallowRef<PackVersion[]>([]);
const chosen = ref("");
const versionOffset = ref(0);
const moreVersions = ref(false);
const searching = ref(false);
const loadingVersions = ref(false);
const failed = ref(false);
const versionFailed = ref(false);
const downloading = ref(false);
const showTask = ref(
  !!props.task && ["running", "paused"].includes(props.task.status),
);
const resultGeneration = ref(0);
const selected = computed(() =>
  versions.value.find((v) => v.id === chosen.value),
);
const blocked = computed(() => props.busy || downloading.value);
let request = 0,
  versionRequest = 0,
  disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;

async function search() {
  clearTimeout(timer);
  const generation = ++request;
  searching.value = true;
  failed.value = false;
  try {
    const result = await props.rpc<{ hits: ResourceProject[]; total: number }>(
      "search_modpacks",
      { platform: platform.value, query: query.value, offset: offset.value },
    );
    if (disposed || generation !== request) return;
    hits.value = result.hits;
    resultGeneration.value++;
    total.value = result.total;
  } catch (error) {
    if (!disposed && generation === request) {
      failed.value = true;
      emit("error", error);
    }
  } finally {
    if (!disposed && generation === request) searching.value = false;
  }
}
async function loadVersions(append = false) {
  if (!project.value) return;
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
    moreVersions.value = result.more;
    versionOffset.value = nextOffset;
    if (!chosen.value)
      chosen.value =
        versions.value.find((v) => v.release_type === "release")?.id ||
        versions.value[0]?.id ||
        "";
  } catch (error) {
    if (!disposed && generation === versionRequest) {
      versionFailed.value = true;
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
  moreVersions.value = false;
  void loadVersions();
}
function back() {
  versionRequest++;
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
function page(delta: number) {
  offset.value = Math.max(0, offset.value + delta);
  void search();
}
watch(query, () => {
  request++;
  clearTimeout(timer);
  // Do not let stale results from the previous query remain clickable.
  searching.value = true;
  timer = setTimeout(() => {
    offset.value = 0;
    void search();
  }, 250);
});
watch(platform, () => {
  back();
  hits.value = [];
  total.value = 0;
  offset.value = 0;
  void search();
});
onMounted(() => void search());
onBeforeUnmount(() => {
  disposed = true;
  request++;
  versionRequest++;
  clearTimeout(timer);
});
</script>

<template>
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
          <button class="button primary" :disabled="blocked" @click="download">
            {{ t("重试") }}
          </button>
        </footer>
      </template>
      <template v-else>
        <div class="pack-tools list-reveal-item" :style="listRevealStyle(0)">
          <button
            v-if="project"
            class="icon-button"
            :disabled="blocked"
            :aria-label="t('返回')"
            @click="back"
          >
            <ArrowLeft :size="18" />
          </button>
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
          />
          <label v-if="!project" class="pack-search"
            ><Search :size="16" /><input
              v-model="query"
              :disabled="blocked"
              :placeholder="t('搜索整合包')"
              :aria-label="t('搜索整合包')"
              @keydown.enter.prevent="
                offset = 0;
                search();
              "
          /></label>
          <button
            v-if="platform === 'curseforge'"
            class="text-button pack-key"
            :disabled="blocked"
            @click="emit('settings')"
          >
            {{ t("网络设置") }}
          </button>
        </div>
        <ContentSwitch
          class="pack-main"
          :content-key="project?.id || `${platform}:${resultGeneration}`"
          :aria-busy="searching || loadingVersions"
        >
          <div class="pack-results" data-list-viewport>
            <template v-if="project">
              <div
                class="pack-detail-heading list-reveal-item"
                :style="listRevealStyle(0)"
              >
                <Archive :size="32" />
                <div>
                  <h3>{{ project.name }}</h3>
                  <small>{{
                    platform === "modrinth" ? "Modrinth" : "CurseForge"
                  }}</small>
                </div>
                <button
                  class="icon-button"
                  :aria-label="t('项目主页')"
                  @click="emit('link', project.url)"
                >
                  <ExternalLink :size="17" />
                </button>
              </div>
              <p
                class="pack-description list-reveal-item"
                :style="listRevealStyle(1)"
              >
                {{ project.description }}
              </p>
              <div
                class="pack-version list-reveal-item"
                :style="listRevealStyle(2)"
              >
                <span>{{ t("版本") }}</span>
                <GlassSelect
                  v-if="versions.length"
                  id="online-pack-version"
                  v-model="chosen"
                  :options="
                    versions.map((v) => ({
                      value: v.id,
                      label: v.name || v.number,
                      detail: [...v.games, ...v.loaders].join(' · '),
                    }))
                  "
                  :disabled="blocked"
                  literal-labels
                  floating
                  compact
                />
                <LoaderCircle v-if="loadingVersions" class="spin" :size="16" />
                <button
                  v-else-if="versionFailed"
                  class="text-button"
                  @click="loadVersions()"
                >
                  {{ t("重试") }}
                </button>
                <span v-else-if="!versions.length" class="pack-muted">{{
                  t("暂无可用版本")
                }}</span>
              </div>
              <button
                v-if="moreVersions"
                class="text-button pack-more"
                :disabled="blocked || loadingVersions"
                @click="loadVersions(true)"
              >
                {{ t("更多版本") }}
              </button>
              <p v-if="selected" class="pack-compatibility">
                {{ [...selected.games, ...selected.loaders].join(" · ") }}
              </p>
            </template>
            <template v-else>
              <button
                v-for="(item, index) in hits"
                :key="item.id"
                class="pack-result list-reveal-item"
                :style="listRevealStyle(index)"
                :disabled="blocked || searching"
                @click="choose(item)"
              >
                <Archive :size="24" />
                <div>
                  <strong>{{ item.name }}</strong>
                  <p>{{ item.description }}</p>
                </div>
                <ArrowRight :size="16" />
              </button>
              <div v-if="searching && !hits.length" class="pack-empty">
                <LoaderCircle class="spin" :size="22" />
              </div>
              <div v-else-if="failed" class="pack-empty">
                <button class="text-button" @click="search">
                  {{ t("重试") }}
                </button>
              </div>
              <div v-else-if="!hits.length" class="pack-empty">
                {{ t("未找到整合包") }}
              </div>
            </template>
          </div>
        </ContentSwitch>
        <footer
          class="pack-footer list-reveal-item"
          :style="listRevealStyle(3)"
          data-list-footer
        >
          <button
            class="text-button"
            :disabled="blocked"
            @click="emit('import')"
          >
            <Upload :size="15" />{{ t("导入") }}
          </button>
          <button
            v-if="project"
            class="button primary"
            :disabled="blocked || !chosen || loadingVersions"
            @click="download"
          >
            <Download :size="15" />{{ t("下载并预览") }}
          </button>
          <div v-else class="pack-pagination">
            <LoaderCircle v-if="searching" class="spin" :size="13" />
            <button
              class="icon-button"
              :disabled="blocked || searching || !offset"
              :aria-label="t('上一页')"
              @click="page(-20)"
            >
              <ArrowLeft :size="16" />
            </button>
            <small>{{ Math.floor(offset / 20) + 1 }}</small>
            <button
              class="icon-button"
              :disabled="
                blocked || searching || offset + 20 >= total || offset >= 9900
              "
              :aria-label="t('下一页')"
              @click="page(20)"
            >
              <ArrowRight :size="16" />
            </button>
          </div>
        </footer>
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
.pack-tools,
.pack-footer,
.pack-pagination,
.pack-version {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-shrink: 0;
}
.pack-tools :deep(.glass-select) {
  min-width: 130px;
}
.pack-search {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 1;
  min-width: 0;
  color: var(--muted);
}
.pack-search input {
  width: 100%;
  padding: 8px 0;
  background: transparent;
  color: var(--ink);
  border: 0;
  border-bottom: 1px solid var(--line);
  border-radius: 0;
  font-size: 13px;
}
.pack-key {
  margin-left: auto;
  font-size: 11px;
  white-space: nowrap;
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
  cursor: pointer;
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
.pack-result div {
  flex: 1;
  min-width: 0;
}
.pack-result strong {
  font-size: 14px;
  font-weight: 500;
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
.pack-result:hover strong {
  color: var(--blue);
  text-shadow: 0 0 10px var(--glow);
}
.pack-footer {
  justify-content: space-between;
}
.pack-pagination {
  margin-left: auto;
  color: var(--muted);
}
.pack-empty {
  min-height: 180px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--muted);
  font-size: 13px;
}
.pack-detail-heading {
  display: flex;
  align-items: center;
  gap: 16px;
  margin: 12px 0 20px;
}
.pack-detail-heading > svg {
  color: var(--blue);
  flex-shrink: 0;
}
.pack-detail-heading div {
  flex: 1;
  min-width: 0;
}
.pack-detail-heading h3 {
  margin: 0 0 4px;
  font-size: 20px;
  line-height: 1.4;
  overflow-wrap: anywhere;
}
.pack-detail-heading small,
.pack-compatibility,
.pack-muted {
  font-size: 12px;
  color: var(--muted);
}
.pack-description {
  font-size: 13px;
  line-height: 1.8;
  margin-bottom: 28px;
  overflow-wrap: anywhere;
}
.pack-version {
  font-size: 12px;
}
.pack-version :deep(.glass-select) {
  flex: 1;
  min-width: 0;
  max-width: 440px;
}
.pack-more {
  margin-top: 12px;
  font-size: 12px;
}
.pack-compatibility {
  font-family: var(--mono);
  line-height: 1.8;
}
</style>
