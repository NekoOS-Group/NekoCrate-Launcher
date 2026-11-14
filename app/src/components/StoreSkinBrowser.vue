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
  Check,
  Download,
  ExternalLink,
  LoaderCircle,
  RefreshCw,
  Search,
  Upload,
  X,
} from "lucide-vue-next";
import { open } from "@tauri-apps/plugin-dialog";
import { t, translateMessage } from "../i18n";
import type { ResourceRpc } from "../projectPresentation";
import { createSkinThumbnails } from "../skinThumbnails";
import { loadSkinImage } from "../previewSkin";
import { listRevealStyle } from "../listMotion";
import CatalogTail from "./CatalogTail.vue";
import GlassSelect from "./GlassSelect.vue";
import SkinLibraryItem from "./SkinLibraryItem.vue";
const props = defineProps<{ rpc: ResourceRpc; busy: boolean }>();
const emit = defineEmits<{
  error: [error: unknown];
  link: [url: string];
  busy: [value: boolean];
  downloaded: [];
}>();
const sources = [
  {
    value: "littleskin",
    label: "LittleSkin",
    url: "https://littleskin.cn/skinlib",
  },
  { value: "elyby", label: "Ely.by", url: "https://ely.by/skins" },
  {
    value: "minecraftskins",
    label: "MinecraftSkins.net",
    url: "https://www.minecraftskins.net/",
  },
  { value: "mineskin", label: "MineSkin", url: "https://mineskin.org/skins" },
];
type Skin = {
  source: string;
  id: string;
  name: string;
  author: string;
  slim: boolean | null;
  texture: string | null;
};
const source = ref("littleskin"),
  query = ref("");
const catalog = computed(
  () => sources.find((s) => s.value === source.value) || sources[0]!,
);
const skins = shallowRef<Skin[]>([]),
  loading = ref(false),
  failure = ref(""),
  more = ref(false),
  scrollTop = ref(0);
const viewport = ref<HTMLElement>();
const saving = ref(""),
  saved = ref(new Set<string>()),
  slim = ref<Record<string, boolean>>({});
const thumbnails = createSkinThumbnails();
const textures = new Map<string, Promise<string>>();
const waiters: (() => void)[] = [];
const key = (skin: Skin) => `${skin.source}:${skin.id}:${skin.texture || ""}`;
let activeLoads = 0,
  cursor = "",
  generation = 0,
  disposed = false;
let timer: ReturnType<typeof setTimeout> | undefined;
const action = <T,>(kind: string, data: Record<string, unknown> = {}) =>
  props.rpc<T>("resource_manager", { action: { kind, ...data } });
function invalidate(waiting = false) {
  generation++;
  // Pending previews belong to the old query. A quick switch back must not reuse
  // a promise whose queued request is about to be cancelled.
  textures.clear();
  cursor = "";
  skins.value = [];
  more.value = false;
  loading.value = waiting;
  failure.value = "";
  scrollTop.value = 0;
  if (viewport.value) viewport.value.scrollTop = 0;
}
async function load(append = false) {
  if (disposed || (append && (loading.value || !more.value))) return;
  if (!append) invalidate();
  const request = generation,
    requestedCursor = cursor;
  loading.value = true;
  failure.value = "";
  try {
    const result = await action<{ skins: Skin[]; next: string | null }>(
      "store_skins",
      {
        source: source.value,
        query: query.value.trim(),
        cursor: requestedCursor,
      },
    );
    if (disposed || request !== generation) return;
    const seen = new Set(skins.value.map(key));
    const added = result.skins.filter((s) => {
      const id = key(s);
      if (seen.has(id)) return false;
      seen.add(id);
      return true;
    });
    skins.value = [...skins.value, ...added];
    // A page can contain only unsupported capes/HD skins; its next page still matters.
    more.value = !!result.next && result.next !== requestedCursor;
    cursor = result.next || "";
  } catch (error) {
    if (!disposed && request === generation)
      failure.value = error instanceof Error ? error.message : String(error);
  } finally {
    if (!disposed && request === generation) loading.value = false;
  }
}
function texture(skin: Skin): Promise<string> {
  const id = key(skin),
    request = generation;
  const existing = textures.get(id);
  if (existing) return existing;
  const result = (async () => {
    if (activeLoads >= 4)
      await new Promise<void>((resolve) => waiters.push(resolve));
    else activeLoads++;
    try {
      if (disposed || request !== generation)
        throw new Error("Skin preview cancelled");
      const data = await action<string>("store_skin_bytes", { input: skin });
      if (skin.slim == null && !disposed) {
        const image = await loadSkinImage(data);
        const canvas = document.createElement("canvas");
        canvas.width = canvas.height = 64;
        const ctx = canvas.getContext("2d")!;
        ctx.imageSmoothingEnabled = false;
        ctx.drawImage(image, 0, 0, 64, (image.height / image.width) * 64);
        const arm = ctx.getImageData(54, 20, 2, 12).data;
        slim.value[id] =
          image.width === image.height &&
          arm.every((v, i) => i % 4 !== 3 || v === 0);
      }
      return data;
    } finally {
      const next = waiters.shift();
      if (next) next();
      else activeLoads--;
    }
  })();
  textures.set(id, result);
  void result.catch(() => {
    if (textures.get(id) === result) textures.delete(id);
  });
  return result;
}
async function collect(skin: Skin) {
  if (saving.value || props.busy) return;
  const id = key(skin);
  saving.value = id;
  emit("busy", true);
  try {
    await texture(skin);
    await action("store_save_skin", {
      input: skin,
      slim: skin.slim ?? !!slim.value[id],
    });
    if (!disposed) {
      saved.value.add(id);
      emit("downloaded");
    }
  } catch (error) {
    if (!disposed) emit("error", error);
  } finally {
    saving.value = "";
    emit("busy", false);
  }
}
async function importPng() {
  if (saving.value || props.busy) return;
  saving.value = "import";
  emit("busy", true);
  try {
    const path = await open({
      multiple: false,
      filters: [{ name: "PNG", extensions: ["png"] }],
    });
    if (typeof path !== "string" || disposed) return;
    await action("import_skin", { path });
    emit("downloaded");
  } catch (error) {
    if (!disposed) emit("error", error);
  } finally {
    saving.value = "";
    emit("busy", false);
  }
}
function searchNow() {
  if (saving.value || props.busy) return;
  clearTimeout(timer);
  void load();
}
watch(source, searchNow);
watch(query, () => {
  clearTimeout(timer);
  invalidate(true);
  timer = setTimeout(() => void load(), 300);
});
onMounted(() => void load());
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  clearTimeout(timer);
  thumbnails.dispose();
  for (const next of waiters.splice(0)) next();
});
</script>
<template>
  <section class="store-skins">
    <form class="catalog-toolbar store-skin-tools" @submit.prevent="searchNow">
      <label class="resource-search"
        ><Search :size="14" />
        <input
          v-model="query"
          :disabled="!!saving || busy"
          maxlength="100"
          :placeholder="t(source === 'elyby' ? '搜索皮肤标签' : '搜索皮肤')"
          :aria-label="t('搜索皮肤')"
        />
        <button
          v-if="query"
          class="icon-button"
          type="button"
          :disabled="!!saving || busy"
          :aria-label="t('清空搜索')"
          @click="query = ''"
        >
          <X :size="14" />
        </button>
      </label>
      <div class="store-skin-sources">
        <GlassSelect
          id="store-skin-source"
          v-model="source"
          :options="sources"
          :disabled="!!saving || busy"
          :aria-label="t('来源')"
          :menu-min-width="180"
          compact
          floating
          literal-labels
        />
        <button
          class="icon-button"
          type="button"
          :title="catalog.label"
          :aria-label="t('访问网站')"
          @click="emit('link', catalog.url)"
        >
          <ExternalLink :size="15" />
        </button>
        <button
          class="icon-button"
          type="button"
          :disabled="!!saving || busy"
          :title="t('导入 PNG')"
          :aria-label="t('导入 PNG')"
          @click="importPng"
        >
          <Upload :size="16" />
        </button>
      </div>
    </form>
    <div
      ref="viewport"
      class="store-skin-scroll"
      data-list-viewport
      data-catalog-viewport
    >
      <div v-if="failure && !skins.length" class="store-skin-unavailable">
        <p>{{ translateMessage(failure) }}</p>
        <div>
          <button class="text-button" @click="emit('link', catalog.url)">
            <ExternalLink :size="15" />{{ t("访问网站") }}
          </button>
          <button class="text-button" :disabled="loading" @click="load()">
            <RefreshCw :size="14" />{{ t("重试") }}
          </button>
        </div>
      </div>
      <div
        v-else-if="!loading && !skins.length && !more"
        class="store-skin-unavailable"
      >
        {{ t("暂无结果") }}
      </div>
      <div class="store-skin-grid">
        <button
          v-for="(skin, index) in skins"
          :key="key(skin)"
          class="store-skin-tile list-reveal-item"
          :style="listRevealStyle(index % 28)"
          :disabled="!!saving || busy"
          :title="
            [
              skin.name,
              skin.author,
              t(saved.has(key(skin)) ? '已下载' : '下载到皮肤库'),
            ]
              .filter(Boolean)
              .join(' · ')
          "
          :aria-label="`${t('下载到皮肤库')} ${skin.name}`"
          @click="collect(skin)"
        >
          <SkinLibraryItem
            :load="() => texture(skin)"
            :render-model="thumbnails.render"
            :slim="skin.slim ?? !!slim[key(skin)]"
            :live="false"
          />
          <small>{{ skin.name }}</small>
          <span
            class="store-skin-action"
            :class="{ saved: saved.has(key(skin)) }"
          >
            <LoaderCircle v-if="saving === key(skin)" class="spin" :size="16" />
            <Check v-else-if="saved.has(key(skin))" :size="16" /><Download
              v-else
              :size="16"
            />
          </span>
        </button>
      </div>
      <CatalogTail
        v-model:position="scrollTop"
        :loading="loading"
        :failure="skins.length ? failure : ''"
        :more="more"
        :disabled="!!saving"
        @load="load(true)"
        @retry="load(!!skins.length)"
      />
    </div>
  </section>
</template>
<style scoped>
.store-skins {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  padding: 0 var(--panel-inset) var(--panel-inset);
  gap: 16px;
}
.store-skin-tools {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: none;
}
.store-skin-tools label {
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 0;
  gap: 8px;
  color: var(--muted);
  border-bottom: 1px solid transparent;
}
.store-skin-tools label:focus-within {
  border-color: var(--blue);
}
.store-skin-tools input {
  flex: 1;
  min-width: 0;
  padding: 0;
  height: 36px;
  border: 0;
  outline: 0;
  box-shadow: none;
  background: none;
  color: var(--ink);
  font-size: 12px;
}
.store-skin-sources {
  display: flex;
  align-items: center;
  gap: 8px;
}
.store-skin-sources :deep(.source-select) {
  width: 166px;
}
.store-skin-sources :deep(.source-select-trigger) {
  height: 36px;
  font-size: 12px;
}
.store-skin-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}
.store-skin-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(112px, 1fr));
  gap: 14px;
}
.store-skin-tile {
  position: relative;
  display: flex;
  align-items: center;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
  border: 0;
  background: transparent;
  padding: 4px;
  color: var(--muted);
}
.store-skin-tile:hover {
  color: var(--blue);
}
.store-skin-tile small {
  font-size: 11px;
  line-height: 16px;
  max-width: 100%;
  white-space: nowrap;
  text-overflow: ellipsis;
  overflow: hidden;
}
.store-skin-action {
  position: absolute;
  right: 4px;
  top: 4px;
  opacity: 0;
  transition: opacity var(--motion-fade);
}
.store-skin-action.saved,
.store-skin-tile:hover .store-skin-action,
.store-skin-tile:focus-visible .store-skin-action,
.store-skin-tile:disabled .store-skin-action {
  opacity: 1;
}
.store-skin-unavailable {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 250px;
  color: var(--muted);
  gap: 10px;
}
.store-skin-unavailable p {
  max-width: 400px;
  text-align: center;
  font-size: 12px;
  line-height: 1.8;
}
.store-skin-unavailable > div {
  display: flex;
  gap: 20px;
}
</style>
