<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import {
  Box,
  Check,
  FileCode2,
  Image,
  LoaderCircle,
  Map,
  Package,
  Pencil,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "lucide-vue-next";
import InstanceIcon from "./InstanceIcon.vue";
import { resolveInstanceIcon } from "../instanceIcons";
import { t } from "../i18n";
import { versionDescription, versionName } from "../versionDescription";
import type {
  ContentKind,
  Instance,
  VersionContent,
  VersionContentCounts,
} from "../types";

const props = defineProps<{
  instance: Instance;
  locked: boolean;
  loadContent: (version: string, kind: ContentKind) => Promise<VersionContent>;
  loadCounts: (version: string) => Promise<VersionContentCounts>;
  rename: (version: string, name: string) => Promise<void>;
  remove: (version: string) => Promise<void>;
}>();
const emit = defineEmits<{
  close: [];
  error: [error: unknown];
  busy: [busy: boolean];
}>();
const tabs = [
  { id: "mods", label: "Mods", icon: Package },
  { id: "datapacks", label: "数据包", icon: FileCode2 },
  { id: "resourcepacks", label: "材质包", icon: Image },
  { id: "saves", label: "存档", icon: Map },
] as const;
const tab = ref<ContentKind>("mods");
const counts = ref<Partial<Record<ContentKind, number>>>({});
const activeTab = computed(() => tabs.find((item) => item.id === tab.value)!);
const selected = ref(
  props.instance.selected_version || props.instance.versions[0]?.id || "",
);
const version = computed(() =>
  props.instance.versions.find((v) => v.id === selected.value),
);
const name = ref("");
const editingName = ref(false);
const nameInput = ref<HTMLInputElement[]>([]);
const query = ref("");
const content = ref<VersionContent | null>(null);
const loading = ref(false);
const failed = ref(false);
const saving = ref(false);
const confirming = ref(false);
let request = 0;
let countRequest = 0;
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
        `${file.name} ${file.file_name} ${file.version || ""}`
          .toLocaleLowerCase()
          .includes(word),
      ),
    ) || []
  );
});
async function reload() {
  const current = ++request;
  const kind = tab.value;
  content.value = null;
  failed.value = false;
  loading.value = !!selected.value;
  if (!selected.value) return;
  try {
    const result = await props.loadContent(selected.value, kind);
    if (current === request) {
      content.value = result;
      counts.value[kind] = result.files.length;
    }
  } catch (error) {
    if (current === request) {
      failed.value = true;
      emit("error", error);
    }
  } finally {
    if (current === request) loading.value = false;
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
      if (content.value) counts.value[tab.value] = content.value.files.length;
    }
  } catch (error) {
    if (current === countRequest) emit("error", error);
  }
}
function refresh() {
  void reload();
  void reloadCounts();
}
watch(
  selected,
  () => {
    counts.value = selected.value
      ? {}
      : { mods: 0, datapacks: 0, resourcepacks: 0, saves: 0 };
    confirming.value = false;
    editingName.value = false;
    void reloadCounts();
  },
  { immediate: true },
);
watch(
  [selected, tab],
  () => {
    query.value = "";
    void reload();
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  request++;
  countRequest++;
});
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
function moveTab(direction: number, event: KeyboardEvent) {
  const index = tabs.findIndex((item) => item.id === tab.value);
  const next = (index + direction + tabs.length) % tabs.length;
  tab.value = tabs[next].id;
  (event.currentTarget as HTMLElement).parentElement
    ?.querySelectorAll<HTMLButtonElement>('[role="tab"]')
    [next]?.focus();
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
</script>

<template>
  <section class="modal glass-surface version-manager">
    <header class="modal-header">
      <h2 class="dialog-heading" tabindex="-1" autofocus>
        {{ t("管理版本") }}
      </h2>
      <div class="version-manager-instance">
        <InstanceIcon :icon="resolveInstanceIcon(instance.icon, instance.id)" />
        <span>{{ instance.name }}</span>
        <button
          class="icon-button"
          :disabled="loading || !version"
          :aria-label="t('刷新列表')"
          :title="t('刷新列表')"
          @click="refresh"
        >
          <RefreshCw :size="15" :class="{ spin: loading }" />
        </button>
      </div>
      <button
        class="icon-button"
        :disabled="saving"
        :aria-label="t('关闭对话框')"
        @click="emit('close')"
      >
        <X :size="18" />
      </button>
    </header>
    <div class="version-manager-body">
      <aside class="version-manager-sidebar">
        <nav class="version-manager-list" :aria-label="t('游戏版本')">
          <template v-for="item in instance.versions" :key="item.id">
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
              <small>{{ versionDescription(item) }}</small>
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
              class="version-manager-choice"
              :aria-current="item.id === selected ? 'true' : undefined"
              :disabled="saving"
              @click="selected = item.id"
            >
              <Box :size="18" :class="{ pending: !item.installed }" />
              <span
                ><strong :title="versionName(item)">{{
                  versionName(item)
                }}</strong
                ><small>{{ versionDescription(item) }}</small></span
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
                {{ t("版本文件将移入游戏目录的回收区，共享 Mod 和存档保留。") }}
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
          role="tablist"
          :aria-label="t('版本内容')"
        >
          <button
            v-for="item in tabs"
            :id="`version-content-tab-${item.id}`"
            :key="item.id"
            class="settings-tab"
            role="tab"
            :aria-selected="tab === item.id"
            aria-controls="version-content-panel"
            :tabindex="tab === item.id ? 0 : -1"
            @click="tab = item.id"
            @keydown.right.prevent="moveTab(1, $event)"
            @keydown.left.prevent="moveTab(-1, $event)"
          >
            {{ t(item.label)
            }}{{
              counts[item.id] === undefined ? "(…)" : `(${counts[item.id]})`
            }}
          </button>
        </div>
        <div
          id="version-content-panel"
          class="version-manager-panel"
          role="tabpanel"
          :aria-labelledby="`version-content-tab-${tab}`"
          :aria-busy="loading"
          tabindex="0"
        >
          <Transition name="settings-page" mode="out-in">
            <div :key="`${selected}:${tab}`" class="version-manager-files">
              <p v-if="!version" class="version-manager-empty">
                {{ t("暂无游戏版本") }}
              </p>
              <p v-else-if="loading" class="version-manager-empty">
                <LoaderCircle class="spin" :size="18" />{{ t("读取中") }}
              </p>
              <p v-else-if="failed" class="version-manager-empty">
                {{ t("无法读取列表") }}
              </p>
              <p
                v-else-if="!filteredFiles.length"
                class="version-manager-empty"
              >
                {{ t(query ? "没有匹配的内容" : "暂无内容") }}
              </p>
              <div
                v-for="file in filteredFiles"
                :key="file.file_name"
                class="version-manager-file"
                :class="{ disabled: !file.enabled }"
              >
                <component :is="activeTab.icon" :size="18" />
                <span class="version-manager-file-text">
                  <strong :title="file.name">{{ file.name }}</strong>
                  <small
                    v-if="file.file_name !== file.name"
                    :title="file.file_name"
                    >{{ file.file_name }}</small
                  >
                </span>
                <span
                  v-if="file.version || !file.enabled"
                  class="version-manager-file-version"
                  >{{ file.version
                  }}<small v-if="!file.enabled">{{ t("已禁用") }}</small></span
                >
              </div>
            </div>
          </Transition>
          <footer v-if="version" class="version-manager-footer">
            <small
              class="version-manager-directory"
              :title="content?.directory"
              >{{ content?.directory }}</small
            >
            <div class="version-manager-search">
              <Search :size="12" /><input
                v-model="query"
                :aria-label="t('搜索当前列表')"
                :placeholder="t('搜索')"
              />
            </div>
          </footer>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.version-manager {
  width: min(820px, calc(100vw - 48px));
  max-width: none;
}
.version-manager-instance {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  color: var(--muted);
  font-size: 12px;
}
.version-manager-instance span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 200px;
}
.version-manager-instance :deep(.instance-art) {
  width: 22px;
  height: 22px;
  flex: 0 0 22px;
}
.version-manager-body {
  display: grid;
  grid-template-columns: 220px minmax(0, 1fr);
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
  overflow: auto;
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
  flex-direction: column;
  min-width: 0;
  gap: 5px;
}
.version-manager-choice strong {
  font-size: 13px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.version-manager-choice small,
.version-manager-edit > small {
  font-size: 10px;
  line-height: 15px;
  color: var(--muted);
  overflow-wrap: anywhere;
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
.version-manager-edit > small {
  display: block;
  margin: 5px 0 0 30px;
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
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
  flex-shrink: 0;
}
.version-manager-tabs .settings-tab {
  font-size: 13px;
  white-space: nowrap;
}
.version-manager-panel {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
}
.version-manager-footer {
  display: flex;
  align-items: center;
  gap: 16px;
  flex-shrink: 0;
  padding-top: 12px;
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
  overflow: auto;
  overscroll-behavior: contain;
}
.version-manager-file {
  display: flex;
  align-items: center;
  gap: 11px;
  min-height: 58px;
  padding: 9px 2px;
}
.version-manager-file > svg {
  color: var(--blue);
  flex-shrink: 0;
  opacity: 0.75;
}
.version-manager-file-text {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
  flex: 1;
}
.version-manager-file-text strong {
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.version-manager-file-text small {
  font: 10px/14px var(--mono);
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.version-manager-file-version {
  max-width: 100px;
  overflow-wrap: anywhere;
  font: 11px/17px var(--mono);
  color: var(--muted);
  text-align: right;
}
.version-manager-file-version small {
  display: block;
  font-size: 10px;
  line-height: 16px;
}
.version-manager-file.disabled {
  opacity: 0.55;
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
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font: 9px/16px var(--mono);
  color: var(--muted);
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
  .version-manager-confirm-enter-active,
  .version-manager-confirm-leave-active {
    transition: none;
  }
}
</style>
