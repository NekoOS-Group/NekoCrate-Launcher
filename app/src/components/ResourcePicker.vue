<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import {
  ArrowLeft,
  Check,
  Image,
  Info,
  LoaderCircle,
  Package,
  Plus,
  Search,
  Sparkles,
  X,
} from "lucide-vue-next";
import GlassSelect from "./GlassSelect.vue";
import ProjectImage from "./ProjectImage.vue";
import ProjectDetails from "./ProjectDetails.vue";
import ContentSwitch from "./ContentSwitch.vue";
import CatalogTail from "./CatalogTail.vue";
import { useCatalogFeed } from "../useCatalogFeed";
import { t } from "../i18n";
import { listRevealStyle } from "../listMotion";
import {
  resourceSourceKey,
  type ResourceCopySource,
} from "../useResourceTransfer";
import type {
  ResourceKind,
  ResourcePlatform,
  ResourceProject,
  ResourceSelection,
  ResourceVersion,
} from "../resources";

const props = defineProps<{
  minecraft: string;
  loader: { kind: string; version: string } | null;
  modelValue: readonly ResourceSelection[];
  localResources?: readonly ResourceCopySource[];
  platform: ResourcePlatform;
  disabled: boolean;
  rpc: <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: ResourceSelection[]];
  removeLocalResource: [value: ResourceCopySource];
  back: [];
  error: [value: unknown];
  link: [url: string];
}>();
const categories = [
  { value: "mods", label: "Mods", icon: Package, color: "#69dcb5" },
  { value: "resourcepacks", label: "资源包", icon: Image, color: "#b49aef" },
  { value: "shaderpacks", label: "光影", icon: Sparkles, color: "#e6c68a" },
] as const;
const kind = ref<ResourceKind>(props.loader ? "mods" : "resourcepacks");
const platform = ref(props.platform);
const query = ref("");
const selectedOnly = ref(false);
const inspected = shallowRef<ResourceProject>();
const pending = ref(new Set<string>());
const scope = computed(() => ({
  minecraft: props.minecraft,
  loader: props.loader,
  kind: kind.value,
  platform: platform.value,
}));
const feed = useCatalogFeed<ResourceProject>((offset) =>
  props.rpc("search_resource_catalog", {
    scope: scope.value,
    query: query.value.trim(),
    offset,
  }),
);
const { hits, loading, failure, more, revision, scrollTop } = feed;
const selectionKey = (platform: string, project: string) =>
  `${platform}/${project}`;
const chosen = (project: string) =>
  props.modelValue.find(
    (p) => p.project === project && p.platform === platform.value,
  );
const icon = computed(
  () => categories.find((c) => c.value === kind.value)!.icon,
);
let disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;
async function search(append = false) {
  clearTimeout(timer);
  await feed.search(append);
}
function remove(selection: ResourceSelection) {
  emit(
    "update:modelValue",
    props.modelValue.filter(
      (p) =>
        p.platform !== selection.platform || p.project !== selection.project,
    ),
  );
}
async function toggle(project: ResourceProject) {
  if (props.disabled) return;
  const existing = chosen(project.id);
  if (existing) {
    remove(existing);
    return;
  }
  const key = selectionKey(platform.value, project.id);
  if (pending.value.has(key)) return;
  if (
    props.modelValue.length +
      (props.localResources?.length || 0) +
      pending.value.size >=
    128
  ) {
    emit("error", t("最多预选 128 项资源"));
    return;
  }
  const selectedScope = { ...scope.value };
  const compatibility = JSON.stringify([props.minecraft, props.loader]);
  pending.value.add(key);
  try {
    const versions = await props.rpc<ResourceVersion[]>(
      "catalog_resource_versions",
      { scope: selectedScope, project: project.id },
    );
    if (
      disposed ||
      compatibility !== JSON.stringify([props.minecraft, props.loader])
    )
      return;
    if (props.modelValue.length + (props.localResources?.length || 0) >= 128) {
      emit("error", t("最多预选 128 项资源"));
      return;
    }
    const version =
      versions.find((v) => v.release_type === "release") || versions[0];
    if (!version) {
      emit("error", t("此资源没有兼容版本"));
      return;
    }
    emit("update:modelValue", [
      ...props.modelValue,
      {
        platform: selectedScope.platform,
        kind: selectedScope.kind,
        project: project.id,
        version: version.id,
        name: project.name,
        number: version.number,
        icon_url: project.icon_url,
      },
    ]);
  } catch (error) {
    if (
      !disposed &&
      compatibility === JSON.stringify([props.minecraft, props.loader])
    )
      emit("error", error);
  } finally {
    pending.value.delete(key);
  }
}
watch(
  [scope, query],
  () => {
    feed.invalidate(true);
    clearTimeout(timer);
    timer = setTimeout(() => void search(), 220);
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  disposed = true;
  clearTimeout(timer);
});
</script>

<template>
  <section
    class="resource-picker"
    data-drop-kind="selection"
    :data-drop-disabled="disabled || !minecraft"
    :data-drop-loader="loader?.kind || 'vanilla'"
  >
    <Transition name="picker-heading" appear>
      <header class="picker-heading">
        <button
          class="icon-button picker-back"
          type="button"
          :aria-label="t('返回')"
          :title="t('返回')"
          @click="inspected ? (inspected = undefined) : $emit('back')"
        >
          <ArrowLeft :size="18" />
        </button>
        <nav class="picker-categories" :aria-label="t('资源分类')">
          <button
            v-for="category in categories"
            :key="category.value"
            :class="{ active: kind === category.value && !selectedOnly }"
            :style="{ '--resource-accent': category.color }"
            :disabled="disabled || (category.value === 'mods' && !loader)"
            :aria-pressed="kind === category.value && !selectedOnly"
            @click="
              kind = category.value;
              selectedOnly = false;
              inspected = undefined;
            "
          >
            <component :is="category.icon" :size="16" />{{ t(category.label) }}
          </button>
        </nav>
        <div v-if="!inspected" class="picker-tools">
          <GlassSelect
            id="resource-picker-platform"
            :model-value="platform"
            :options="[
              { value: 'modrinth', label: 'Modrinth' },
              { value: 'curseforge', label: 'CurseForge' },
            ]"
            :disabled="disabled"
            compact
            literal-labels
            floating
            @update:model-value="
              platform = $event as ResourcePlatform;
              selectedOnly = false;
            "
          />
          <label class="picker-search"
            ><Search :size="14" /><input
              v-model="query"
              :placeholder="t('搜索资源')"
              :aria-label="t('搜索资源')"
              @input="selectedOnly = false"
          /></label>
          <button
            type="button"
            class="text-button picker-count"
            :class="{ active: selectedOnly }"
            :aria-pressed="selectedOnly"
            @click="selectedOnly = !selectedOnly"
          >
            {{
              t("已选 {count} 项", {
                count: modelValue.length + (localResources?.length || 0),
              })
            }}
          </button>
        </div>
      </header>
    </Transition>
    <ContentSwitch
      class="picker-content"
      :content-key="
        inspected
          ? `${platform}/${inspected.id}`
          : selectedOnly
            ? 'selected'
            : `list:${revision}`
      "
    >
      <div class="picker-results" data-catalog-viewport>
        <div v-if="inspected" class="picker-detail">
          <div class="picker-detail-heading">
            <ProjectImage :url="inspected.icon_url" :rpc="rpc" :size="44" />
            <h3>{{ inspected.name }}</h3>
            <button
              class="text-button"
              :disabled="
                disabled || pending.has(selectionKey(platform, inspected.id))
              "
              :aria-pressed="!!chosen(inspected.id)"
              @click="toggle(inspected)"
            >
              <Check v-if="chosen(inspected.id)" :size="16" /><Plus
                v-else
                :size="16"
              />{{ t(chosen(inspected.id) ? "已预选" : "加入预选") }}
            </button>
          </div>
          <ProjectDetails
            :project="inspected"
            :platform="platform"
            :rpc="rpc"
            @link="$emit('link', $event)"
          />
        </div>
        <template v-else-if="selectedOnly">
          <button
            v-for="(item, index) in modelValue"
            :key="selectionKey(item.platform, item.project)"
            class="picker-card selected list-reveal-item"
            :style="listRevealStyle(index)"
            :disabled="disabled"
            :aria-label="`${t('移除')} ${item.name}`"
            @click="remove(item)"
          >
            <ProjectImage :url="item.icon_url" :rpc="rpc" :size="32"
              ><component
                :is="
                  categories.find((c) => c.value === item.kind)?.icon || Package
                "
                :size="20" /></ProjectImage
            ><span
              ><strong>{{ item.name }}</strong
              ><small
                >{{ item.number }} ·
                {{
                  item.platform === "curseforge" ? "CurseForge" : "Modrinth"
                }}</small
              ></span
            ><X :size="16" />
          </button>
          <button
            v-for="(item, index) in localResources"
            :key="resourceSourceKey(item)"
            class="picker-card selected list-reveal-item"
            :style="listRevealStyle(modelValue.length + index)"
            :disabled="disabled"
            :aria-label="`${t('移除')} ${item.name}`"
            @click="$emit('removeLocalResource', item)"
          >
            <component
              :is="
                categories.find((c) => c.value === item.kind)?.icon || Package
              "
              :size="32"
            />
            <span
              ><strong>{{ item.name }}</strong
              ><small>{{ item.file }}</small></span
            ><X :size="16" />
          </button>
          <span
            v-if="!modelValue.length && !localResources?.length"
            class="picker-empty"
            >{{ t("尚未预选资源") }}</span
          >
        </template>
        <template v-else>
          <div
            v-for="(item, index) in hits"
            :key="`${platform}/${kind}/${item.id}`"
            class="picker-entry list-reveal-item"
            :style="listRevealStyle(index % 20)"
          >
            <button
              class="picker-card"
              :class="{ selected: !!chosen(item.id) }"
              :disabled="
                disabled || pending.has(selectionKey(platform, item.id))
              "
              :aria-pressed="!!chosen(item.id)"
              @click="toggle(item)"
            >
              <ProjectImage :url="item.icon_url" :rpc="rpc" :size="36"
                ><component :is="icon" :size="22"
              /></ProjectImage>
              <span
                ><strong>{{ item.name }}</strong
                ><small>{{
                  chosen(item.id)?.number || item.description
                }}</small></span
              >
              <LoaderCircle
                v-if="pending.has(selectionKey(platform, item.id))"
                class="spin"
                :size="16"
              /><Check v-else-if="chosen(item.id)" :size="16" /><Plus
                v-else
                :size="16"
              />
            </button>
            <button
              class="icon-button picker-info"
              :aria-label="`${t('查看介绍')} ${item.name}`"
              :title="t('查看介绍')"
              @click="inspected = item"
            >
              <Info :size="16" />
            </button>
          </div>
          <span
            v-if="!failure && !loading && !hits.length"
            class="picker-empty"
            >{{ t("没有找到兼容资源") }}</span
          >
          <CatalogTail
            v-model:position="scrollTop"
            :loading="loading"
            :more="more"
            :failure="failure"
            :disabled="disabled"
            @load="search(true)"
            @retry="feed.retry()"
          />
        </template>
      </div>
    </ContentSwitch>
  </section>
</template>

<style scoped>
.resource-picker {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.picker-tools {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 12px;
  margin-left: auto;
  min-width: 0;
  flex: 1;
}
.picker-count {
  flex-shrink: 0;
  white-space: nowrap;
}
.picker-heading {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 24px;
  flex-shrink: 0;
  min-width: 0;
}
.picker-back {
  flex-shrink: 0;
}
.picker-categories {
  display: flex;
  gap: 14px;
  flex-shrink: 0;
}
.picker-categories button {
  display: flex;
  align-items: center;
  gap: 6px;
  border: 0;
  background: transparent;
  color: var(--muted);
  padding: 8px 0;
  font-size: 14px;
  white-space: nowrap;
}
.picker-categories button.active,
.picker-categories button:hover:not(:disabled) {
  color: var(--resource-accent);
  text-shadow: 0 0 12px var(--resource-accent);
}
.picker-tools :deep(.source-select) {
  width: 110px;
  flex-shrink: 0;
}
.picker-search {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 1 160px;
  min-width: 80px;
  color: var(--muted);
}
.picker-search svg {
  flex-shrink: 0;
}
.picker-search input {
  border: 0;
  box-shadow: none;
  background: transparent;
  width: 100%;
  min-width: 0;
  padding: 6px 0;
  font-size: 12px;
  outline: none;
}
.picker-content {
  flex: 1;
  min-height: 0;
}
.picker-entry {
  display: flex;
  align-items: center;
  border-bottom: 1px solid var(--line);
}
.picker-entry > .picker-card {
  border-bottom: 0;
  min-width: 0;
  flex: 1;
}
.picker-info {
  flex-shrink: 0;
}
.picker-detail-heading {
  display: flex;
  align-items: center;
  gap: 14px;
  margin: 12px 0 18px;
}
.picker-detail-heading h3 {
  font-size: 18px;
  font-weight: 500;
  min-width: 0;
  overflow-wrap: anywhere;
  flex: 1;
  margin: 0;
}
.picker-detail-heading .text-button {
  flex-shrink: 0;
  font-size: 12px;
}
.picker-results {
  flex: 1;
  min-height: 0;
  overflow: auto;
  overflow-x: hidden;
  padding: 4px 24px 12px;
}
.picker-card {
  display: flex;
  align-items: center;
  gap: 14px;
  width: 100%;
  border: 0;
  border-bottom: 1px solid var(--line);
  background: transparent;
  padding: 13px 8px;
  color: var(--muted);
  text-align: left;
  transition:
    color 180ms var(--motion-ease),
    text-shadow 180ms var(--motion-ease);
}
.picker-card > svg {
  flex-shrink: 0;
}
.picker-card > span:not(.project-image) {
  display: flex;
  flex-direction: column;
  gap: 5px;
  flex: 1;
  min-width: 0;
}
.picker-card strong {
  font-size: 13px;
  font-weight: 500;
  color: var(--ink);
}
.picker-card small {
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.picker-card:hover,
.picker-card.selected,
.picker-count.active {
  color: var(--blue);
}
.picker-card.selected strong {
  color: var(--blue);
  text-shadow: 0 0 12px var(--glow);
}
.picker-heading-enter-active {
  transition:
    opacity var(--motion-fade) var(--motion-ease),
    transform var(--motion-row) var(--motion-ease);
}
.picker-heading-enter-from {
  opacity: 0;
  transform: translateX(-8px);
}
@media (prefers-reduced-motion: reduce) {
  .picker-heading-enter-active {
    transition: none;
  }
}
.picker-empty {
  display: flex;
  gap: 16px;
  padding: 24px 8px;
  color: var(--muted);
  font-size: 12px;
}
</style>
