<script setup lang="ts">
import {
  computed,
  defineAsyncComponent,
  h,
  inject,
  nextTick,
  onBeforeUnmount,
  ref,
  watch,
  type Component,
} from "vue";
import {
  Braces,
  ChevronRight,
  LoaderCircle,
  Settings2,
  Save,
  AlignLeft,
} from "lucide-vue-next";
import { modFileName } from "../contentIdentity";
import {
  resourceTransferKey,
  isResourceCopyKind,
} from "../useResourceTransfer";
import GlassSelect from "./GlassSelect.vue";
import { locale, t, translateMessage } from "../i18n";
import { formatBytes } from "../downloadWorkflow";
import type { ContentKind, ContentMetadata, LocalContentFile } from "../types";
import type { ConfigFormat } from "../configSyntax";

const ConfigEditor = defineAsyncComponent({
  loader: () => import("./ConfigEditor.vue"),
  loadingComponent: {
    render: () => h("div", { class: "mod-config-editor-loading" }),
  },
  delay: 0,
});

import {
  configDrafts,
  configKey,
  type ModActions,
  type ModConfigFile,
  type ModConfigDocument,
} from "../modManagement";
const props = defineProps<{
  modActions?: ModActions;
  locked?: boolean;
  file: LocalContentFile;
  instanceId: string;
  version: string;
  kind: ContentKind;
  icon: Component;
  loadMetadata: (
    id: string,
    version: string,
    kind: ContentKind,
    file: string,
  ) => Promise<ContentMetadata>;
  openFolder: (
    id: string,
    version: string,
    kind: ContentKind,
    file: string,
    source?: string,
  ) => Promise<void>;
  loadSource: (
    id: string,
    version: string,
    kind: ContentKind,
    file: string,
    source: string,
  ) => Promise<string>;
  dependencyTargets: ReadonlyMap<string, string>;
}>();
const emit = defineEmits<{
  error: [error: unknown];
  changed: [file: string];
  "show-mod": [id: string, version: string, file: string];
}>();
const row = ref<HTMLElement | null>(null);
const transferResource = inject(resourceTransferKey, undefined);
const draggableResource = computed(
  () =>
    !!transferResource &&
    isResourceCopyKind(props.kind) &&
    available.value &&
    !props.locked &&
    !modSaving.value &&
    !modDownloading.value,
);
function beginResourceDrag(event: PointerEvent) {
  if (!draggableResource.value || !isResourceCopyKind(props.kind)) return;
  transferResource?.(event, {
    kind: props.kind,
    instance: props.instanceId,
    version: props.version,
    file: props.file.file_name,
    name: displayName.value,
  });
}
const highlighted = ref(false);
let highlightTimer: ReturnType<typeof setTimeout> | undefined;
const rawSources = ref<
  Record<string, { text?: string; loading: boolean; failed: boolean }>
>({});
const expanded = ref(false);
const metadata = ref<ContentMetadata | null>(null);
const archivePrefix = computed(() =>
  metadata.value?.size != null
    ? `${props.file.file_name.split("/").pop()}::`
    : "",
);
const loading = ref(false);
const failed = ref(false);
const openingFolder = ref(false);
const available = computed(
  () => !props.file.declaration || props.file.declaration.status === "local",
);
const canExpand = computed(() =>
  !available.value
    ? !!props.file.declaration
    : props.file.has_metadata === false
      ? false
      : metadata.value
        ? metadata.value.sources.length > 0
        : props.file.has_metadata === true,
);
const statusLabel = computed(() => {
  const status = props.file.declaration?.status;
  return status
    ? {
        unresolved: "待解析",
        pending: "待下载",
        partial: "部分下载",
        local: "已下载",
        manual: "需手动下载",
        skipped: "未选择",
      }[status]
    : "";
});
const progress = computed(() => {
  const entry = props.file.declaration;
  return entry?.total
    ? Math.min(100, (entry.downloaded / entry.total) * 100)
    : 0;
});
const hasDisplayName = computed(
  () =>
    !!props.file.declaration ||
    ["mods", "resourcepacks"].includes(props.kind) ||
    props.file.name !== props.file.file_name.split("/").pop(),
);
const displayName = computed(() => {
  if (
    ["mods", "resourcepacks"].includes(props.kind) &&
    props.file.name === props.file.file_name.split("/").pop()
  ) {
    return (
      props.file.name.replace(/\.jar(?:\.(?:disabled|off))?$|\.zip$/i, "") ||
      props.file.name
    );
  }
  return props.file.name;
});
const isMod = computed(() => props.kind === "mods");
const confirmedEnabled = ref<boolean>();
const modEnabled = computed(() => confirmedEnabled.value ?? props.file.enabled);
const displayFileName = computed(() =>
  isMod.value ? modFileName(props.file.file_name) : props.file.file_name,
);
const fileLabel = computed(() =>
  isMod.value && !modEnabled.value
    ? `${displayFileName.value} · ${t("已禁用")}`
    : displayFileName.value,
);
const modDownloading = computed(
  () =>
    !!props.file.declaration &&
    !!props.modActions?.downloading?.(
      props.instanceId,
      props.version,
      props.file.declaration.key,
    ),
);
const modState = computed(() =>
  !available.value ? "missing" : modEnabled.value ? "enabled" : "disabled",
);
const modActionLabel = computed(() => {
  if (modDownloading.value) return t("下载中");
  if (modSaving.value && !available.value) return t("下载中");
  if (!available.value) return t("未下载 · 点击下载");
  return t(modEnabled.value ? "已启用 · 点击禁用" : "已禁用 · 点击启用");
});
async function activateMod() {
  if (props.locked || modSaving.value || modDownloading.value) return;
  if (available.value) {
    await changeEnabled();
    return;
  }
  const declaration = props.file.declaration;
  if (!declaration || !props.modActions?.download) return;
  const identity = panelId.value;
  modSaving.value = true;
  try {
    await props.modActions.download(
      props.instanceId,
      props.version,
      declaration.key,
    );
  } catch (error) {
    emit("error", error);
  } finally {
    modSaving.value = false;
    if (identity === panelId.value) emit("changed", props.file.file_name);
  }
}
const configOpen = ref(false);
const configFiles = ref<ModConfigFile[]>([]);
const configDocument = ref<ModConfigDocument | null>(null);
const configText = ref("");
const configLoading = ref(false);
const configEditor = ref<{ format: () => void; formatting: boolean }>();
const configIssueCount = ref(0);
const configFormat = computed<ConfigFormat | null>(() => {
  const format = configDocument.value?.format.toLowerCase();
  return format === "json" || format === "toml" || format === "cfg"
    ? format
    : null;
});
watch(
  () => configDocument.value?.path,
  () => {
    configIssueCount.value = 0;
  },
);
const modSaving = ref(false);
let configRequest = 0;
const configDirty = computed(
  () => configDocument.value && configText.value !== configDocument.value.text,
);
function rememberDraft() {
  const doc = configDocument.value;
  if (!doc) return;
  const key = configKey(props.instanceId, props.version, doc.path);
  if (configDirty.value)
    configDrafts.set(key, { document: doc, text: configText.value });
  else configDrafts.delete(key);
}
async function changeEnabled() {
  if (!props.modActions || props.locked || modSaving.value || !available.value)
    return;
  const identity = panelId.value;
  const enabled = !modEnabled.value;
  modSaving.value = true;
  try {
    const name = await props.modActions.setEnabled(
      props.instanceId,
      props.version,
      props.file.file_name,
      enabled,
    );
    if (identity === panelId.value) {
      confirmedEnabled.value = enabled;
      emit("changed", name);
    }
  } catch (error) {
    emit("error", error);
  } finally {
    modSaving.value = false;
  }
}
async function chooseConfig(path: string) {
  if (!props.modActions || modSaving.value) return;
  rememberDraft();
  const ticket = ++configRequest;
  const key = configKey(props.instanceId, props.version, path);
  configLoading.value = true;
  try {
    const draft = configDrafts.get(key);
    const doc =
      draft?.document ??
      (await props.modActions.read(
        props.instanceId,
        props.version,
        props.file.file_name,
        path,
      ));
    if (ticket !== configRequest) return;
    configDocument.value = doc;
    configText.value = draft?.text ?? doc.text;
  } catch (error) {
    if (ticket === configRequest) emit("error", error);
  } finally {
    if (ticket === configRequest) configLoading.value = false;
  }
}
async function showConfig() {
  if (!props.modActions) return;
  expanded.value = true;
  configOpen.value = !configOpen.value;
  if (!configOpen.value) {
    expanded.value = canExpand.value;
    void loadDetails();
    return;
  }
  const ticket = ++configRequest;
  configLoading.value = true;
  try {
    const files = await props.modActions.list(
      props.instanceId,
      props.version,
      props.file.file_name,
    );
    if (ticket !== configRequest) return;
    configFiles.value = files;
    if (!files.length) {
      configOpen.value = false;
      expanded.value = false;
      emit("changed", props.file.file_name);
      return;
    }
    if (files.length && !configDocument.value)
      await chooseConfig(files[0]!.path);
  } catch (error) {
    if (ticket === configRequest) emit("error", error);
  } finally {
    if (ticket === configRequest) configLoading.value = false;
  }
}
async function saveConfig() {
  const doc = configDocument.value;
  if (
    !doc ||
    !props.modActions ||
    props.locked ||
    configLoading.value ||
    configIssueCount.value > 0 ||
    modSaving.value ||
    !configDirty.value
  )
    return;
  modSaving.value = true;
  const ticket = configRequest;
  const key = configKey(props.instanceId, props.version, doc.path);
  try {
    const saved = await props.modActions.save(
      props.instanceId,
      props.version,
      props.file.file_name,
      doc.path,
      doc.revision,
      configText.value,
    );
    configDrafts.delete(key);
    if (ticket === configRequest) {
      configDocument.value = saved;
      configText.value = saved.text;
    }
  } catch (error) {
    emit("error", error);
  } finally {
    modSaving.value = false;
  }
}
onBeforeUnmount(() => {
  rememberDraft();
  configRequest++;
});
let request = 0;
const panelId = computed(
  () =>
    `content-${encodeURIComponent(JSON.stringify([props.instanceId, props.version, props.kind, props.file.file_name]))}`,
);
async function toggle() {
  if (!canExpand.value) return;
  if (configOpen.value) {
    configOpen.value = false;
    expanded.value = true;
    await loadDetails();
    return;
  }
  expanded.value = !expanded.value;
  await loadDetails();
}
async function loadDetails() {
  if (
    !canExpand.value ||
    !available.value ||
    !expanded.value ||
    metadata.value ||
    loading.value
  )
    return;
  const ticket = ++request;
  loading.value = true;
  failed.value = false;
  try {
    const result = await props.loadMetadata(
      props.instanceId,
      props.version,
      props.kind,
      props.file.file_name,
    );
    if (ticket === request) metadata.value = result;
  } catch (error) {
    if (ticket === request) {
      failed.value = true;
      emit("error", error);
    }
  } finally {
    if (ticket === request) loading.value = false;
  }
}
async function openFileFolder(source?: string) {
  if (!available.value || openingFolder.value) return;
  openingFolder.value = true;
  try {
    await props.openFolder(
      props.instanceId,
      props.version,
      props.kind,
      props.file.file_name,
      source,
    );
  } catch (error) {
    emit("error", error);
  } finally {
    openingFolder.value = false;
  }
}
function showDependency(id: string) {
  const target = props.dependencyTargets.get(id);
  if (target) emit("show-mod", props.instanceId, props.version, target);
}
async function reveal() {
  const identity = panelId.value;
  expanded.value = canExpand.value;
  void loadDetails();
  highlighted.value = true;
  clearTimeout(highlightTimer);
  highlightTimer = setTimeout(() => {
    highlighted.value = false;
  }, 1800);
  await nextTick();
  if (identity !== panelId.value) return;
  row.value?.focus({ preventScroll: true });
  row.value?.scrollIntoView({ behavior: "smooth", block: "nearest" });
}
defineExpose({ reveal });
async function toggleSource(source: string, event: Event) {
  if (!(event.target as HTMLDetailsElement).open) return;
  const current = rawSources.value[source];
  if (current?.loading || current?.text !== undefined) return;
  rawSources.value[source] = { loading: true, failed: false };
  const ticket = request;
  const identity = panelId.value;
  try {
    const text = await props.loadSource(
      props.instanceId,
      props.version,
      props.kind,
      props.file.file_name,
      source,
    );
    if (ticket === request && identity === panelId.value)
      rawSources.value[source] = { text, loading: false, failed: false };
  } catch (error) {
    if (ticket === request && identity === panelId.value) {
      rawSources.value[source] = { loading: false, failed: true };
      emit("error", error);
    }
  }
}
function displayValue(
  field: ContentMetadata["sources"][number]["fields"][number],
) {
  if (field.format === "translate") return t(field.value);
  if (field.format === "timestamp") {
    const date = new Date(Number(field.value));
    if (Number.isFinite(date.getTime()))
      return date.toLocaleString(locale.value);
  }
  return field.value;
}
watch(
  [
    () => props.instanceId,
    () => props.version,
    () => props.kind,
    () =>
      props.kind === "mods"
        ? modFileName(props.file.file_name)
        : props.file.file_name,
  ],
  () => {
    request++;
    configRequest++;
    confirmedEnabled.value = undefined;
    configOpen.value = false;
    configDocument.value = null;
    configFiles.value = [];
    configLoading.value = false;
    expanded.value = false;
    metadata.value = null;
    loading.value = false;
    failed.value = false;
    rawSources.value = {};
    highlighted.value = false;
    clearTimeout(highlightTimer);
  },
);
watch(
  () => props.file.enabled,
  () => {
    confirmedEnabled.value = undefined;
  },
);
watch(available, () => {
  request++;
  metadata.value = null;
  loading.value = false;
  failed.value = false;
  rawSources.value = {};
  if (available.value) void loadDetails();
});
watch(
  () => props.file.has_metadata,
  () => {
    request++;
    metadata.value = null;
    loading.value = false;
    failed.value = false;
    rawSources.value = {};
    if (expanded.value && !configOpen.value) void loadDetails();
  },
);
watch(canExpand, (value) => {
  if (!value && !configOpen.value) expanded.value = false;
});
onBeforeUnmount(() => {
  request++;
  clearTimeout(highlightTimer);
});
</script>

<template>
  <article
    ref="row"
    tabindex="-1"
    class="content-item"
    :class="{
      'is-expanded': expanded,
      'is-targeted': highlighted,
      'is-pending': !available,
    }"
  >
    <div
      class="content-item-heading"
      :class="{
        disabled: available && (isMod ? !modEnabled : !file.enabled),
        'is-expandable': canExpand,
      }"
      @click="toggle"
    >
      <button
        v-if="isMod && modActions"
        type="button"
        class="content-item-icon content-state-button"
        :data-state="modState"
        :title="modActionLabel"
        :aria-label="`${modActionLabel} · ${file.name}`"
        :aria-busy="modSaving || modDownloading"
        :disabled="
          locked ||
          modSaving ||
          modDownloading ||
          (!available && !modActions.download)
        "
        @click.stop="activateMod"
      >
        <LoaderCircle
          v-if="modDownloading || (modSaving && !available)"
          :size="18"
          class="spin"
          aria-hidden="true"
        />
        <svg
          v-else
          width="18"
          height="18"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.5"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path class="mod-state-shell" d="m12 2 9 5v10l-9 5-9-5V7z" />
          <path
            v-if="modState !== 'missing'"
            d="m3 7 9 5 9-5M12 12v10M7.5 4.5l9 5"
          />
          <path v-else d="M12 7v10m-3-3 3 3 3-3" />
        </svg>
      </button>
      <span
        v-else
        class="content-item-icon"
        :title="statusLabel ? t(statusLabel) : undefined"
        :role="statusLabel ? 'img' : undefined"
        :aria-label="statusLabel ? t(statusLabel) : undefined"
        :aria-hidden="statusLabel ? undefined : true"
      >
        <component :is="icon" :size="18" aria-hidden="true" />
      </span>
      <span
        class="content-item-label"
        :class="{ 'resource-drag-handle': draggableResource }"
        @pointerdown="beginResourceDrag"
        @dragstart.prevent
      >
        <component
          :is="canExpand ? 'button' : 'span'"
          v-if="hasDisplayName"
          :type="canExpand ? 'button' : undefined"
          class="content-item-toggle"
          :aria-expanded="canExpand ? expanded : undefined"
          :aria-controls="canExpand ? panelId : undefined"
          @click.stop="toggle"
        >
          <strong :title="displayName">{{ displayName }}</strong>
          <span
            v-if="file.version"
            class="content-item-version"
            :title="file.version"
            >{{ file.version }}</span
          >
        </component>
        <span
          class="content-item-file"
          :class="{ 'is-primary': !hasDisplayName }"
        >
          <span v-if="available" class="content-file-name" :title="fileLabel">
            {{ displayFileName }}
          </span>
          <span v-else class="content-planned-file">{{
            file.declaration?.path ||
            (file.declaration?.file
              ? "File #" + file.declaration.file
              : file.file_name)
          }}</span>
          <span
            v-if="!hasDisplayName && file.version"
            class="content-item-version"
            :title="file.version"
            >{{ file.version }}</span
          >
          <small v-if="file.size > 0 || (kind === 'mods' && available)">{{
            formatBytes(file.size)
          }}</small>
          <small
            v-if="available && (isMod ? !modEnabled : !file.enabled)"
            class="content-disabled-label"
            >{{ t("已禁用") }}</small
          >
        </span>
      </span>
      <span
        v-if="file.declaration && !available && !isMod"
        class="content-item-status"
        :data-status="file.declaration.status"
      >
        {{ t(statusLabel) }}
        <small v-if="file.declaration.status === 'partial'"
          >{{ formatBytes(file.declaration.downloaded) }} /
          {{ formatBytes(file.declaration.total) }}</small
        >
      </span>
      <template
        v-if="kind === 'mods' && available && modActions && file.has_config"
      >
        <button
          type="button"
          class="content-item-expand"
          :aria-label="t('配置')"
          :title="t('配置')"
          :aria-expanded="configOpen && expanded"
          @click.stop="showConfig"
        >
          <Settings2 :size="14" />
        </button>
      </template>
      <button
        v-if="canExpand"
        type="button"
        class="content-item-expand"
        :aria-label="`${t('元数据')} · ${file.name}`"
        :aria-expanded="expanded"
        :aria-controls="panelId"
        @click.stop="toggle"
      >
        <ChevronRight :size="13" class="content-item-chevron" />
      </button>
    </div>
    <div
      v-if="file.declaration?.status === 'partial' && file.declaration.total"
      class="content-download-progress"
      role="progressbar"
      :aria-label="t('下载进度')"
      :aria-valuenow="Math.round(progress)"
      :aria-valuemin="0"
      :aria-valuemax="100"
    >
      <span :style="{ width: progress + '%' }" />
    </div>
    <Transition name="content-expand">
      <div
        v-if="expanded"
        :id="panelId"
        class="content-item-expanded"
        :aria-busy="loading"
      >
        <div class="content-item-clip">
          <div class="content-item-detail">
            <section
              v-if="configOpen && modActions"
              class="mod-config"
              :aria-label="t('配置')"
            >
              <div class="mod-config-frame">
                <div class="mod-config-toolbar">
                  <span :id="`${panelId}-config-label`" hidden>{{
                    t("配置文件")
                  }}</span>
                  <GlassSelect
                    v-if="configFiles.length"
                    :id="`${panelId}-config`"
                    :model-value="configDocument?.path || ''"
                    :disabled="
                      configLoading || modSaving || !configFiles.length
                    "
                    :options="
                      configFiles.map((entry) => ({
                        value: entry.path,
                        label: entry.path,
                      }))
                    "
                    compact
                    floating
                    literal-labels
                    @update:model-value="chooseConfig"
                  />
                  <button
                    v-if="configFormat"
                    type="button"
                    class="text-button mod-config-format"
                    :title="t('格式化（Shift + Alt + F）')"
                    :aria-label="t('格式化')"
                    :disabled="
                      locked ||
                      modSaving ||
                      configLoading ||
                      !configEditor ||
                      configEditor.formatting
                    "
                    @click="configEditor?.format()"
                  >
                    <AlignLeft :size="14" />
                  </button>
                  <button
                    class="text-button"
                    type="button"
                    :disabled="
                      locked ||
                      modSaving ||
                      configLoading ||
                      !configDirty ||
                      configIssueCount > 0
                    "
                    @click="saveConfig"
                  >
                    <Save :size="14" />{{ t("保存")
                    }}<span v-if="configDirty"> ·</span>
                  </button>
                </div>
                <ConfigEditor
                  v-if="configDocument && configFormat"
                  :key="configDocument.path"
                  ref="configEditor"
                  v-model="configText"
                  :format="configFormat"
                  :label="configDocument.path"
                  :readonly="locked || modSaving || configLoading"
                  @update:model-value="rememberDraft"
                  @issues="configIssueCount = $event"
                  @save="saveConfig"
                />
                <textarea
                  v-else-if="configDocument"
                  v-model="configText"
                  :readonly="locked || modSaving || configLoading"
                  spellcheck="false"
                  class="mod-config-editor"
                  :aria-label="configDocument.path"
                  @input="rememberDraft"
                  @keydown.ctrl.enter.prevent="saveConfig"
                  @keydown.meta.enter.prevent="saveConfig"
                />
                <p v-else-if="!configLoading" class="content-item-note">
                  {{
                    t("未找到匹配的配置文件；部分 Mod 首次运行后才生成配置。")
                  }}
                </p>
              </div>
              <p v-if="configDocument" class="content-item-note">
                {{ t("按 Mod ID 匹配配置；保存保留原文并创建备份。") }}
              </p>
              <button
                v-if="configDirty"
                type="button"
                class="text-button"
                :disabled="modSaving"
                @click="
                  configText = configDocument!.text;
                  rememberDraft();
                "
              >
                {{ t("放弃修改") }}
              </button>
            </section>
            <template v-else-if="!available && file.declaration">
              <p
                v-if="file.declaration.status !== 'unresolved'"
                class="content-item-note"
              >
                {{ t("下载完成后可查看文件内的元数据。") }}
              </p>
              <dl class="content-fields">
                <div>
                  <dt>{{ t("来源") }}</dt>
                  <dd>{{ file.declaration.source }}</dd>
                </div>
                <div v-if="file.declaration.project">
                  <dt>Project ID</dt>
                  <dd>{{ file.declaration.project }}</dd>
                </div>
                <div v-if="file.declaration.file">
                  <dt>File ID</dt>
                  <dd>{{ file.declaration.file }}</dd>
                </div>
                <div v-if="file.declaration.optional">
                  <dt>{{ t("可选内容") }}</dt>
                  <dd>
                    {{
                      t(
                        file.declaration.status === "skipped"
                          ? "未选择"
                          : "已选择",
                      )
                    }}
                  </dd>
                </div>
              </dl>
            </template>
            <p v-if="loading && !configOpen" class="content-item-note">
              <LoaderCircle class="spin" :size="14" />{{ t("读取中") }}
            </p>
            <p v-else-if="failed && !configOpen" class="content-item-note">
              {{ t("无法读取元数据") }}
            </p>
            <template v-if="metadata && !configOpen">
              <section
                v-for="(source, sourceIndex) in metadata.sources"
                :key="sourceIndex"
                class="content-source"
              >
                <header>
                  <strong v-if="source.title && source.title !== file.name">{{
                    source.source === ".nekocrate/resources.json"
                      ? t("安装来源")
                      : source.title
                  }}</strong>
                  <component
                    :is="source.revealable ? 'button' : 'span'"
                    :type="source.revealable ? 'button' : undefined"
                    class="content-source-file"
                    :class="{ 'is-link': source.revealable }"
                    :title="
                      source.revealable ? t('在文件管理器中显示') : undefined
                    "
                    @click="source.revealable && openFileFolder(source.source)"
                    ><Braces :size="14" /><span
                      >{{
                        source.source === ".nekocrate/resources.json"
                          ? ""
                          : archivePrefix
                      }}{{ source.source }}</span
                    ></component
                  >
                </header>
                <p v-if="source.description" class="content-description">
                  {{ source.description }}
                </p>
                <dl v-if="source.fields.length" class="content-fields">
                  <div
                    v-for="(field, fieldIndex) in source.fields"
                    :key="fieldIndex"
                  >
                    <dt>{{ t(field.label) }}</dt>
                    <dd>{{ displayValue(field) }}</dd>
                  </div>
                </dl>
                <details
                  v-if="source.dependencies.length"
                  class="content-dependencies"
                >
                  <summary>
                    {{ t("依赖与兼容") }} ({{ source.dependencies.length }})
                  </summary>
                  <ul>
                    <li
                      v-for="(
                        dependency, dependencyIndex
                      ) in source.dependencies"
                      :key="dependencyIndex"
                    >
                      <span>
                        <button
                          v-if="dependencyTargets.has(dependency.id)"
                          type="button"
                          class="content-dependency-link"
                          @click="showDependency(dependency.id)"
                        >
                          {{ dependency.id }}
                        </button>
                        <template v-else>{{ dependency.id }}</template>
                        <small>{{ t(dependency.relation) }}</small></span
                      ><code>{{ dependency.requirement }}</code>
                    </li>
                  </ul>
                </details>
                <p v-if="source.issue" class="content-item-note">
                  {{ translateMessage(source.issue) }}
                </p>
                <details
                  v-if="source.source !== '.nekocrate/resources.json'"
                  class="content-raw"
                  @toggle="toggleSource(source.source, $event)"
                >
                  <summary>
                    {{ t("原始文件")
                    }}<small v-if="kind === 'saves'">NBT → JSON</small>
                  </summary>
                  <p
                    v-if="rawSources[source.source]?.loading"
                    class="content-item-note"
                  >
                    <LoaderCircle class="spin" :size="14" />{{ t("读取中") }}
                  </p>
                  <p
                    v-else-if="rawSources[source.source]?.failed"
                    class="content-item-note"
                  >
                    {{ t("无法读取元数据") }}
                  </p>
                  <pre
                    v-else-if="rawSources[source.source]?.text !== undefined"
                    >{{ rawSources[source.source].text }}</pre>
                </details>
              </section>
            </template>
          </div>
        </div>
      </div>
    </Transition>
  </article>
</template>

<style scoped>
.content-item.is-pending .content-item-icon:not(.content-state-button) {
  opacity: 0.65;
  stroke-dasharray: 3 2;
}
.content-planned-file {
  color: var(--muted);
  overflow-wrap: anywhere;
}
.content-item-status {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  flex-shrink: 0;
  gap: 3px;
  color: var(--muted);
  font-size: 10px;
}
.content-item-status small {
  font: 9px/1.4 var(--mono);
}
.content-download-progress {
  height: 2px;
  margin: 7px 24px 0 29px;
  background: var(--line);
  overflow: hidden;
}
.content-download-progress > span {
  display: block;
  height: 100%;
  background: var(--tab-color);
  transition: width 300ms ease-out;
}
.content-item:focus {
  outline: none;
}
.content-item.is-targeted .content-item-label strong {
  color: var(--tab-color);
  text-shadow: 0 0 18px color-mix(in srgb, var(--tab-color) 60%, transparent);
}
.content-item-heading {
  display: flex;
  align-items: center;
  gap: 11px;
  width: 100%;
  min-height: 58px;
  padding: 9px 2px;
  border: 0;
  background: transparent;
  color: var(--ink);
  text-align: left;
  cursor: default;
}
.content-item-heading.is-expandable {
  cursor: pointer;
}
span.content-item-toggle {
  cursor: default;
}
.content-item-icon {
  display: grid;
  place-items: center;
  width: 18px;
  height: 18px;
  color: var(--tab-color);
  flex-shrink: 0;
  opacity: 0.75;
}
.content-item-toggle,
.content-item-expand {
  min-width: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}
.content-source-file,
.content-dependency-link {
  padding: 0;
  border: 0;
  background: none;
  text-align: left;
  color: inherit;
  font: inherit;
}
.content-source-file {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  max-width: 100%;
  color: var(--muted);
  font: 11px/17px var(--mono);
}
.content-source-file > span {
  min-width: 0;
  overflow-wrap: anywhere;
}
.content-source-file svg {
  color: var(--tab-color);
}
.content-source-file.is-link,
.content-dependency-link {
  color: var(--tab-color);
  transition: text-shadow 180ms;
}
.content-source-file.is-link:hover,
.content-dependency-link:hover {
  text-shadow: 0 0 10px color-mix(in srgb, var(--tab-color) 40%, transparent);
}
.content-raw {
  margin-top: 16px;
}
.content-raw summary {
  font-size: 11px;
  color: var(--muted);
  cursor: pointer;
}
.content-raw summary small {
  margin-left: 8px;
  font: 9px/16px var(--mono);
}
.content-raw pre {
  max-height: 300px;
  overflow: auto;
  overscroll-behavior: contain;
  margin: 12px 0 0;
  padding: 8px 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  color: var(--muted);
  font: 10px/17px var(--mono);
  user-select: text;
  tab-size: 2;
}
.content-item-toggle:focus-visible,
.content-item-expand:focus-visible {
  outline: 1px solid var(--tab-color);
  outline-offset: -1px;
}
.content-item-heading.disabled {
  opacity: 0.55;
}
.content-item-heading {
  transition: opacity 180ms ease-out;
}
.content-item-label {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
  flex: 1;
}
.content-item-label strong {
  display: block;
  min-width: 0;
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  transition:
    color 180ms,
    text-shadow 180ms;
}
.content-item-heading.is-expandable:hover strong,
.is-expanded .content-item-label strong {
  color: var(--tab-color);
  text-shadow: 0 0 10px color-mix(in srgb, var(--tab-color) 25%, transparent);
}
.content-item-file {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  font: 10px/14px var(--mono);
  color: var(--muted);
}
.content-file-name {
  font: inherit;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition:
    color 180ms,
    text-shadow 180ms;
}
.content-item-file.is-primary {
  font-family: inherit;
}
.content-item-file.is-primary .content-file-name {
  color: var(--ink);
  font-size: 13px;
  font-weight: 500;
  line-height: 18px;
}
.content-item-file small {
  flex-shrink: 0;
  font: 10px/14px var(--mono);
}
.content-item-expand {
  display: grid;
  place-items: center;
  align-self: stretch;
  flex-shrink: 0;
  width: 24px;
  color: var(--tab-color);
}
.content-item-version {
  min-width: 0;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font: 11px/17px var(--mono);
  color: var(--muted);
}
.content-item-toggle {
  display: flex;
  align-items: baseline;
  gap: 8px;
}
.content-item-chevron {
  transition: transform 220ms var(--motion-ease);
}
.is-expanded .content-item-chevron {
  transform: rotate(90deg);
}
.content-item-expanded {
  display: grid;
  grid-template-rows: 1fr;
  opacity: 1;
}
.content-item-clip {
  min-height: 0;
  overflow: hidden;
}
.content-item-detail {
  padding: 2px 4px 18px 31px;
}
.content-source + .content-source {
  margin-top: 18px;
}
.content-source header {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 10px;
}
.content-source header strong {
  font-size: 12px;
  font-weight: 500;
}
.content-source > :not(header) {
  margin-inline-start: 20px;
}
.content-description {
  margin: 0 0 14px;
  font-size: 12px;
  line-height: 1.75;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}
.content-fields {
  display: grid;
  gap: 9px;
  margin: 0;
}
.content-fields > div {
  display: grid;
  grid-template-columns: minmax(80px, 0.3fr) minmax(0, 1fr);
  gap: 16px;
  align-items: baseline;
}
.content-fields dt {
  font-size: 10px;
  line-height: 17px;
  color: var(--muted);
}
.content-fields dd {
  margin: 0;
  font: 11px/17px var(--mono);
  overflow-wrap: anywhere;
  user-select: text;
}
.content-dependencies {
  margin-top: 14px;
}
.content-dependencies summary {
  cursor: pointer;
  font-size: 11px;
  color: var(--tab-color);
}
.content-dependencies ul {
  padding: 0;
  margin: 10px 0 0;
  list-style: none;
}
.content-dependencies li {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 12px;
  margin: 7px 0;
  font: 10px/17px var(--mono);
  overflow-wrap: anywhere;
  user-select: text;
}
.content-dependencies small {
  margin-left: 8px;
  font-size: 9px;
  color: var(--muted);
}
.content-item-note {
  display: flex;
  align-items: center;
  gap: 7px;
  margin: 10px 0;
  color: var(--muted);
  font-size: 10px;
  line-height: 17px;
  overflow-wrap: anywhere;
}
.content-expand-enter-active,
.content-expand-leave-active {
  transition:
    grid-template-rows 240ms var(--motion-ease),
    opacity 180ms ease;
}
.content-expand-enter-from,
.content-expand-leave-to {
  grid-template-rows: 0fr;
  opacity: 0;
}
@media (prefers-reduced-motion: reduce) {
  .content-item-expanded,
  .content-item-chevron,
  .content-item-label strong {
    transition: none;
  }
}
</style>

<style scoped>
.mod-config {
  display: grid;
  gap: 8px;
  min-width: 0;
}
.mod-config-toolbar {
  display: flex;
  gap: 8px;
  align-items: center;
  min-width: 0;
  padding: 6px 8px 2px;
}
.mod-config-frame {
  min-width: 0;
  border: 1px solid color-mix(in srgb, currentColor 16%, transparent);
  transition: border-color 180ms ease;
}
.mod-config-frame:focus-within {
  border-color: color-mix(in srgb, var(--tab-color) 55%, transparent);
}
.mod-config-toolbar .text-button {
  flex-shrink: 0;
  min-height: 28px;
  padding: 0 4px;
  font-size: 11px;
}
.mod-config-toolbar :deep(.source-select) {
  flex: 1;
  min-width: 0;
  background: transparent;
  color: inherit;
  border: 0;
  font: inherit;
  font-size: 11px;
}

.mod-config-editor {
  display: block;
  width: 100%;
  min-height: 220px;
  max-height: 420px;
  resize: vertical;
  box-sizing: border-box;
  margin: 0;
  padding: 8px 12px 12px;
  border: 0;
  outline: none;
  background: transparent;
  color: inherit;
  font:
    12px/1.65 ui-monospace,
    SFMono-Regular,
    Consolas,
    monospace;
  tab-size: 2;
}
.mod-config-editor-loading {
  min-height: 266px;
}
</style>

<style scoped>
.mod-config-toolbar :deep(.source-select-trigger) {
  height: 28px;
  padding: 0 4px;
  border: 0;
  background: transparent;
  box-shadow: none;
  font-size: 11px;
}
.mod-config-toolbar :deep(.selection-label) {
  flex: 1;
}
.mod-config-toolbar :deep(.source-select-trigger:hover:not(:disabled)),
.mod-config-toolbar :deep(.source-select-trigger[aria-expanded="true"]) {
  color: var(--tab-color);
  text-shadow: 0 0 10px var(--glow);
}
</style>

<style scoped>
.content-state-button {
  box-sizing: content-box;
  padding: 8px;
  margin: -8px;
  border: 0;
  background: transparent;
  cursor: pointer;
  opacity: 1;
  transition:
    color 180ms ease-out,
    filter 180ms ease-out;
}
.content-state-button[data-state="enabled"] .mod-state-shell {
  fill: currentColor;
  fill-opacity: 0.3;
}
.content-state-button .mod-state-shell {
  fill: currentColor;
  fill-opacity: 0;
  transition: fill-opacity 180ms ease-out;
}
.content-state-button[data-state="disabled"] {
  color: var(--muted);
}
.content-state-button[data-state="missing"] .mod-state-shell {
  stroke-dasharray: 3 2;
}
.content-state-button:hover:not(:disabled),
.content-state-button:focus-visible {
  color: var(--tab-color);
  filter: drop-shadow(
    0 0 4px color-mix(in srgb, var(--tab-color) 45%, transparent)
  );
}
.content-state-button:focus-visible {
  outline: none;
}
.content-state-button:disabled {
  cursor: default;
}
</style>
