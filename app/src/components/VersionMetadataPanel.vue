<script setup lang="ts">
import { listRevealStyle } from "../listMotion";
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { Braces, ChevronRight } from "lucide-vue-next";
import { t, translateMessage } from "../i18n";
import type {
  VersionContentTab,
  MetadataDocument,
  VersionMetadata,
} from "../types";

const props = defineProps<{
  version: string;
  refresh: number;
  loadMetadata: (version: string) => Promise<VersionMetadata>;
}>();
const emit = defineEmits<{
  error: [error: unknown];
  "show-content": [kind: VersionContentTab];
}>();
const metadata = ref<VersionMetadata | null>(null);
const loading = ref(false);
const failed = ref(false);
const opened = ref<string | null>(null);
const raw = ref(false);
let request = 0;
const sourceText = computed(() => {
  if (!raw.value) return "";
  const text = metadata.value?.documents.find(
    (d) => d.path === opened.value,
  )?.json;
  if (text == null) return "";
  try {
    return JSON.stringify(JSON.parse(text), null, 2);
  } catch {
    return text;
  }
});
function toggle(document: MetadataDocument) {
  opened.value = opened.value === document.path ? null : document.path;
  raw.value = false;
}
function location(document: MetadataDocument) {
  const root = metadata.value?.game_directory;
  const path = document.path.replaceAll("\\", "/");
  const prefix = root?.replaceAll("\\", "/").replace(/\/$/, "") + "/";
  return path.startsWith(prefix) ? path.slice(prefix.length) : path;
}
async function reload(reset: boolean) {
  const ticket = ++request;
  loading.value = true;
  failed.value = false;
  if (reset) {
    metadata.value = null;
    opened.value = null;
    raw.value = false;
  }
  try {
    const result = await props.loadMetadata(props.version);
    if (ticket !== request) return;
    metadata.value = result;
    if (reset) {
      opened.value =
        (
          result.documents.find((d) => d.id === props.version) ||
          result.documents.find((d) =>
            d.fields.some((f) =>
              ["游戏版本", "版本 ID", "Java 版本"].includes(f.label),
            ),
          ) ||
          result.documents[0]
        )?.path || null;
    } else if (!result.documents.some((d) => d.path === opened.value)) {
      opened.value = null;
      raw.value = false;
    }
  } catch (error) {
    if (ticket === request) {
      failed.value = true;
      emit("error", error);
    }
  } finally {
    if (ticket === request) loading.value = false;
  }
}
watch(
  () => props.version,
  () => void reload(true),
  { immediate: true },
);
watch(
  () => props.refresh,
  () => void reload(false),
);
onBeforeUnmount(() => {
  request++;
});
</script>

<template>
  <div class="version-metadata" :aria-busy="loading">
    <p v-if="failed" class="metadata-empty">{{ t("无法读取元数据") }}</p>
    <template v-if="metadata">
      <p v-if="metadata.issue" class="metadata-note">
        {{ translateMessage(metadata.issue) }}
      </p>
      <p v-if="!metadata.documents.length" class="metadata-empty">
        {{ t("未找到元数据文件") }}
      </p>
      <article
        v-for="(document, index) in metadata.documents"
        :key="document.path"
        class="metadata-document list-reveal-item"
        :style="listRevealStyle(index, 0)"
        :class="{ 'is-open': opened === document.path }"
      >
        <button
          class="metadata-document-toggle"
          :aria-expanded="opened === document.path"
          :aria-controls="`metadata-file-${index}`"
          @click="toggle(document)"
        >
          <Braces :size="18" :stroke-width="1.5" />
          <span class="metadata-document-label">
            <strong>{{ document.name }}</strong>
            <small :title="document.path"
              >{{ t(document.kind) }} · {{ location(document) }}</small
            >
          </span>
          <ChevronRight :size="15" class="metadata-chevron" />
        </button>
        <Transition name="metadata-expand">
          <div
            v-if="opened === document.path"
            :id="`metadata-file-${index}`"
            class="metadata-expanded"
          >
            <div class="metadata-expanded-clip">
              <div class="metadata-details">
                <p v-if="document.issue" class="metadata-note">
                  {{ translateMessage(document.issue) }}
                </p>
                <dl v-if="document.fields.length" class="metadata-fields">
                  <div
                    v-for="(field, fieldIndex) in document.fields"
                    :key="fieldIndex"
                  >
                    <dt>{{ t(field.label) }}</dt>
                    <dd>{{ field.value }}</dd>
                  </div>
                </dl>
                <p
                  v-if="
                    !document.issue &&
                    !document.fields.length &&
                    !document.sections.length
                  "
                  class="metadata-note"
                >
                  {{ t("此文件暂无可解析的信息") }}
                </p>
                <template
                  v-for="(section, sectionIndex) in document.sections"
                  :key="sectionIndex"
                >
                  <button
                    v-if="section.target"
                    type="button"
                    class="metadata-section-link"
                    @click="emit('show-content', section.target)"
                  >
                    {{ t(section.label)
                    }}<span>({{ section.entries.length }})</span>
                    <ChevronRight :size="13" />
                  </button>
                  <details v-else open class="metadata-section">
                    <summary>
                      <ChevronRight :size="12" />
                      {{ t(section.label)
                      }}<span>({{ section.entries.length }})</span>
                    </summary>
                    <p v-if="section.note" class="metadata-note">
                      {{ t(section.note) }}
                    </p>
                    <ul v-if="section.entries.length" class="metadata-entries">
                      <li
                        v-for="(entry, entryIndex) in section.entries"
                        :key="entryIndex"
                      >
                        <span>{{ entry.name }}</span>
                        <small v-if="entry.detail">{{ entry.detail }}</small>
                      </li>
                    </ul>
                  </details>
                </template>
                <button
                  v-if="document.json !== null"
                  class="metadata-source-toggle"
                  :aria-expanded="raw"
                  :aria-controls="`metadata-source-${index}`"
                  @click="raw = !raw"
                >
                  <ChevronRight :size="12" :class="{ expanded: raw }" />{{
                    t("原始文件")
                  }}
                </button>
                <pre
                  v-if="raw && document.json !== null"
                  :id="`metadata-source-${index}`"
                  class="metadata-json"
                  tabindex="0"
                  :aria-label="t('原始文件')"
                ><code>{{ sourceText }}</code></pre>
              </div>
            </div>
          </div>
        </Transition>
      </article>
    </template>
  </div>
</template>

<style scoped>
.version-metadata {
  flex: 1;
  min-height: 0;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  padding: 0 5px 12px 0;
}
.metadata-document + .metadata-document {
  border-top: 1px solid color-mix(in srgb, var(--line) 45%, transparent);
}
.metadata-document-toggle {
  display: flex;
  width: 100%;
  align-items: center;
  gap: 12px;
  border: 0;
  background: transparent;
  padding: 14px 2px;
  text-align: left;
  color: var(--muted);
  transition: color 180ms var(--motion-ease);
}
.metadata-document-toggle:hover,
.is-open > .metadata-document-toggle {
  color: var(--blue);
}
.metadata-document-toggle:focus-visible,
.metadata-section-link:focus-visible,
.metadata-source-toggle:focus-visible {
  outline: 1px solid var(--blue);
  outline-offset: -1px;
}
.metadata-document-toggle > svg {
  flex-shrink: 0;
}
.metadata-document-label {
  display: flex;
  flex: 1;
  min-width: 0;
  flex-direction: column;
  gap: 4px;
}
.metadata-document-label strong {
  font: 12px/18px var(--mono);
  color: var(--ink);
  overflow-wrap: anywhere;
}
.metadata-document-label small {
  font: 9px/15px var(--mono);
  color: var(--muted);
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.metadata-chevron,
.metadata-source-toggle svg {
  transition: transform 220ms var(--motion-ease);
}
.is-open .metadata-chevron,
.metadata-source-toggle svg.expanded {
  transform: rotate(90deg);
}
.metadata-expanded {
  display: grid;
  grid-template-rows: 1fr;
  opacity: 1;
}
.metadata-expanded-clip {
  min-height: 0;
  overflow: hidden;
}
.metadata-details {
  padding: 2px 4px 18px 32px;
}
.metadata-fields {
  margin: 0;
  display: grid;
  gap: 10px;
}
.metadata-fields > div {
  display: grid;
  grid-template-columns: minmax(80px, 0.3fr) minmax(0, 1fr);
  gap: 16px;
  align-items: baseline;
}
.metadata-fields dt {
  font-size: 11px;
  line-height: 17px;
  color: var(--muted);
}
.metadata-fields dd {
  margin: 0;
  font: 11px/17px var(--mono);
  overflow-wrap: anywhere;
  user-select: text;
}
.metadata-section {
  margin-top: 20px;
}
.metadata-section summary,
.metadata-section-link {
  display: flex;
  align-items: center;
  gap: 4px;
  cursor: pointer;
  list-style: none;
  margin: 0 0 9px;
  font-size: 11px;
  line-height: 18px;
  font-weight: 500;
  color: var(--blue);
}
.metadata-section summary span,
.metadata-section-link span {
  margin-left: 4px;
  font: 10px var(--mono);
  color: var(--muted);
}
.metadata-section-link {
  margin: 20px 0 0;
  padding: 4px 0;
  border: 0;
  background: transparent;
  transition: text-shadow 180ms var(--motion-ease);
}
.metadata-section-link:hover {
  text-shadow: 0 0 10px var(--glow);
}
.metadata-section-link > svg {
  margin-left: 4px;
}
.metadata-section summary::-webkit-details-marker {
  display: none;
}
.metadata-section summary svg {
  transition: transform 180ms var(--motion-ease);
}
.metadata-section[open] summary svg {
  transform: rotate(90deg);
}
.metadata-note {
  margin: 0 0 10px;
  font-size: 10px;
  line-height: 17px;
  color: var(--muted);
  overflow-wrap: anywhere;
}
.metadata-entries {
  padding: 0;
  margin: 0;
  list-style: none;
}
.metadata-entries li {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 6px 0;
  user-select: text;
}
.metadata-entries span {
  font: 11px/17px var(--mono);
  overflow-wrap: anywhere;
}
.metadata-entries small {
  font: 9px/15px var(--mono);
  color: var(--muted);
  overflow-wrap: anywhere;
}
.metadata-source-toggle {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 4px 0;
  margin-top: 16px;
  border: 0;
  background: transparent;
  color: var(--muted);
  font-size: 10px;
}
.metadata-source-toggle:hover {
  color: var(--blue);
}
.metadata-json {
  margin: 8px 0 0;
  padding: 10px;
  max-height: 280px;
  overflow: auto;
  background: color-mix(in srgb, var(--night) 25%, transparent);
  font: 10px/17px var(--mono);
  user-select: text;
  tab-size: 2;
}
.metadata-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 20px 0;
  font-size: 12px;
  color: var(--muted);
}
.metadata-expand-enter-active,
.metadata-expand-leave-active {
  transition:
    grid-template-rows 240ms var(--motion-ease),
    opacity 180ms ease;
}
.metadata-expand-enter-from,
.metadata-expand-leave-to {
  grid-template-rows: 0fr;
  opacity: 0;
}
@media (prefers-reduced-motion: reduce) {
  .metadata-expanded,
  .metadata-chevron,
  .metadata-source-toggle svg,
  .metadata-document-toggle {
    transition: none;
  }
}
</style>
