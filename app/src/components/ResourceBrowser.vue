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
  ArrowLeft,
  ArrowRight,
  Check,
  Download,
  FileCode2,
  FolderOpen,
  Image,
  LoaderCircle,
  Map,
  Package,
  Search,
  Sparkles,
  Upload,
  X,
  ExternalLink,
} from "lucide-vue-next";
import { open } from "@tauri-apps/plugin-dialog";
import { t, translateMessage } from "../i18n";
import { versionName } from "../versionDescription";
import type { Task } from "../types";
import type {
  DetectedResource,
  ResourceKind,
  ResourcePlan,
  ResourcePlatform,
  ResourceProject,
  ResourceSession,
  ResourceTarget,
  ResourceVersion,
} from "../resources";
import GlassSelect from "./GlassSelect.vue";
import DownloadWorkflow from "./DownloadWorkflow.vue";
import ContentSwitch from "./ContentSwitch.vue";
import { listRevealStyle } from "../listMotion";

const props = defineProps<{
  embedded?: boolean;
  session: ResourceSession;
  busy: boolean;
  task?: Task;
  controlPending?: boolean;
  retrying?: string;
  retryEnabled?: boolean;
  rpc: <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
  prepare: (
    op: string,
    data: Record<string, unknown>,
  ) => Promise<ResourcePlan | undefined>;
  install: (plan: ResourcePlan) => Promise<boolean>;
}>();
const emit = defineEmits<{
  close: [];
  error: [error: unknown];
  link: [url: string];
  pause: [];
  resume: [];
  cancel: [];
  retry: [file: string];
  continue: [];
  installed: [];
}>();
const categories = [
  { value: "mods", label: "Mods", icon: Package, color: "#69dcb5" },
  { value: "resourcepacks", label: "资源包", icon: Image, color: "#b49aef" },
  { value: "datapacks", label: "数据包", icon: FileCode2, color: "#e7bc76" },
  { value: "shaderpacks", label: "光影", icon: Sparkles, color: "#e6c68a" },
  { value: "saves", label: "存档", icon: Map, color: "#7dbfee" },
] as const;
const kind = ref<ResourceKind>(props.session.kind);
const platform = ref<ResourcePlatform>(props.session.platform || "modrinth");
const query = ref("");
const target = shallowRef<ResourceTarget>();
const project = shallowRef<ResourceProject>();
const hits = shallowRef<ResourceProject[]>([]);
const versions = shallowRef<ResourceVersion[]>([]);
const chosen = ref("");
const world = ref("");
const offset = ref(0);
const total = ref(0);
const searching = ref(false);
const resultGeneration = ref(0);
const loadingVersions = ref(false);
const preparing = ref(false);
const installing = ref(false);
const complete = ref(false);
const plan = shallowRef<ResourcePlan>();
const pending = shallowRef<ResourcePlan[]>([]);
const files = ref<DetectedResource[]>(props.session.files || []);
const selectedKinds = ref<Record<string, ResourceKind>>({});
const local = ref(!!props.session.files?.length);
const icon = (category: ResourceKind) =>
  categories.find((c) => c.value === category)?.icon || Package;
const versionTitle = computed(() =>
  versionName(
    props.session.instance.versions.find((v) => v.id === props.session.version),
  ),
);
const scope = computed(() => ({
  id: props.session.instance.id,
  version: props.session.version,
  platform: platform.value,
  kind: kind.value,
}));
const worldRequired = computed(() =>
  local.value
    ? files.value.some((f) => selectedKinds.value[f.path] === "datapacks")
    : kind.value === "datapacks",
);
const blocked = computed(
  () => props.busy || preparing.value || installing.value,
);
const showTask = computed(
  () =>
    !!props.task &&
    (installing.value ||
      (!!plan.value && props.task.resource_task === plan.value.id)),
);
const canPreview = computed(
  () =>
    target.value &&
    !blocked.value &&
    (!worldRequired.value || world.value) &&
    (local.value
      ? files.value.length > 0 &&
        files.value.every((f) => selectedKinds.value[f.path])
      : !!chosen.value),
);
const bytes = (size: number) =>
  size > 1024 * 1024
    ? `${(size / 1024 / 1024).toFixed(1)} MB`
    : `${Math.ceil(size / 1024)} KB`;
let searchRequest = 0;
let versionRequest = 0;
let disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;
async function search() {
  const request = ++searchRequest;
  if (
    local.value ||
    kind.value === "saves" ||
    !target.value ||
    (kind.value === "mods" && !target.value.loader)
  ) {
    searching.value = false;
    return;
  }
  searching.value = true;
  try {
    const response = await props.rpc<{
      hits: ResourceProject[];
      total: number;
    }>("search_resources", {
      scope: scope.value,
      query: query.value,
      offset: offset.value,
    });
    if (request !== searchRequest || disposed) return;
    hits.value = response.hits;
    resultGeneration.value++;
    total.value = response.total;
  } catch (error) {
    if (request === searchRequest && !disposed) emit("error", error);
  } finally {
    if (request === searchRequest) searching.value = false;
  }
}
async function selectProject(item: ResourceProject) {
  if (blocked.value) return;
  const request = ++versionRequest;
  project.value = item;
  versions.value = [];
  chosen.value = "";
  loadingVersions.value = true;
  try {
    const result = await props.rpc<ResourceVersion[]>("resource_versions", {
      scope: scope.value,
      project: item.id,
    });
    if (request !== versionRequest || disposed) return;
    versions.value = result;
    chosen.value =
      result.find((v) => v.release_type === "release")?.id ||
      result[0]?.id ||
      "";
  } catch (error) {
    if (request === versionRequest && !disposed) emit("error", error);
  } finally {
    if (request === versionRequest) loadingVersions.value = false;
  }
}
function back() {
  if (blocked.value) return;
  if (plan.value) {
    plan.value = undefined;
    complete.value = false;
  } else if (project.value) {
    project.value = undefined;
    versionRequest++;
    loadingVersions.value = false;
  } else {
    local.value = false;
    files.value = [];
  }
}
function setFiles(value: DetectedResource[]) {
  files.value = value;
  selectedKinds.value = Object.fromEntries(
    value.filter((f) => f.kinds.length === 1).map((f) => [f.path, f.kinds[0]]),
  );
  local.value = true;
  project.value = undefined;
  plan.value = undefined;
  complete.value = false;
}
async function pick(directory = false) {
  try {
    const paths = await open({
      multiple: true,
      directory,
      title: t(directory ? "导入资源目录" : "导入资源文件"),
      ...(directory
        ? {}
        : { filters: [{ name: "Minecraft", extensions: ["jar", "zip"] }] }),
    });
    if (!paths) return;
    const result = await props.rpc<DetectedResource[]>("inspect_resources", {
      paths: Array.isArray(paths) ? paths : [paths],
    });
    if (!disposed) setFiles(result);
  } catch (error) {
    emit("error", error);
  }
}
async function preview() {
  if (!canPreview.value) return;
  preparing.value = true;
  try {
    const result = await props.prepare(
      local.value ? "prepare_local_resources" : "prepare_resource",
      local.value
        ? {
            id: scope.value.id,
            version: scope.value.version,
            inputs: files.value.map((f) => ({
              path: f.path,
              kind: selectedKinds.value[f.path],
            })),
            world: world.value || null,
          }
        : {
            scope: scope.value,
            project: project.value!.id,
            version: chosen.value,
            world: world.value || null,
          },
    );
    if (!disposed && result) {
      plan.value = result;
      complete.value = false;
    }
  } finally {
    preparing.value = false;
  }
}
async function install() {
  if (!plan.value || blocked.value || plan.value.conflicts.length) return;
  installing.value = true;
  try {
    complete.value = await props.install(plan.value);
    if (complete.value) {
      pending.value = pending.value.filter((p) => p.id !== plan.value?.id);
      emit("installed");
    }
  } finally {
    installing.value = false;
  }
}
watch(
  () => props.task?.status,
  (status) => {
    if (
      status !== "done" ||
      installing.value ||
      complete.value ||
      !plan.value ||
      props.task?.resource_task !== plan.value.id
    )
      return;
    complete.value = true;
    pending.value = pending.value.filter((p) => p.id !== plan.value!.id);
    emit("installed");
  },
);
watch([kind, platform, local], () => {
  searchRequest++;
  versionRequest++;
  project.value = undefined;
  chosen.value = "";
  hits.value = [];
  total.value = 0;
  offset.value = 0;
  void search();
});
watch(query, () => {
  clearTimeout(timer);
  searchRequest++;
  timer = setTimeout(() => {
    offset.value = 0;
    void search();
  }, 250);
});
watch(offset, () => void search());
watch(
  () => props.session.files,
  (value) => {
    if (value && !blocked.value) setFiles(value);
  },
);
onMounted(async () => {
  if (files.value.length) setFiles(files.value);
  try {
    const result = await props.rpc<ResourceTarget>("resource_target", {
      id: scope.value.id,
      version: scope.value.version,
    });
    if (disposed) return;
    target.value = result;
    if (result.worlds.length === 1) world.value = result.worlds[0];
    void search();
    const plans = await props.rpc<ResourcePlan[]>("resource_plans", {
      id: scope.value.id,
      version: scope.value.version,
    });
    if (!disposed) pending.value = plans.filter((p) => !p.conflicts.length);
  } catch (error) {
    if (!disposed) emit("error", error);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  clearTimeout(timer);
  searchRequest++;
  versionRequest++;
});
</script>

<template>
  <section
    class="resource-browser"
    :data-drop-kind="kind"
    :class="embedded ? 'resource-embedded' : 'modal glass-surface'"
    :data-resource-instance="session.instance.id"
    :data-resource-version="session.version"
  >
    <ContentSwitch
      class="resource-flow"
      :content-key="
        showTask && task ? 'task' : plan ? `plan:${plan.id}` : 'browse'
      "
    >
      <template v-if="showTask && task">
        <DownloadWorkflow
          :task="task"
          :name="t('添加资源')"
          :version="versionTitle"
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
          v-if="!installing"
          class="resource-footer list-reveal-item"
          :style="listRevealStyle(0)"
          data-list-footer
        >
          <span v-if="complete" class="resource-success"
            ><Check :size="16" />{{ t("资源安装完成") }}</span
          >
          <button
            v-else-if="!task.operation"
            class="text-button"
            :disabled="busy"
            @click="install"
          >
            <Download :size="15" />{{ t("继续安装") }}
          </button>
          <button
            class="text-button"
            @click="
              plan = undefined;
              complete = false;
            "
          >
            {{ t("继续浏览") }}
          </button>
        </footer>
      </template>
      <template v-else>
        <header
          v-if="!embedded || plan || project || local"
          class="modal-header resource-header list-reveal-item"
          :style="listRevealStyle(0)"
        >
          <button
            v-if="plan || project || local"
            class="icon-button"
            :disabled="blocked"
            :aria-label="t('返回')"
            @click="back"
          >
            <ArrowLeft :size="18" />
          </button>
          <div>
            <h2 class="dialog-heading" tabindex="-1">
              {{ t(plan ? "安装预览" : "添加资源") }}
            </h2>
            <small v-if="!embedded"
              >{{ session.instance.name }} · {{ versionTitle
              }}<span v-if="target">
                · {{ target.minecraft
                }}{{ target.loader ? ` · ${target.loader.kind}` : "" }}</span
              ></small
            >
          </div>
          <button
            v-if="!embedded"
            class="icon-button resource-close"
            :disabled="preparing"
            :aria-label="t('关闭对话框')"
            @click="emit('close')"
          >
            <X :size="18" />
          </button>
        </header>
        <div
          v-if="target?.shared"
          class="resource-shared list-reveal-item"
          :style="listRevealStyle(1)"
        >
          {{ t("此版本与同目录的其他版本共享资源") }}
        </div>
        <template v-if="plan">
          <div class="resource-plan-list" data-list-viewport>
            <div
              v-for="(entry, index) in plan.entries"
              :key="entry.destination"
              class="resource-plan-item list-reveal-item"
              :style="listRevealStyle(index)"
            >
              <component :is="icon(entry.kind)" :size="22" />
              <div>
                <strong>{{ entry.origin.name }}</strong
                ><small>{{ entry.destination }}</small>
              </div>
              <span>{{
                entry.reuse ? t("复用已有文件") : bytes(entry.size)
              }}</span>
            </div>
            <p
              v-for="(issue, index) in plan.conflicts"
              :key="issue"
              class="resource-conflict list-reveal-item"
              :style="listRevealStyle(plan.entries.length + index)"
            >
              {{ translateMessage(issue) }}
            </p>
            <p
              v-for="(warning, index) in plan.warnings"
              :key="warning"
              class="resource-warning list-reveal-item"
              :style="
                listRevealStyle(
                  plan.entries.length + plan.conflicts.length + index,
                )
              "
            >
              {{ translateMessage(warning) }}
            </p>
          </div>
          <footer
            class="resource-footer list-reveal-item"
            :style="listRevealStyle(3)"
            data-list-footer
          >
            <small>{{ t("必要前置已包含，不覆盖已有的不同文件") }}</small
            ><button
              class="button primary"
              :disabled="blocked || !!plan.conflicts.length"
              @click="install"
            >
              <Download :size="16" />{{ t("确认安装") }}
            </button>
          </footer>
        </template>
        <template v-else>
          <nav
            v-if="!local"
            class="resource-categories"
            :aria-label="t('资源类型')"
          >
            <button
              v-for="(category, index) in categories"
              :key="category.value"
              :data-drop-kind="category.value"
              :disabled="blocked"
              class="list-reveal-item"
              :style="{
                '--category-color': category.color,
                ...listRevealStyle(index),
              }"
              :class="{ active: kind === category.value }"
              @click="kind = category.value"
            >
              <component :is="category.icon" :size="17" />{{
                t(category.label)
              }}
            </button>
          </nav>
          <div
            v-if="!local && !project && kind !== 'saves'"
            class="resource-tools list-reveal-item"
            :style="listRevealStyle(1)"
          >
            <GlassSelect
              id="resource-provider"
              v-model="platform"
              :disabled="blocked"
              compact
              :options="[
                { value: 'modrinth', label: 'Modrinth' },
                { value: 'curseforge', label: 'CurseForge' },
              ]"
            />
            <label class="resource-search"
              ><Search :size="15" /><input
                v-model="query"
                :disabled="blocked"
                :placeholder="t('搜索资源')"
                :aria-label="t('搜索资源')"
            /></label>
            <LoaderCircle v-if="searching" class="spin" :size="16" />
          </div>
          <div
            v-if="worldRequired"
            class="resource-world list-reveal-item"
            :style="listRevealStyle(2)"
          >
            <span>{{ t("目标存档") }}</span
            ><GlassSelect
              v-if="target?.worlds.length"
              id="resource-world"
              v-model="world"
              literal-labels
              compact
              floating
              :disabled="blocked"
              :options="[
                { value: '', label: t('选择存档') },
                ...target.worlds.map((w) => ({ value: w, label: w })),
              ]"
            /><small v-else>{{ t("请先在此版本中创建存档") }}</small>
          </div>
          <ContentSwitch
            class="resource-main"
            :content-key="
              local
                ? 'local'
                : project?.id || `${kind}:${platform}:${resultGeneration}`
            "
          >
            <div class="resource-results" data-list-viewport>
              <template v-if="local">
                <div
                  v-for="(file, index) in files"
                  :key="file.path"
                  class="resource-local-row list-reveal-item"
                  :style="listRevealStyle(index)"
                >
                  <component :is="icon(selectedKinds[file.path])" :size="22" />
                  <div>
                    <strong>{{ file.name }}</strong
                    ><small :title="file.path">{{ file.path }}</small
                    ><span
                      v-if="file.issue || file.format !== 'resource'"
                      class="resource-conflict"
                      >{{
                        file.issue
                          ? translateMessage(file.issue)
                          : t("未识别的资源文件")
                      }}</span
                    >
                  </div>
                  <GlassSelect
                    v-if="file.kinds.length > 1"
                    :id="`kind-${files.indexOf(file)}`"
                    :model-value="selectedKinds[file.path] || ''"
                    compact
                    floating
                    :options="[
                      { value: '', label: '选择资源类型' },
                      ...categories.filter((c) => file.kinds.includes(c.value)),
                    ]"
                    @update:model-value="
                      selectedKinds[file.path] = $event as ResourceKind
                    "
                  />
                  <small v-else>{{
                    t(
                      categories.find((c) => c.value === file.kinds[0])
                        ?.label || "",
                    )
                  }}</small>
                </div>
              </template>
              <div v-else-if="project" class="resource-detail">
                <div
                  class="resource-detail-title list-reveal-item"
                  :style="listRevealStyle(0)"
                >
                  <component :is="icon(kind)" :size="30" />
                  <h3>{{ project.name }}</h3>
                  <button
                    class="icon-button"
                    :aria-label="t('项目主页')"
                    @click="emit('link', project.url)"
                  >
                    <ExternalLink :size="16" />
                  </button>
                </div>
                <p class="list-reveal-item" :style="listRevealStyle(1)">
                  {{ project.description }}
                </p>
                <div
                  class="resource-compatible list-reveal-item"
                  :style="listRevealStyle(2)"
                >
                  <label>{{ t("兼容版本") }}</label>
                  <GlassSelect
                    v-if="versions.length"
                    id="resource-version"
                    v-model="chosen"
                    literal-labels
                    floating
                    :disabled="blocked"
                    :options="
                      versions.map((v) => ({
                        value: v.id,
                        label: v.number,
                        detail: v.release_type,
                      }))
                    "
                  />
                  <LoaderCircle
                    v-else-if="loadingVersions"
                    :size="18"
                    class="spin"
                  />
                  <p v-else>{{ t("未找到兼容版本") }}</p>
                </div>
              </div>
              <template v-else-if="kind === 'saves'"
                ><div
                  class="resource-empty list-reveal-item"
                  :style="listRevealStyle(0)"
                >
                  <Map :size="40" />
                  <p>{{ t("将地图目录或 ZIP 拖入此窗口") }}</p>
                  <button class="text-button" @click="pick()">
                    <Upload :size="16" />{{ t("导入资源文件") }}
                  </button>
                </div></template
              >
              <template v-else>
                <button
                  v-for="(item, index) in hits"
                  :key="item.id"
                  class="resource-result list-reveal-item"
                  :style="listRevealStyle(index)"
                  :disabled="blocked"
                  @click="selectProject(item)"
                >
                  <component :is="icon(kind)" :size="26" />
                  <div>
                    <strong>{{ item.name }}</strong>
                    <p>{{ item.description }}</p>
                  </div>
                  <span
                    ><Download :size="12" />{{
                      item.downloads.toLocaleString()
                    }}</span
                  >
                </button>
                <p v-if="!searching && !hits.length" class="resource-empty">
                  {{
                    t(
                      kind === "mods" && target && !target.loader
                        ? "此版本尚未安装 Mod 加载器"
                        : "未找到兼容资源",
                    )
                  }}
                </p>
              </template>
            </div>
          </ContentSwitch>
          <div
            v-if="pending.length && !local && !project"
            class="resource-pending list-reveal-item"
            :style="listRevealStyle(2)"
          >
            <span>{{ t("未完成的安装") }}</span
            ><button
              v-for="item in pending.slice(0, 4)"
              :key="item.id"
              class="text-button"
              :disabled="blocked"
              @click="plan = item"
            >
              {{ item.entries[0]?.origin.name }}<ArrowRight :size="13" />
            </button>
          </div>
          <footer
            class="resource-footer list-reveal-item"
            :style="listRevealStyle(3)"
            data-list-footer
          >
            <div class="resource-import">
              <button class="text-button" :disabled="blocked" @click="pick()">
                <Upload :size="15" />{{ t("导入文件") }}</button
              ><button
                class="icon-button"
                :disabled="blocked"
                :aria-label="t('导入资源目录')"
                :title="t('导入资源目录')"
                @click="pick(true)"
              >
                <FolderOpen :size="17" />
              </button>
            </div>
            <template v-if="local || project"
              ><button
                v-if="preparing"
                class="text-button"
                @click="emit('cancel')"
              >
                {{ t("取消") }}</button
              ><button
                class="button primary"
                :disabled="!canPreview"
                @click="preview"
              >
                <LoaderCircle v-if="preparing" class="spin" :size="15" />{{
                  t("预览安装")
                }}
              </button></template
            >
            <div v-else-if="kind !== 'saves'" class="resource-pagination">
              <button
                class="icon-button"
                :disabled="offset === 0 || searching"
                :aria-label="t('上一页')"
                @click="offset = Math.max(0, offset - 20)"
              >
                <ArrowLeft :size="16" /></button
              ><small>{{ Math.floor(offset / 20) + 1 }}</small
              ><button
                class="icon-button"
                :disabled="offset + 20 >= total || searching"
                :aria-label="t('下一页')"
                @click="offset += 20"
              >
                <ArrowRight :size="16" />
              </button>
            </div>
          </footer>
        </template>
      </template>
    </ContentSwitch>
  </section>
</template>

<style scoped>
.resource-browser {
  width: min(820px, calc(100vw - 44px));
  max-width: none;
  padding: 24px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-height: calc(100vh - 48px);
  overflow: hidden;
}
.resource-flow > :deep(.content-switch-page) {
  gap: 16px;
}
.resource-header {
  margin: 0;
  gap: 12px;
  align-items: center;
  flex-shrink: 0;
}
.resource-embedded {
  width: 100%;
  flex: 1;
  min-height: 0;
  max-height: none;
  padding: 12px 24px 20px;
}
.resource-embedded .resource-main,
.resource-embedded .resource-plan-list {
  height: auto;
  min-height: 60px;
  flex: 1;
}
.resource-embedded .resource-header h2 {
  font-size: 13px;
}
.resource-header h2 {
  margin: 0;
  font-size: 18px;
}
.resource-header small {
  color: var(--muted);
  font-size: 11px;
}
.resource-close {
  margin-left: auto;
}
.resource-shared {
  font-size: 11px;
  color: var(--muted);
}
.resource-categories {
  display: flex;
  justify-content: space-evenly;
  gap: 10px;
  flex-shrink: 0;
}
.resource-categories button {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 9px 2px;
  color: var(--muted);
  background: transparent;
  border: 0;
  font-size: 12px;
  cursor: pointer;
}
.resource-categories button.active {
  color: var(--category-color);
  text-shadow: 0 0 12px var(--category-color);
}
.resource-categories button.active svg {
  filter: drop-shadow(0 0 7px var(--category-color));
}
.resource-tools,
.resource-world {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-shrink: 0;
}
.resource-tools :deep(.glass-select) {
  min-width: 130px;
}
.resource-world {
  font-size: 12px;
}
.resource-world :deep(.glass-select) {
  min-width: 180px;
}
.resource-search {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 1;
  min-width: 0;
  color: var(--muted);
}
.resource-search input {
  width: 100%;
  border: 0;
  border-bottom: 1px solid var(--line);
  border-radius: 0;
  background: transparent;
  color: var(--ink);
  padding: 8px;
  font-size: 13px;
}
.resource-main,
.resource-plan-list {
  height: 320px;
  min-height: 160px;
  overflow-y: auto;
  overflow-x: hidden;
  overscroll-behavior: contain;
  scrollbar-width: thin;
}
.resource-results {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overflow-x: hidden;
}
.resource-result {
  display: flex;
  align-items: center;
  gap: 16px;
  width: 100%;
  padding: 15px 10px;
  text-align: left;
  background: transparent;
  border: 0;
  border-bottom: 1px solid var(--line);
  color: var(--ink);
  cursor: pointer;
}
.resource-result:hover strong {
  color: var(--blue);
  text-shadow: 0 0 12px var(--blue);
}
.resource-result > svg {
  flex-shrink: 0;
  color: var(--muted);
}
.resource-result > div {
  flex: 1;
  min-width: 0;
}
.resource-result strong {
  font-size: 14px;
  font-weight: 550;
  transition:
    color 0.2s,
    text-shadow 0.2s;
}
.resource-result p {
  font-size: 11px;
  color: var(--muted);
  margin: 6px 0 0;
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.resource-result > span {
  display: flex;
  gap: 5px;
  align-items: center;
  color: var(--muted);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  flex-shrink: 0;
}
.resource-detail {
  padding: 8px 10px;
  max-width: 640px;
}
.resource-detail-title {
  display: flex;
  align-items: center;
  gap: 14px;
}
.resource-detail h3 {
  font-weight: 550;
  font-size: 22px;
  margin: 4px 0;
}
.resource-detail p {
  font-size: 13px;
  line-height: 1.7;
  color: var(--muted);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.resource-detail label {
  display: block;
  font-size: 11px;
  color: var(--muted);
  margin: 25px 0 10px;
}
.resource-detail :deep(.glass-select) {
  width: 100%;
}
.resource-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  height: 220px;
  font-size: 13px;
  color: var(--muted);
}
.resource-empty p {
  margin: 0;
}
.resource-local-row,
.resource-plan-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 15px 4px;
  border-bottom: 1px solid var(--line);
}
.resource-local-row > div,
.resource-plan-item > div {
  flex: 1;
  min-width: 0;
}
.resource-local-row strong,
.resource-plan-item strong {
  display: block;
  font-size: 13px;
  font-weight: 500;
  overflow-wrap: anywhere;
}
.resource-local-row small,
.resource-plan-item small {
  display: block;
  color: var(--muted);
  font-size: 10px;
  margin-top: 5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.resource-plan-item > span {
  font-size: 11px;
  color: var(--muted);
  flex-shrink: 0;
}
.resource-local-row :deep(.glass-select) {
  max-width: 160px;
}
.resource-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding-top: 8px;
  flex-shrink: 0;
}
.resource-footer > small {
  font-size: 11px;
  color: var(--muted);
  max-width: 60%;
}
.resource-pagination,
.resource-import {
  display: flex;
  align-items: center;
  gap: 10px;
}
.resource-pagination small {
  font-variant-numeric: tabular-nums;
  color: var(--muted);
}
.resource-import button {
  font-size: 12px;
}
.resource-conflict {
  color: #f19b9b;
  font-size: 12px;
  line-height: 1.6;
}
.resource-warning {
  font-size: 11px;
  color: var(--muted);
  line-height: 1.6;
}
.resource-success {
  display: flex;
  align-items: center;
  gap: 8px;
  color: #69dcb5;
  font-size: 13px;
}
.resource-pending {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  font-size: 11px;
  color: var(--muted);
}
.resource-pending button {
  font-size: 11px;
  max-width: 180px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.resource-browser :deep(.download-workflow) {
  overflow: auto;
  min-height: 0;
}
@media (max-height: 650px) {
  .resource-main,
  .resource-plan-list {
    height: 240px;
  }
  .resource-browser {
    gap: 12px;
    padding: 20px;
  }
}
</style>
