<script setup lang="ts">
import { computed, onBeforeUnmount, ref, shallowRef, watch } from "vue";
import {
  ArrowLeft,
  Check,
  Image,
  LoaderCircle,
  Package,
  Plus,
  Search,
  Sparkles,
  X,
} from "lucide-vue-next";
import GlassSelect from "./GlassSelect.vue";
import { t, translateMessage } from "../i18n";
import { listRevealStyle } from "../listMotion";
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
  platform: ResourcePlatform;
  disabled: boolean;
  rpc: <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: ResourceSelection[]];
  back: [];
  error: [value: unknown];
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
const hits = shallowRef<ResourceProject[]>([]);
const total = ref(0);
const offset = ref(0);
const loading = ref(false);
const failure = ref("");
const pending = ref(new Set<string>());
const scope = computed(() => ({
  minecraft: props.minecraft,
  loader: props.loader,
  kind: kind.value,
  platform: platform.value,
}));
const selectionKey = (platform: string, project: string) =>
  `${platform}/${project}`;
const chosen = (project: string) =>
  props.modelValue.find(
    (p) => p.project === project && p.platform === platform.value,
  );
const icon = computed(
  () => categories.find((c) => c.value === kind.value)!.icon,
);
const context = computed(
  () =>
    `${props.minecraft} · ${props.loader ? `${props.loader.kind} ${props.loader.version}` : t("原版")}`,
);
let generation = 0;
let disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;
async function search(append = false) {
  const request = ++generation;
  const next = append ? offset.value + 20 : 0;
  if (!append) hits.value = [];
  loading.value = true;
  failure.value = "";
  try {
    const result = await props.rpc<{ hits: ResourceProject[]; total: number }>(
      "search_resource_catalog",
      { scope: scope.value, query: query.value.trim(), offset: next },
    );
    if (disposed || request !== generation) return;
    hits.value = append ? [...hits.value, ...result.hits] : result.hits;
    offset.value = next;
    total.value = result.total;
  } catch (error) {
    if (!disposed && request === generation)
      failure.value = translateMessage(String(error));
  } finally {
    if (!disposed && request === generation) loading.value = false;
  }
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
  if (props.modelValue.length + pending.value.size >= 128) {
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
    generation++;
    hits.value = [];
    total.value = 0;
    loading.value = true;
    clearTimeout(timer);
    timer = setTimeout(() => void search(), 220);
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  clearTimeout(timer);
});
</script>

<template>
  <section class="resource-picker">
    <header class="picker-context">
      <button class="text-button" type="button" @click="$emit('back')">
        <ArrowLeft :size="16" />{{ t("资源预选") }}
      </button>
      <span>{{ context }}</span>
      <button
        class="text-button picker-count"
        :class="{ active: selectedOnly }"
        :aria-pressed="selectedOnly"
        @click="selectedOnly = !selectedOnly"
      >
        {{ t("已选 {count} 项", { count: modelValue.length }) }}
      </button>
    </header>
    <div class="picker-tools">
      <nav :aria-label="t('资源分类')">
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
          "
        >
          <component :is="category.icon" :size="16" />{{ t(category.label) }}
        </button>
      </nav>
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
    </div>
    <div class="picker-results">
      <template v-if="selectedOnly">
        <button
          v-for="(item, index) in modelValue"
          :key="selectionKey(item.platform, item.project)"
          class="picker-card selected list-reveal-item"
          :style="listRevealStyle(index)"
          :disabled="disabled"
          :aria-label="`${t('移除')} ${item.name}`"
          @click="remove(item)"
        >
          <component
            :is="categories.find((c) => c.value === item.kind)?.icon || Package"
            :size="20"
          /><span
            ><strong>{{ item.name }}</strong
            ><small
              >{{ item.number }} ·
              {{
                item.platform === "curseforge" ? "CurseForge" : "Modrinth"
              }}</small
            ></span
          ><X :size="16" />
        </button>
        <span v-if="!modelValue.length" class="picker-empty">{{
          t("尚未预选资源")
        }}</span>
      </template>
      <template v-else>
        <button
          v-for="(item, index) in hits"
          :key="`${platform}/${kind}/${item.id}`"
          class="picker-card list-reveal-item"
          :class="{ selected: !!chosen(item.id) }"
          :style="listRevealStyle(index)"
          :disabled="disabled || pending.has(selectionKey(platform, item.id))"
          :aria-pressed="!!chosen(item.id)"
          @click="toggle(item)"
        >
          <component :is="icon" :size="20" /><span
            ><strong>{{ item.name }}</strong
            ><small>{{
              chosen(item.id)?.number || item.description
            }}</small></span
          ><LoaderCircle
            v-if="pending.has(selectionKey(platform, item.id))"
            class="spin"
            :size="16"
          /><Check v-else-if="chosen(item.id)" :size="16" /><Plus
            v-else
            :size="16"
          />
        </button>
        <div v-if="failure" class="picker-empty">
          <span>{{ failure }}</span
          ><button class="text-button" @click="search()">
            {{ t("重试") }}
          </button>
        </div>
        <span v-else-if="!loading && !hits.length" class="picker-empty">{{
          t("没有找到兼容资源")
        }}</span>
        <button
          v-if="!failure && (loading || hits.length < total)"
          class="text-button picker-more"
          :disabled="loading"
          @click="search(true)"
        >
          <LoaderCircle v-if="loading" class="spin" :size="16" /><span v-else>{{
            t("加载更多")
          }}</span>
        </button>
      </template>
    </div>
    <footer class="picker-footer">
      <small>{{ t("预选资源将随此版本一起安装") }}</small
      ><button type="button" class="button primary" @click="$emit('back')">
        {{ t("完成选择") }}
      </button>
    </footer>
  </section>
</template>

<style scoped>
.resource-picker {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.picker-context,
.picker-tools,
.picker-footer {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 12px 24px;
  flex-shrink: 0;
}
.picker-context > span {
  font: 11px var(--mono);
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.picker-count {
  margin-left: auto;
  white-space: nowrap;
}
.picker-tools {
  gap: 12px;
  padding-top: 2px;
}
.picker-tools nav {
  display: flex;
  gap: 14px;
  margin-right: auto;
}
.picker-tools nav button {
  display: flex;
  align-items: center;
  gap: 6px;
  border: 0;
  background: transparent;
  color: var(--muted);
  padding: 8px 0;
  font-size: 12px;
  white-space: nowrap;
}
.picker-tools nav button.active,
.picker-tools nav button:hover:not(:disabled) {
  color: var(--resource-accent);
  text-shadow: 0 0 12px var(--resource-accent);
}
.picker-tools :deep(.glass-select) {
  width: 110px;
  flex-shrink: 0;
}
.picker-search {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 160px;
  color: var(--muted);
}
.picker-search input {
  border: 0;
  background: transparent;
  width: 100%;
  min-width: 0;
  padding: 6px 0;
  font-size: 12px;
  outline: none;
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
.picker-card > span {
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
.picker-footer {
  justify-content: space-between;
  border-top: 1px solid var(--line);
}
.picker-footer small {
  font-size: 11px;
  color: var(--muted);
}
.picker-empty {
  display: flex;
  gap: 16px;
  padding: 24px 8px;
  color: var(--muted);
  font-size: 12px;
}
.picker-more {
  margin: 14px auto 0;
}
</style>
