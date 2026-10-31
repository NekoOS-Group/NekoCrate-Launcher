<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onBeforeUpdate,
  ref,
  shallowRef,
  watch,
} from "vue";
import {
  Box,
  Play,
  Check,
  FileCode2,
  FileQuestion,
  FolderOpen,
  Braces,
  Image,
  LoaderCircle,
  LibraryBig,
  Map as MapIcon,
  Package,
  Plus,
  Sparkles,
  Pencil,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-vue-next";
import VersionMetadataPanel from "./VersionMetadataPanel.vue";
import ResourceSelectionBar from "./ResourceSelectionBar.vue";
import {
  useResourceSelection,
  changeSelectedMods,
} from "../useResourceSelection";
import {
  isResourceDragKind,
  type ResourceCopySource,
} from "../useResourceTransfer";
import VersionContentItem from "./VersionContentItem.vue";
import InstanceTarget from "./InstanceTarget.vue";
import AnimatedCount from "./AnimatedCount.vue";
import { useHorizontalLayout } from "../useHorizontalLayout";
import { listRevealStyle, leaveList, resetListLeave } from "../listMotion";
import { t } from "../i18n";
import { versionName } from "../versionDescription";
import type {
  ContentKind,
  ContentLoadOptions,
  LocalContentFile,
  ContentMetadata,
  Instance,
  VersionContent,
  VersionContentCounts,
  VersionContentTab,
  VersionMetadata,
} from "../types";

import { anchorModFiles, modFileName, modRowKeys } from "../contentIdentity";
import type { ModActions } from "../modManagement";
const props = defineProps<{
  initialVersion?: string;
  initialTab?: ContentKind;
  modActions?: ModActions;
  instance: Instance;
  instances: readonly Instance[];
  locked: boolean;
  downloading?: boolean;
  downloadVersion?: string;
  prefetchContent?: (version: string, signal: AbortSignal) => Promise<void>;
  loadContent: (
    version: string,
    kind: ContentKind,
    options?: ContentLoadOptions,
  ) => Promise<VersionContent>;
  loadContentMetadata: (
    id: string,
    version: string,
    kind: ContentKind,
    file: string,
  ) => Promise<ContentMetadata>;
  loadCounts: (version: string) => Promise<VersionContentCounts>;
  openContentFolder: (
    id: string,
    version: string,
    kind: ContentKind,
    file?: string,
    source?: string,
  ) => Promise<void>;
  loadContentSource: (
    id: string,
    version: string,
    kind: ContentKind,
    file: string,
    source: string,
  ) => Promise<string>;
  loadMetadata: (version: string) => Promise<VersionMetadata>;
  rename: (version: string, name: string) => Promise<void>;
  remove: (version: string) => Promise<void>;
}>();
const emit = defineEmits<{
  close: [];
  error: [error: unknown];
  busy: [busy: boolean];
  "select-instance": [id: string];
  "add-resources": [version: string, kind: string];
}>();
async function openDirectory() {
  if (
    !content.value?.directory ||
    tab.value === "metadata" ||
    tab.value === "libraries"
  )
    return;
  try {
    await props.openContentFolder(props.instance.id, selected.value, tab.value);
  } catch (error) {
    emit("error", error);
  }
}
const tabs = [
  { id: "metadata", label: "元数据", icon: Braces, color: "#98d6d9" },
  { id: "libraries", label: "运行依赖", icon: LibraryBig, color: "#e7aa76" },
  { id: "mods", label: "Mods", icon: Package, color: "#69dcb5" },
  { id: "datapacks", label: "数据包", icon: FileCode2, color: "#e7bc76" },
  { id: "resourcepacks", label: "资源包", icon: Image, color: "#b49aef" },
  { id: "shaderpacks", label: "光影", icon: Sparkles, color: "#e6c68a" },
  { id: "saves", label: "存档", icon: MapIcon, color: "#7dbfee" },
  { id: "unresolved", label: "待解析", icon: FileQuestion, color: "#e7bc76" },
] as const;
const tab = ref<VersionContentTab | "metadata">(props.initialTab || "metadata");
const tabBar = ref<HTMLElement | null>(null);
const header = ref<HTMLElement>();
useHorizontalLayout(header);
const metadataRefresh = ref(0);
const counts = ref<Partial<VersionContentCounts>>({});
const visibleTabs = computed(() =>
  tabs.filter(
    (item) =>
      item.id === "metadata" ||
      (counts.value[item.id] || 0) > 0 ||
      (tab.value === item.id && loading.value),
  ),
);
const activeTab = computed(() => tabs.find((item) => item.id === tab.value)!);
const selected = ref(
  props.initialVersion ||
    props.instance.selected_version ||
    props.instance.versions[0]?.id ||
    "",
);
const version = computed(() =>
  props.instance.versions.find((v) => v.id === selected.value),
);
const name = ref("");
const editingName = ref(false);
const nameInput = ref<HTMLInputElement[]>([]);
const query = ref("");
const content = shallowRef<VersionContent | null>(null);
const libraries = shallowRef<VersionMetadata["libraries"]>([]);
let contentController: AbortController | undefined;
let prefetchController: AbortController | undefined;
const rowMotions = new Map<string, ReturnType<typeof listRevealStyle>>();
const modKeys = computed(() => modRowKeys(content.value?.files || []));
function contentRowKey(file: Pick<LocalContentFile, "file_name">) {
  return tab.value === "mods"
    ? modKeys.value.get(file.file_name) || file.file_name
    : file.file_name;
}
function contentRevealStyle(
  file: Pick<LocalContentFile, "file_name">,
  index: number,
) {
  const key = contentRowKey(file);
  let style = rowMotions.get(key);
  if (!style) {
    style = listRevealStyle(index, 0);
    rowMotions.set(key, style);
  }
  return style;
}
const contentRows = new Map<string, { reveal: () => Promise<void> }>();
const dependencyTargets = computed(() => {
  const targets = new Map<string, string>();
  const files = content.value?.files || [];
  // Prefer an enabled provider when a disabled copy is also present.
  for (const enabled of [true, false]) {
    for (const file of files) {
      if (file.enabled !== enabled) continue;
      for (const id of file.mod_ids || []) {
        if (!targets.has(id)) targets.set(id, file.file_name);
      }
    }
  }
  return targets;
});
function setContentRow(file: string, row: unknown) {
  if (row) contentRows.set(file, row as { reveal: () => Promise<void> });
  else contentRows.delete(file);
}
async function showMod(id: string, version: string, file: string) {
  if (
    id !== props.instance.id ||
    version !== selected.value ||
    tab.value !== "mods" ||
    !content.value?.files.some((item) => item.file_name === file)
  )
    return;
  const scope = request;
  query.value = "";
  await nextTick();
  if (
    scope === request &&
    id === props.instance.id &&
    version === selected.value &&
    tab.value === "mods"
  )
    await contentRows.get(file)?.reveal();
}
const loading = ref(false);
const failed = ref(false);
const saving = ref(false);
const confirming = ref(false);
let request = 0;
let countRequest = 0;
watch(
  () => props.instance.id,
  () => {
    selected.value =
      props.instance.selected_version || props.instance.versions[0]?.id || "";
  },
);
watch(
  () => props.instance.versions.map((v) => v.id),
  (ids) => {
    if (!ids.includes(selected.value)) selected.value = ids[0] || "";
  },
);
watch(
  () => version.value && versionName(version.value),
  (value) => {
    name.value = value || "";
  },
  { immediate: true },
);
const dirty = computed(
  () => !!version.value && name.value.trim() !== versionName(version.value),
);
const validName = computed(
  () =>
    !!name.value.trim() &&
    [...name.value.trim()].length <= 80 &&
    !/[\u0000-\u001f\u007f]/.test(name.value),
);
const filteredFiles = computed(() => {
  const words = query.value
    .trim()
    .toLocaleLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  return (
    content.value?.files.filter((file) =>
      words.every((word) =>
        `${file.name} ${file.file_name} ${file.version || ""} ${(file.mod_ids || []).join(" ")}`
          .toLocaleLowerCase()
          .includes(word),
      ),
    ) || []
  );
});
const selection = useResourceSelection(
  () =>
    (isResourceDragKind(tab.value) ? filteredFiles.value : []).filter(
      (file) => !file.declaration || file.declaration.status === "local",
    ),
  contentRowKey,
);
const batchSaving = ref(false);
const selectedFiles = selection.selected;
const selectedHasEnabled = computed(() =>
  selectedFiles.value.some((file) => file.enabled),
);
const selectedHasDisabled = computed(() =>
  selectedFiles.value.some((file) => !file.enabled),
);
watch([selected, tab, query, () => props.instance.id], () => selection.clear());
function selectResource(event: MouseEvent, file: LocalContentFile) {
  return batchSaving.value || selection.click(event, file);
}
function selectionKeyboard(event: KeyboardEvent) {
  if (!batchSaving.value && isResourceDragKind(tab.value))
    selection.keyboard(event);
}
function selectedDragSources(file: LocalContentFile): ResourceCopySource[] {
  if (!isResourceDragKind(tab.value)) return [];
  const kind = tab.value;
  return selection
    .drag(file)
    .map((item) => ({
      kind,
      instance: props.instance.id,
      version: selected.value,
      file: item.file_name,
      name: item.name,
    }));
}
async function setSelectedEnabled(enabled: boolean) {
  if (
    props.locked ||
    batchSaving.value ||
    !props.modActions ||
    tab.value !== "mods"
  )
    return;
  const id = props.instance.id,
    version = selected.value,
    scope = request;
  const files = [...selectedFiles.value];
  batchSaving.value = true;
  emit("busy", true);
  try {
    const failures = await changeSelectedMods(
      files,
      enabled,
      (f) => f.enabled,
      (f, state) =>
        props.modActions!.setEnabled(id, version, f.file_name, state),
      (file, name) => {
        if (scope !== request || !content.value) return;
        content.value = {
          ...content.value,
          files: content.value.files.map((item) =>
            item.file_name === file.file_name
              ? { ...item, file_name: name, enabled }
              : item,
          ),
        };
      },
    );
    if (scope === request) await reload(true);
    if (failures.length)
      emit(
        "error",
        failures
          .map(({ item, error }) => `${item.name}: ${String(error)}`)
          .join("\n"),
      );
  } finally {
    batchSaving.value = false;
    emit("busy", false);
  }
}
const filteredLibraries = computed(() => {
  const words = query.value
    .trim()
    .toLocaleLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  return libraries.value.filter((library) =>
    words.every((word) =>
      (library.name + " " + (library.detail || ""))
        .toLocaleLowerCase()
        .includes(word),
    ),
  );
});
function ensureVisibleTab() {
  if (tab.value === "metadata" || counts.value[tab.value] !== 0) return;
  // Keep the default metadata page available even for an empty/pending version.
  tab.value =
    tabs.find(
      (item) => item.id !== "metadata" && (counts.value[item.id] || 0) > 0,
    )?.id || "metadata";
}
async function contentChanged(file: LocalContentFile, name: string) {
  if (
    tab.value === "mods" &&
    content.value &&
    name !== file.file_name &&
    modFileName(name) === modFileName(file.file_name)
  ) {
    content.value = {
      ...content.value,
      files: content.value.files.map((item) =>
        item.file_name === file.file_name
          ? { ...item, file_name: name, enabled: /\.jar$/i.test(name) }
          : item,
      ),
    };
  }
  await reload(true);
}
async function reload(preserve = false) {
  const current = ++request;
  contentController?.abort();
  contentController = new AbortController();
  const kind = tab.value;
  const previousFiles =
    preserve && kind === "mods" ? content.value?.files : undefined;
  const reconcile = (result: VersionContent) =>
    previousFiles
      ? { ...result, files: anchorModFiles(previousFiles, result.files) }
      : result;
  if (!preserve) {
    content.value = null;
    libraries.value = [];
    contentRows.clear();
  }
  failed.value = false;
  loading.value = !!selected.value && kind !== "metadata";
  if (!selected.value || kind === "metadata") return;
  try {
    if (kind === "libraries") {
      const result = await props.loadMetadata(selected.value);
      if (current === request) {
        libraries.value = result.libraries || [];
        counts.value.libraries = libraries.value.length;
      }
      return;
    }
    const result = await props.loadContent(selected.value, kind, {
      fresh: preserve,
      signal: contentController.signal,
      onProgress: (result) => {
        if (current !== request) return;
        content.value = reconcile(result);
        counts.value[kind] = result.files.length;
      },
    });
    if (current === request) {
      content.value = reconcile(result);
      counts.value[kind] = result.files.length;
    }
  } catch (error) {
    if (current === request) {
      failed.value = true;
      emit("error", error);
    }
  } finally {
    if (current === request) {
      loading.value = false;
      if (!failed.value) ensureVisibleTab();
    }
  }
}
async function reloadCounts() {
  const current = ++countRequest;
  if (!selected.value) return;
  try {
    const result = await props.loadCounts(selected.value);
    if (current === countRequest) {
      counts.value = result;
      // An already loaded list is more recent than the initial directory scan.
      if (
        content.value &&
        !loading.value &&
        tab.value !== "metadata" &&
        tab.value !== "libraries"
      )
        counts.value[tab.value] = content.value.files.length;
      // A tab selected in the same update may still be waiting for its watcher.
      await nextTick();
      if (current === countRequest && !loading.value) ensureVisibleTab();
    }
  } catch (error) {
    if (current === countRequest) emit("error", error);
  }
}
function refresh() {
  metadataRefresh.value++;
  void reload();
  void reloadCounts();
}
let pollTimer: ReturnType<typeof setTimeout> | undefined;
let pollTicket = 0;
watch(
  [
    () => props.downloading,
    () => props.downloadVersion,
    selected,
    () => props.instance.id,
  ],
  (_value, previous) => {
    clearTimeout(pollTimer);
    const ticket = ++pollTicket;
    const active = () =>
      ticket === pollTicket &&
      props.downloading &&
      props.downloadVersion === selected.value;
    const update = async () => {
      await Promise.all([reload(true), reloadCounts()]);
      if (ticket !== pollTicket) return;
      metadataRefresh.value++;
      if (active()) pollTimer = setTimeout(() => void update(), 1000);
    };
    if (active()) pollTimer = setTimeout(() => void update(), 500);
    else if (
      previous?.[0] &&
      previous[1] === selected.value &&
      previous[3] === props.instance.id
    )
      void update();
  },
  { immediate: true },
);
watch(
  [selected, () => props.instance.id],
  () => {
    // Keep the previous counts until the new scan arrives, including on failure.
    if (!selected.value)
      counts.value = {
        mods: 0,
        datapacks: 0,
        resourcepacks: 0,
        saves: 0,
        unresolved: 0,
        libraries: 0,
      };
    confirming.value = false;
    editingName.value = false;
    prefetchController?.abort();
    prefetchController = new AbortController();
    if (selected.value && props.prefetchContent)
      void props
        .prefetchContent(selected.value, prefetchController.signal)
        .catch(() => {
          // The visible page reports a read error if needed; speculative work stays quiet.
        });
    void reloadCounts();
  },
  { immediate: true },
);
watch(
  [selected, tab, () => props.instance.id],
  () => {
    query.value = "";
    rowMotions.clear();
    void reload();
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  pollTicket++;
  clearTimeout(pollTimer);
  request++;
  countRequest++;
  contentController?.abort();
  prefetchController?.abort();
});
function chooseInstance(id: string | null) {
  if (!saving.value && id && id !== props.instance.id)
    emit("select-instance", id);
}
async function startRename() {
  if (!version.value || saving.value) return;
  name.value = versionName(version.value);
  confirming.value = false;
  editingName.value = true;
  await nextTick();
  nameInput.value[0]?.focus();
  nameInput.value[0]?.select();
}
function cancelRename() {
  if (saving.value) return;
  editingName.value = false;
  name.value = versionName(version.value);
}
const tabPositions = new WeakMap<
  Element,
  { left: number; top: number; width: number }
>();
onBeforeUpdate(() => {
  const bar = tabBar.value;
  if (!bar) return;
  const origin = bar.getBoundingClientRect();
  for (const button of bar.querySelectorAll<HTMLElement>('[role="tab"]')) {
    const bounds = button.getBoundingClientRect();
    tabPositions.set(button, {
      left: bounds.left - origin.left,
      top: bounds.top - origin.top,
      width: bounds.width,
    });
  }
});
function pinLeavingTab(element: Element) {
  const button = element as HTMLElement;
  // Measure before the patch so multiple disappearing tabs keep their original
  // positions, rather than inheriting intermediate flex layouts.
  const bounds = tabPositions.get(button) || {
    left: button.offsetLeft,
    top: button.offsetTop,
    width: button.offsetWidth,
  };
  button.style.left = bounds.left + "px";
  button.style.top = bounds.top + "px";
  button.style.width = bounds.width + "px";
  const focused = document.activeElement === button;
  button.inert = true;
  if (focused)
    void nextTick(() =>
      tabBar.value
        ?.querySelector<HTMLButtonElement>('[aria-selected="true"]')
        ?.focus({ preventScroll: true }),
    );
}
function resetLeavingTab(element: Element) {
  const button = element as HTMLElement;
  for (const property of ["left", "top", "width"])
    button.style.removeProperty(property);
  button.inert = false;
}
function moveTab(direction: number, event: KeyboardEvent) {
  const items = visibleTabs.value;
  const index = items.findIndex((item) => item.id === tab.value);
  const next = (index + direction + items.length) % items.length;
  tab.value = items[next].id;
  (event.currentTarget as HTMLElement).parentElement
    ?.querySelector<HTMLButtonElement>("#version-content-tab-" + items[next].id)
    ?.focus();
}
async function showContent(kind: VersionContentTab) {
  tab.value = kind;
  await nextTick();
  tabBar.value
    ?.querySelector<HTMLButtonElement>("#version-content-tab-" + kind)
    ?.focus({ preventScroll: true });
}
async function change(remove = false) {
  if (
    saving.value ||
    !version.value ||
    (remove ? props.locked : !validName.value || !dirty.value)
  )
    return;
  const id = version.value.id;
  const value = name.value.trim();
  saving.value = true;
  emit("busy", true);
  try {
    if (remove) await props.remove(id);
    else await props.rename(id, value);
    confirming.value = false;
    editingName.value = false;
  } catch (error) {
    emit("error", error);
  } finally {
    saving.value = false;
    emit("busy", false);
  }
}
defineExpose({ refresh });
</script>

<template>
  <section
    class="modal glass-surface version-manager"
    :data-resource-instance="instance.id"
    :data-resource-version="selected"
    data-drop-kind="version"
    @keydown="selectionKeyboard"
  >
    <header ref="header" class="modal-header">
      <h2 class="dialog-heading" tabindex="-1" autofocus>
        {{ t("管理版本") }}
      </h2>
      <Play
        class="version-manager-separator"
        :size="9"
        fill="currentColor"
        :stroke-width="0"
        aria-hidden="true"
      />
      <div class="version-manager-instance">
        <InstanceTarget
          context="manage"
          :target="instance"
          :instances="instances"
          :disabled="saving"
          @select="chooseInstance"
        />
      </div>
      <div class="version-manager-header-actions">
        <button
          class="icon-button"
          :disabled="locked || !version"
          :title="t('添加资源')"
          :aria-label="t('添加资源')"
          @click="emit('add-resources', selected, tab)"
        >
          <Plus :size="18" />
        </button>
        <button
          class="icon-button"
          :disabled="loading || !version"
          :aria-label="t('刷新列表')"
          :title="t('刷新列表')"
          @click="refresh"
        >
          <RefreshCw :size="18" :class="{ spin: loading }" />
        </button>
        <button
          class="icon-button"
          :disabled="saving"
          :aria-label="t('关闭对话框')"
          :title="t('关闭对话框')"
          @click="emit('close')"
        >
          <X :size="18" />
        </button>
      </div>
    </header>
    <div class="version-manager-body">
      <aside class="version-manager-sidebar">
        <nav class="version-manager-list" :aria-label="t('游戏版本')">
          <template
            v-for="(item, index) in instance.versions"
            :key="`${instance.id}:${item.id}`"
          >
            <form
              v-if="item.id === selected && editingName"
              class="version-manager-edit"
              @submit.prevent="change()"
              @keydown.esc.stop.prevent="cancelRename"
            >
              <div class="version-manager-name">
                <Box :size="18" />
                <input
                  ref="nameInput"
                  v-model="name"
                  :aria-label="t('子实例名称')"
                  :disabled="saving"
                  maxlength="80"
                  :placeholder="item.id"
                />
              </div>
              <div class="version-manager-edit-actions">
                <button
                  class="icon-button"
                  type="button"
                  :disabled="saving"
                  :aria-label="t('取消')"
                  :title="t('取消')"
                  @click="cancelRename"
                >
                  <X :size="15" />
                </button>
                <button
                  class="icon-button"
                  :disabled="saving || !dirty || !validName"
                  :aria-label="t('保存名称')"
                  :title="t('保存名称')"
                >
                  <LoaderCircle v-if="saving" :size="15" class="spin" /><Check
                    v-else
                    :size="15"
                  />
                </button>
              </div>
            </form>
            <button
              v-else
              class="version-manager-choice list-reveal-item"
              :data-resource-instance="instance.id"
              :data-resource-version="item.id"
              data-drop-kind="version"
              :style="listRevealStyle(index)"
              :aria-current="item.id === selected ? 'true' : undefined"
              :disabled="saving"
              @click="selected = item.id"
            >
              <Box :size="18" :class="{ pending: !item.installed }" />
              <span
                ><strong :title="versionName(item)">{{
                  versionName(item)
                }}</strong
                ><small v-if="!item.installed">{{ t("待下载") }}</small></span
              >
            </button>
          </template>
          <p v-if="!instance.versions.length" class="version-manager-empty">
            {{ t("暂无游戏版本") }}
          </p>
        </nav>
        <footer v-if="version" class="version-manager-actions">
          <Transition name="version-manager-confirm" mode="out-in">
            <div
              v-if="confirming"
              key="confirm"
              class="version-manager-confirmation"
            >
              <p>
                {{ t("版本文件将移入系统回收站，共享 Mod 和存档保留。") }}
              </p>
              <div>
                <button
                  class="text-button"
                  :disabled="saving"
                  @click="confirming = false"
                >
                  {{ t("取消") }}
                </button>
                <button
                  class="text-button danger"
                  :disabled="locked || saving"
                  @click="change(true)"
                >
                  <LoaderCircle v-if="saving" class="spin" :size="14" />{{
                    t("确认删除")
                  }}
                </button>
              </div>
            </div>
            <div v-else key="actions" class="version-manager-action-row">
              <button
                class="text-button"
                :disabled="saving || editingName"
                @click="startRename"
              >
                <Pencil :size="14" />{{ t("改名") }}
              </button>
              <button
                class="text-button danger"
                :disabled="locked || saving"
                :title="locked ? t('请先结束游戏或当前任务') : undefined"
                @click="
                  cancelRename();
                  confirming = true;
                "
              >
                <Trash2 :size="14" />{{ t("删除") }}
              </button>
            </div>
          </Transition>
        </footer>
      </aside>
      <div class="version-manager-detail">
        <div
          class="version-manager-tabs"
          ref="tabBar"
          role="tablist"
          :aria-label="t('版本内容')"
        >
          <TransitionGroup
            name="version-tab"
            @before-leave="pinLeavingTab"
            @after-leave="resetLeavingTab"
            @leave-cancelled="resetLeavingTab"
          >
            <button
              v-for="item in visibleTabs"
              :id="`version-content-tab-${item.id}`"
              :data-drop-kind="item.id"
              :key="item.id"
              class="settings-tab"
              :style="{ '--tab-color': item.color }"
              role="tab"
              :aria-selected="tab === item.id"
              aria-controls="version-content-panel"
              :tabindex="tab === item.id ? 0 : -1"
              @click="tab = item.id"
              @keydown.right.prevent="moveTab(1, $event)"
              @keydown.left.prevent="moveTab(-1, $event)"
            >
              <span class="version-tab-content">
                <component :is="item.icon" :size="15" aria-hidden="true" />
                <span class="version-tab-label"
                  >{{ t(item.label)
                  }}<AnimatedCount
                    v-if="item.id !== 'metadata'"
                    :value="counts[item.id]"
                /></span>
              </span>
            </button>
          </TransitionGroup>
        </div>
        <div
          id="version-content-panel"
          :data-drop-kind="tab"
          class="version-manager-panel"
          role="tabpanel"
          :aria-labelledby="`version-content-tab-${tab}`"
          :aria-busy="loading"
          tabindex="0"
        >
          <div class="version-content-stage">
            <Transition
              name="version-content-fade"
              @before-enter="resetListLeave"
              @leave="leaveList"
              @leave-cancelled="resetListLeave"
            >
              <VersionMetadataPanel
                v-if="tab === 'metadata' && version"
                :key="`metadata:${instance.id}:${selected}`"
                :version="selected"
                :refresh="metadataRefresh"
                :load-metadata="loadMetadata"
                @show-content="showContent"
                @error="emit('error', $event)"
              />
              <div
                v-else
                :key="`${instance.id}:${selected}:${tab}`"
                class="version-content-page"
                :style="{ '--tab-color': activeTab.color }"
              >
                <div class="version-manager-files" data-list-viewport>
                  <p v-if="!version" class="version-manager-empty">
                    {{ t("暂无游戏版本") }}
                  </p>
                  <p
                    v-else-if="failed && !content && !libraries.length"
                    class="version-manager-empty"
                  >
                    {{ t("无法读取列表") }}
                  </p>
                  <p
                    v-else-if="
                      !loading &&
                      !failed &&
                      !(tab === 'libraries'
                        ? filteredLibraries.length
                        : filteredFiles.length)
                    "
                    class="version-manager-empty"
                  >
                    {{ t(query ? "没有匹配的内容" : "暂无内容") }}
                  </p>
                  <template v-if="tab === 'libraries'">
                    <article
                      v-for="(library, index) in filteredLibraries"
                      :key="library.name"
                      class="version-library-item list-reveal-item"
                      :style="
                        contentRevealStyle({ file_name: library.name }, index)
                      "
                    >
                      <LibraryBig :size="18" aria-hidden="true" />
                      <span>
                        <strong :title="library.name">{{
                          library.name
                        }}</strong>
                        <small v-if="library.detail" :title="library.detail">{{
                          library.detail
                        }}</small>
                      </span>
                    </article>
                  </template>
                  <template v-else>
                    <VersionContentItem
                      class="list-reveal-item"
                      :style="contentRevealStyle(file, index)"
                      v-for="(file, index) in filteredFiles"
                      :key="`${instance.id}:${selected}:${tab}:${contentRowKey(file)}`"
                      :ref="(row) => setContentRow(file.file_name, row)"
                      :file="file"
                      :instance-id="instance.id"
                      :version="selected"
                      :kind="tab as ContentKind"
                      :icon="activeTab.icon"
                      :mod-actions="modActions"
                      :locked="locked || batchSaving"
                      :selected="selection.contains(file)"
                      :select="selectResource"
                      :drag-sources="() => selectedDragSources(file)"
                      @changed="(name) => contentChanged(file, name)"
                      :load-metadata="loadContentMetadata"
                      :open-folder="openContentFolder"
                      :load-source="loadContentSource"
                      :dependency-targets="dependencyTargets"
                      @show-mod="showMod"
                      @error="emit('error', $event)"
                    />
                  </template>
                </div>
                <footer
                  v-if="version"
                  class="version-manager-footer"
                  data-list-footer
                >
                  <button
                    v-if="content?.directory"
                    type="button"
                    class="version-manager-directory"
                    :title="content?.directory"
                    :disabled="!content?.directory"
                    @click="openDirectory"
                  >
                    <FolderOpen :size="12" /><span>{{
                      content?.directory
                    }}</span>
                  </button>
                  <ResourceSelectionBar
                    :count="selectedFiles.length"
                    :mods="tab === 'mods'"
                    :busy="locked || batchSaving"
                    :enable="selectedHasDisabled"
                    :disable="selectedHasEnabled"
                    @clear="selection.clear"
                    @enabled="setSelectedEnabled"
                  />
                  <div class="version-manager-search">
                    <Search :size="12" /><input
                      v-model="query"
                      :aria-label="t('搜索当前列表')"
                      :placeholder="t('搜索')"
                    />
                  </div>
                </footer>
              </div>
            </Transition>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.version-manager {
  width: min(940px, calc(100vw - 48px));
  max-width: none;
}
.version-manager .modal-header {
  justify-content: flex-start;
  gap: 10px;
  margin-bottom: 0;
}
.version-manager .modal-header h2 {
  flex: 0 0 auto;
}
.version-manager-header-actions {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.version-manager-separator {
  color: var(--muted);
  flex-shrink: 0;
}
.version-manager-instance {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  color: var(--muted);
  font-size: 12px;
}
.version-manager-instance :deep(.instance-target-trigger .instance-art) {
  width: 22px;
  height: 22px;
  flex: 0 0 22px;
}
.version-manager-body {
  display: grid;
  grid-template-columns: 200px minmax(0, 1fr);
  height: min(470px, calc(100vh - 160px));
  min-height: 220px;
}
.version-manager-sidebar {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  border-right: 1px solid var(--line);
}
.version-manager-list {
  flex: 1;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  padding: 6px 12px 12px 16px;
}
.version-manager-choice {
  display: flex;
  width: 100%;
  gap: 12px;
  align-items: center;
  border: 0;
  background: transparent;
  color: var(--muted);
  padding: 14px 8px;
  text-align: left;
  transition:
    color 160ms,
    text-shadow 160ms;
}
.version-manager-choice > svg {
  flex-shrink: 0;
}
.version-manager-choice .pending {
  stroke-dasharray: 3 2;
}
.version-manager-choice > span {
  display: flex;
  flex: 1;
  align-items: center;
  min-width: 0;
  gap: 8px;
}
.version-manager-choice strong {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.version-manager-choice small {
  flex-shrink: 0;
  white-space: nowrap;
  color: var(--muted);
  font-size: 10px;
  line-height: 15px;
  text-shadow: none;
}
.version-manager-choice[aria-current="true"] {
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
}
.version-manager-choice[aria-current="true"] > svg {
  fill: color-mix(in srgb, var(--blue) 24%, transparent);
  filter: drop-shadow(0 0 5px var(--glow));
}
.version-manager-choice:hover {
  color: var(--ink);
}
.version-manager-edit {
  padding: 9px 8px;
}
.version-manager-name {
  display: flex;
  align-items: center;
  gap: 12px;
  color: var(--blue);
}
.version-manager-name > svg {
  flex-shrink: 0;
}
.version-manager-name input {
  min-width: 0;
  width: 100%;
  background: transparent;
  border: 0;
  border-bottom: 1px solid var(--blue);
  border-radius: 0;
  padding: 5px 0;
  font-size: 13px;
  line-height: 20px;
  outline: none;
  box-shadow: none;
}
.version-manager-edit-actions {
  display: flex;
  justify-content: flex-end;
  gap: 4px;
  margin-top: 4px;
}
.version-manager-edit-actions .icon-button {
  width: 28px;
  height: 28px;
}
.version-manager-actions {
  padding: 16px 24px 20px;
}
.version-manager-action-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
}
.version-manager-actions .text-button {
  font-size: 12px;
  gap: 6px;
}
.version-manager-confirmation p {
  color: var(--muted);
  font-size: 11px;
  line-height: 18px;
  margin: 0 0 12px;
}
.version-manager-confirmation > div {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
}
.version-manager-detail {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  padding: 4px 24px 16px;
  overflow: hidden;
}
.version-manager-tabs {
  position: relative;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-evenly;
  gap: 12px;
  margin: 8px 0 4px;
  flex-shrink: 0;
  min-height: 36px;
}
.version-tab-move,
.version-tab-enter-active,
.version-tab-leave-active {
  transition:
    transform 420ms cubic-bezier(0.16, 1, 0.3, 1),
    opacity 220ms ease-out;
}
.version-tab-enter-from,
.version-tab-leave-to {
  opacity: 0;
  transform: translateX(12px);
}
.version-tab-leave-active {
  position: absolute;
  pointer-events: none;
}
.version-manager-tabs .settings-tab {
  display: inline-flex;
  align-items: center;
  flex: 0 0 auto;
  height: 36px;
  padding-inline: 0;
  font-size: 12px;
  white-space: nowrap;
}
.version-tab-content {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  line-height: 24px;
  /* Keep icon and label on the same composited plane during width changes. */
  transform: translateZ(0);
}
.version-manager-tabs .settings-tab svg {
  flex-shrink: 0;
  color: var(--tab-color);
  opacity: 0.65;
  transition:
    opacity 180ms,
    filter 180ms;
}
.version-manager-tabs .settings-tab:hover,
.version-manager-tabs .settings-tab[aria-selected="true"] {
  color: var(--tab-color);
  text-shadow: 0 0 12px color-mix(in srgb, var(--tab-color) 50%, transparent);
}
.version-manager-tabs .settings-tab[aria-selected="true"] svg {
  opacity: 1;
  filter: drop-shadow(
    0 0 5px color-mix(in srgb, var(--tab-color) 60%, transparent)
  );
}
.version-manager-panel {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
}
.version-library-item {
  display: flex;
  align-items: center;
  gap: 11px;
  min-height: 58px;
  padding: 9px 2px;
}
.version-library-item > svg {
  flex-shrink: 0;
  color: var(--tab-color);
  opacity: 0.75;
}
.version-library-item > span {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  gap: 5px;
}
.version-library-item strong,
.version-library-item small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  user-select: text;
}
.version-library-item strong {
  font: 12px/18px var(--mono);
  color: var(--ink);
}
.version-library-item small {
  font: 10px/15px var(--mono);
  color: var(--muted);
}
.version-manager-footer {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-shrink: 0;
  padding-top: 12px;
}
.version-content-stage {
  display: grid;
  grid-template-rows: minmax(0, 1fr);
  flex: 1;
  min-height: 0;
  min-width: 0;
  overflow: hidden;
}
.version-content-stage > * {
  grid-area: 1 / 1;
  min-height: 0;
}
.version-content-page {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}
.version-content-fade-leave-active {
  pointer-events: none;
}
.version-manager-search {
  display: flex;
  align-items: center;
  gap: 7px;
  width: 130px;
  flex-shrink: 0;
  color: var(--muted);
}
.version-manager-search input {
  min-width: 0;
  width: 100%;
  border: 0;
  background: transparent;
  border-radius: 0;
  padding: 4px 0;
  font-size: 10px;
  line-height: 16px;
  box-shadow: none;
}
.version-manager-files {
  flex: 1;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
}
.version-manager-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--muted);
  font-size: 12px;
  padding: 28px 12px;
}
.version-manager-directory {
  display: flex;
  align-items: center;
  gap: 6px;
  border: 0;
  background: transparent;
  padding: 0;
  text-align: left;
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font: 9px/16px var(--mono);
  color: var(--muted);
}
.version-manager-directory svg {
  flex-shrink: 0;
}
.version-manager-directory span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.version-manager-directory:hover {
  color: var(--tab-color, var(--blue));
}
.version-manager-confirm-enter-active,
.version-manager-confirm-leave-active {
  transition:
    opacity 140ms,
    transform 180ms var(--motion-ease);
}
.version-manager-confirm-enter-from,
.version-manager-confirm-leave-to {
  opacity: 0;
  transform: translateY(4px);
}
@media (max-width: 660px) {
  .version-manager-body {
    grid-template-columns: 165px minmax(0, 1fr);
  }
  .version-manager-detail {
    padding-inline: 16px;
  }
  .version-manager-list {
    padding-inline: 8px;
  }
  .version-manager-actions {
    padding-inline: 16px;
  }
  .version-manager-search {
    width: 110px;
  }
  .version-manager-tabs {
    gap: 8px;
  }
  .version-manager-tabs .settings-tab {
    font-size: 12px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .version-tab-move,
  .version-tab-enter-active,
  .version-tab-leave-active,
  .version-manager-confirm-enter-active,
  .version-manager-confirm-leave-active {
    transition: none;
  }
}
.version-tab-label {
  display: inline-flex;
  align-items: center;
  height: 24px;
  line-height: 24px;
}
</style>
