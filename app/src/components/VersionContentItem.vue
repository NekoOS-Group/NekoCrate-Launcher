<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  ref,
  watch,
  type Component,
} from "vue";
import { Braces, ChevronRight, LoaderCircle } from "lucide-vue-next";
import { locale, t, translateMessage } from "../i18n";
import { formatBytes } from "../downloadWorkflow";
import type { ContentKind, ContentMetadata, LocalContentFile } from "../types";

const props = defineProps<{
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
  "show-mod": [id: string, version: string, file: string];
}>();
const row = ref<HTMLElement | null>(null);
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
let request = 0;
const panelId = computed(
  () =>
    `content-${encodeURIComponent(JSON.stringify([props.instanceId, props.version, props.kind, props.file.file_name]))}`,
);
async function toggle() {
  expanded.value = !expanded.value;
  await loadDetails();
}
async function loadDetails() {
  if (!available.value || !expanded.value || metadata.value || loading.value)
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
  expanded.value = true;
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
    () => props.file.file_name,
  ],
  () => {
    request++;
    expanded.value = false;
    metadata.value = null;
    loading.value = false;
    failed.value = false;
    rawSources.value = {};
    highlighted.value = false;
    clearTimeout(highlightTimer);
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
      :class="{ disabled: !file.enabled }"
      @click="toggle"
    >
      <component :is="icon" :size="18" />
      <span class="content-item-label">
        <button
          v-if="hasDisplayName"
          type="button"
          class="content-item-toggle"
          :aria-expanded="expanded"
          :aria-controls="panelId"
          @click.stop="toggle"
        >
          <strong :title="displayName">{{ displayName }}</strong>
        </button>
        <span
          class="content-item-file"
          :class="{ 'is-primary': !hasDisplayName }"
        >
          <button
            v-if="available"
            type="button"
            class="content-file-link"
            :title="t('在文件管理器中显示')"
            :aria-label="`${t('在文件管理器中显示')} · ${file.file_name}`"
            :aria-busy="openingFolder"
            @click.stop="openFileFolder()"
          >
            {{ file.file_name }}
          </button>
          <span v-else class="content-planned-file">{{
            file.declaration?.path ||
            (file.declaration?.file
              ? "File #" + file.declaration.file
              : file.file_name)
          }}</span>
          <small v-if="file.size > 0 || (kind === 'mods' && available)">{{
            formatBytes(file.size)
          }}</small>
        </span>
      </span>
      <span
        v-if="file.declaration"
        class="content-item-status"
        :data-status="file.declaration.status"
      >
        {{ t(statusLabel) }}
        <small v-if="file.declaration.status === 'partial'"
          >{{ formatBytes(file.declaration.downloaded) }} /
          {{ formatBytes(file.declaration.total) }}</small
        >
      </span>
      <span v-if="file.version || !file.enabled" class="content-item-version"
        >{{ file.version
        }}<small v-if="!file.enabled">{{ t("已禁用") }}</small></span
      >
      <button
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
            <template v-if="!available && file.declaration">
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
            <p v-if="loading" class="content-item-note">
              <LoaderCircle class="spin" :size="14" />{{ t("读取中") }}
            </p>
            <p v-else-if="failed" class="content-item-note">
              {{ t("无法读取元数据") }}
            </p>
            <template v-if="metadata">
              <p v-if="!metadata.sources.length" class="content-item-note">
                {{ t("此文件未提供可识别的元数据") }}
              </p>
              <section
                v-for="(source, sourceIndex) in metadata.sources"
                :key="sourceIndex"
                class="content-source"
              >
                <header>
                  <strong v-if="source.title && source.title !== file.name">{{
                    source.title
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
                      >{{ archivePrefix }}{{ source.source }}</span
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
.content-item.is-pending .content-item-heading > svg {
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
.content-item-status[data-status="local"] {
  color: var(--tab-color);
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
  cursor: pointer;
}
.content-item-heading > svg {
  color: var(--tab-color);
  flex-shrink: 0;
  opacity: 0.75;
}
.content-item-toggle,
.content-file-link,
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
.content-file-link:focus-visible,
.content-item-expand:focus-visible {
  outline: 1px solid var(--tab-color);
  outline-offset: -1px;
}
.content-item-heading.disabled {
  opacity: 0.55;
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
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  transition:
    color 180ms,
    text-shadow 180ms;
}
.content-item-heading:hover strong,
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
.content-file-link {
  font: inherit;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition:
    color 180ms,
    text-shadow 180ms;
}
.content-file-link:hover,
.content-file-link:focus-visible {
  color: var(--tab-color);
  text-shadow: 0 0 10px color-mix(in srgb, var(--tab-color) 25%, transparent);
}
.content-item-file.is-primary {
  font-family: inherit;
}
.content-item-file.is-primary .content-file-link {
  color: var(--ink);
  font-size: 13px;
  font-weight: 500;
  line-height: 18px;
}
.content-item-file.is-primary .content-file-link:hover,
.content-item-file.is-primary .content-file-link:focus-visible {
  color: var(--tab-color);
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
  max-width: 100px;
  overflow-wrap: anywhere;
  font: 11px/17px var(--mono);
  color: var(--muted);
  text-align: right;
}
.content-item-version small {
  display: block;
  font-size: 10px;
  line-height: 16px;
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
  .content-file-link,
  .content-item-label strong {
    transition: none;
  }
}
</style>
