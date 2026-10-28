<script setup lang="ts">
import {
  computed,
  inject,
  nextTick,
  onBeforeUnmount,
  ref,
  shallowRef,
  watch,
} from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Coffee,
  Package,
  Shirt,
  HardDrive,
  X,
  Plus,
  FolderOpen,
  Download,
  Trash2,
  Search,
  Image,
  FileCode2,
  Sparkles,
  ChevronRight,
  LoaderCircle,
} from "lucide-vue-next";
import GlassSelect from "./GlassSelect.vue";
import ContentSwitch from "./ContentSwitch.vue";
import SkinStage from "./SkinStage.vue";
import SkinLibraryItem from "./SkinLibraryItem.vue";
import VersionContentItem from "./VersionContentItem.vue";
import { t } from "../i18n";
import { formatBytes as formatSmallBytes } from "../downloadWorkflow";
const formatBytes = (bytes: number) =>
  bytes >= 1024 ** 3
    ? `${(bytes / 1024 ** 3).toFixed(1)} GiB`
    : formatSmallBytes(bytes);
import { listRevealStyle } from "../listMotion";
import { createVersionContentLoader } from "../versionContentLoader";
import { modFileName } from "../contentIdentity";
import {
  resourceTransferKey,
  isResourceCopyKind,
} from "../useResourceTransfer";
import { createSkinThumbnails } from "../skinThumbnails";
import { builtinSkins } from "../builtinSkins";
import {
  mergeBuiltinAccountSkins,
  isGuestSkin,
  type SavedSkin,
  type SkinChoice,
} from "../skinLibrary";
import type {
  Account,
  ContentKind,
  ContentMetadata,
  LocalContentFile,
  VersionContent,
} from "../types";
import type { ModActions } from "../modManagement";

type Rpc = <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
type Runtime = {
  path: string;
  major: number;
  version: string;
  arch: string;
  jdk: boolean;
  source: string;
  size: number;
  used_by: string[];
};
type Owner = { id: string; version: string; label: string };
type Group = { directory: string; owners: Owner[] };
type ResourceRow = {
  group: string;
  key: string;
  file: LocalContentFile;
  directory: string;
  owners: Owner[];
};
type CachePart = { kind: "java" | "skins"; size: number; files: number };
type CacheEntry = {
  kind: "metadata" | "resources" | "partials";
  size: number;
  files: number;
  parts: CachePart[];
};
const props = defineProps<{
  rpc: Rpc;
  busy: boolean;
  running: string[];
  modActions: ModActions;
  installJava: (major: number) => Promise<unknown>;
  accounts: Account[];
  selectedAccount: string | null;
  guestSkin: string;
  guestName: string;
  selectSkin: (id: string) => Promise<Account | null>;
  loadAccountSkin: (id: string) => Promise<string | null>;
}>();
const emit = defineEmits<{
  close: [];
  error: [error: unknown];
  busy: [value: boolean];
  "skin-preview": [value: boolean];
  "skin-changed": [];
  manage: [id: string, version: string, kind: ContentKind];
}>();
const transferResource = inject(resourceTransferKey, undefined);
function beginResourceDrag(event: PointerEvent, row: ResourceRow) {
  const owner = row.owners[0];
  if (!owner || !isResourceCopyKind(resourceKind.value) || locked(row)) return;
  transferResource?.(event, {
    kind: resourceKind.value,
    instance: owner.id,
    version: owner.version,
    file: row.file.file_name,
    name: resourceName(row),
  });
}
const tabs = [
  { id: "java", label: "Java", icon: Coffee, color: "#e7bc76" },
  { id: "resources", label: "游戏资源", icon: Package, color: "#69dcb5" },
  { id: "skins", label: "皮肤", icon: Shirt, color: "#b49aef" },
  { id: "cache", label: "缓存", icon: HardDrive, color: "#7dbfee" },
];
const tab = ref("java");
const loading = ref(false);
const mutating = ref(false);
const installing = ref(false);
const loaded = ref(false);
const query = ref("");
const runtimes = shallowRef<Runtime[]>([]);
const major = ref("21");
const javaPath = ref("");
const addingJava = ref(false);
const selectedJava = computed(() =>
  runtimes.value.find((runtime) => runtime.path === javaPath.value),
);
const javaVersion = (runtime: Runtime) =>
  runtime.version.match(/version "([^"]+)"/)?.[1] || runtime.version;
const javaSource = (runtime: Runtime) =>
  t(
    {
      managed: "启动器托管",
      registered: "手动登记",
      system: "系统安装",
      configured: "实例指定",
    }[runtime.source] || runtime.source,
  );
watch(runtimes, (value) => {
  if (!value.some((runtime) => runtime.path === javaPath.value))
    javaPath.value = value[0]?.path || "";
});
const savedSkins = shallowRef<SavedSkin[]>([]);
const skins = computed<SkinChoice[]>(() =>
  mergeBuiltinAccountSkins(builtinSkins, props.accounts, savedSkins.value),
);
const applyingSkin = ref("");
const skinId = ref("");
let skinPicked = false;
const skin = computed(() => skins.value.find((s) => s.id === skinId.value));
watch(
  () => tab.value === "skins",
  (value) => emit("skin-preview", value),
  { immediate: true },
);
const skinName = ref("");
const texture = ref<string | null>(null);
const textureLoading = ref(false);
const thumbnails = createSkinThumbnails();
const liveTextureKey = ref("");
const previewTextureKey = computed(() =>
  JSON.stringify([texture.value, !!skin.value?.slim]),
);
const liveSkinReady = computed(
  () =>
    !textureLoading.value && liveTextureKey.value === previewTextureKey.value,
);
const skinTarget = computed(
  () => `#rm-skin-live-${skins.value.findIndex((s) => s.id === skinId.value)}`,
);
const skinUsers = (item: SkinChoice) =>
  [
    ...(item.users?.map((user) => user.name) || []),
    ...(isGuestSkin(item, props.guestSkin)
      ? [`${t("游客")} · ${props.guestName}`]
      : []),
  ].join(", ");
const skinLabel = (item: SkinChoice) => {
  const users = skinUsers(item);
  if (!users) return item.name;
  const usage = t("{name} 正在使用", { name: users });
  return item.account ? usage : `${item.name} · ${usage}`;
};
const localTextures = new Map<string, Promise<string>>();
const localTextureUrls = new Set<string>();
let disposed = false;
const cache = shallowRef<CacheEntry[]>([]);
const cacheLabels = {
  metadata: "索引与元数据",
  resources: "资源缓存",
  partials: "未完成下载",
};
const cachePartLabels = { java: "Java", skins: "皮肤" };
const cachePartColors = { java: "#e7bc76", skins: "#b49aef" };
const cacheUsageLabel = (entry: CacheEntry) =>
  (entry.parts || [])
    .map((part) => `${t(cachePartLabels[part.kind])} ${formatBytes(part.size)}`)
    .join(" · ");
const confirm = shallowRef<{
  title: string;
  detail: string;
  run: () => Promise<void>;
}>();
const resourceKind = ref<ContentKind>("mods");
// Keep a dense grid's total entrance close to the small Java list.
const resourceRowStagger = 3;
const kinds = [
  { value: "mods", label: "Mods", icon: Package },
  { value: "resourcepacks", label: "资源包", icon: Image },
  { value: "shaderpacks", label: "光影", icon: Sparkles },
  { value: "datapacks", label: "数据包", icon: FileCode2 },
];
const resourceIcon = computed(
  () => kinds.find((k) => k.value === resourceKind.value)!.icon,
);
const resources = shallowRef<ResourceRow[]>([]);
let resourceGroups = new Map<string, ResourceRow[]>();
const readyGroups = shallowRef(new Set<string>());
const scanIssues = ref<string[]>([]);
const scanned = ref(0);
const totalGroups = ref(0);
const loader = createVersionContentLoader(props.rpc);
let controller: AbortController | undefined;
let generation = 0;
let skinGeneration = 0;
const library = <T = unknown,>(
  kind: string,
  data: Record<string, unknown> = {},
) => props.rpc<T>("resource_manager", { action: { kind, ...data } });
const filtered = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase();
  return resources.value.filter(
    (row) =>
      !needle ||
      [
        row.file.name,
        row.file.file_name,
        ...row.owners.map((o) => o.label),
      ].some((s) => s.toLocaleLowerCase().includes(needle)),
  );
});
const resourceKey = ref("");
const selectedResource = computed(() =>
  filtered.value.find((row) => row.key === resourceKey.value),
);
const resourceChanged = computed(() => {
  const row = selectedResource.value;
  return () => {
    if (row) void changed(row);
  };
});
const resourceDetail = ref<InstanceType<typeof VersionContentItem>>();
const resourceName = (row: ResourceRow) =>
  row.file.name.replace(/\.(?:jar|zip)(?:\.(?:disabled|off))?$/i, "");
watch(
  () => selectedResource.value?.key,
  async (key) => {
    if (!key) return;
    await nextTick();
    if (selectedResource.value?.key === key)
      await resourceDetail.value?.reveal();
  },
);
const emptyDependencies = new Map<string, string>();
const dependencies = computed(() => {
  const groups = new Map<string, Map<string, string>>();
  for (const row of resources.value) {
    let entries = groups.get(row.directory);
    if (!entries) {
      entries = new Map();
      groups.set(row.directory, entries);
    }
    for (const id of row.file.mod_ids || [])
      entries.set(id, row.file.file_name);
  }
  return groups;
});
function locked(row: ResourceRow) {
  return (
    props.busy ||
    mutating.value ||
    !readyGroups.value.has(row.group) ||
    row.owners.some((o) => props.running.includes(o.id))
  );
}
const loadMetadata = (
  id: string,
  version: string,
  kind: ContentKind,
  file: string,
) =>
  props.rpc<ContentMetadata>("version_content_metadata", {
    id,
    version,
    kind,
    file,
  });
const openFolder = async (
  id: string,
  version: string,
  kind: ContentKind,
  file: string,
  source?: string,
) => {
  await props.rpc("open_content_folder", { id, version, kind, file, source });
};
const loadSource = (
  id: string,
  version: string,
  kind: ContentKind,
  file: string,
  source: string,
) =>
  props.rpc<string>("version_content_source", {
    id,
    version,
    kind,
    file,
    source,
  });
async function run(work: () => Promise<void>) {
  if (mutating.value) return;
  mutating.value = true;
  emit("busy", true);
  try {
    await work();
  } catch (error) {
    emit("error", error);
  } finally {
    mutating.value = false;
    emit("busy", false);
  }
}
async function refresh() {
  controller?.abort();
  controller = new AbortController();
  const signal = controller.signal,
    request = ++generation,
    active = tab.value;
  loading.value = true;
  loaded.value = false;
  confirm.value = undefined;
  try {
    if (active === "java") {
      const value = await library<Runtime[]>("java");
      if (request === generation) runtimes.value = value;
    }
    if (active === "cache") {
      const value = await library<CacheEntry[]>("cache");
      if (request === generation) cache.value = value;
    }
    if (active === "skins") {
      const value = await library<SavedSkin[]>("skins");
      if (request === generation) {
        savedSkins.value = value;
      }
    }
    if (active === "resources") {
      const kind = resourceKind.value;
      const index = await library<{ groups: Group[]; issues: string[] }>(
        "groups",
        { content: kind },
      );
      if (request !== generation) return;
      scanIssues.value = index.issues;
      scanned.value = 0;
      totalGroups.value = index.groups.length;
      const results = (resourceGroups = new Map<string, ResourceRow[]>());
      readyGroups.value = new Set();
      const publish = (group: Group, content: VersionContent) => {
        if (request !== generation) return;
        results.set(
          group.directory,
          content.files
            .filter((f) => !f.declaration || f.declaration.status === "local")
            .map((file) => ({
              group: group.directory,
              key: JSON.stringify([
                group.directory,
                modFileName(file.file_name),
              ]),
              file,
              directory: content.directory,
              owners: group.owners,
            })),
        );
        resources.value = [...results.values()].flat();
      };
      resources.value = [];
      // Scan one shared working directory once; metadata batches paint progressively.
      for (const group of index.groups) {
        if (signal.aborted) return;
        const owner = group.owners[0]!;
        try {
          publish(
            group,
            await loader(owner.id, owner.version, kind, {
              signal,
              fresh: true,
              onProgress: (c) => publish(group, c),
            }),
          );
        } catch (error) {
          if (signal.aborted) return;
          scanIssues.value.push(`${owner.label}: ${String(error)}`);
        }
        if (request === generation) {
          readyGroups.value = new Set([...readyGroups.value, group.directory]);
          scanned.value++;
        }
      }
    }
  } catch (error) {
    if (request === generation && !signal.aborted) emit("error", error);
  } finally {
    if (request === generation) {
      loading.value = false;
      loaded.value = true;
    }
  }
}
watch(
  [tab, resourceKind],
  () => {
    query.value = "";
    resourceKey.value = "";
    void refresh();
  },
  { immediate: true },
);
watch(skin, (value) => {
  skinName.value = value?.name || "";
});
watch(
  skins,
  (value) => {
    if (!skinPicked || !value.some((s) => s.id === skinId.value))
      skinId.value =
        value.find((s) =>
          props.selectedAccount
            ? s.users?.some((user) => user.id === props.selectedAccount)
            : isGuestSkin(s, props.guestSkin),
        )?.id ||
        value[0]?.id ||
        "";
  },
  { immediate: true },
);
let skinPointer: [number, number] | undefined;
let skinDragged = false;
function beginSkinPointer(event: PointerEvent) {
  skinPointer = [event.clientX, event.clientY];
  skinDragged = event.button !== 0;
}
function moveSkinPointer(event: PointerEvent) {
  if (
    skinPointer &&
    Math.hypot(event.clientX - skinPointer[0], event.clientY - skinPointer[1]) >
      6
  )
    skinDragged = true;
}
async function pickSkin(item: SkinChoice, event: MouseEvent) {
  if (mutating.value || applyingSkin.value || (event.detail && skinDragged))
    return;
  const previous = skinId.value;
  skinPicked = true;
  skinId.value = item.id;
  const inUse = props.selectedAccount
    ? item.users?.some((user) => user.id === props.selectedAccount)
    : isGuestSkin(item, props.guestSkin);
  if (inUse) return;
  applyingSkin.value = item.id;
  try {
    const account = await props.selectSkin(item.id);
    if (account?.skin && !item.account && !item.builtin) {
      savedSkins.value = savedSkins.value.map((skin) =>
        skin.id === item.id
          ? { ...skin, texture_url: account.skin!.url }
          : skin,
      );
    }
  } catch (error) {
    skinId.value = previous;
    emit("error", error);
  } finally {
    applyingSkin.value = "";
  }
}
function loadSkin(item: SkinChoice): Promise<string | null> {
  if (item.builtin) return Promise.resolve(item.texture || null);
  if (item.account) return props.loadAccountSkin(item.account);
  let pending = localTextures.get(item.id);
  if (!pending) {
    pending = library<number[]>("skin_bytes", { id: item.id })
      .then((bytes) => {
        if (disposed) return "";
        const url = URL.createObjectURL(
          new Blob([new Uint8Array(bytes)], { type: "image/png" }),
        );
        localTextureUrls.add(url);
        return url;
      })
      .catch((error) => {
        localTextures.delete(item.id);
        throw error;
      });
    localTextures.set(item.id, pending);
  }
  return pending;
}
watch(
  () =>
    tab.value === "skins"
      ? JSON.stringify([skin.value?.id, skin.value?.textureKey])
      : null,
  async () => {
    const request = ++skinGeneration;
    const selected = skin.value;
    textureLoading.value = tab.value === "skins" && !!selected;
    if (tab.value !== "skins") return;
    if (!selected) {
      texture.value = null;
      return;
    }
    try {
      const value = await loadSkin(selected);
      if (request === skinGeneration) texture.value = value;
    } catch (error) {
      if (request === skinGeneration) {
        texture.value = null;
        emit("error", error);
      }
    } finally {
      if (request === skinGeneration) textureLoading.value = false;
    }
  },
);
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  skinGeneration++;
  controller?.abort();
  thumbnails.dispose();
  for (const url of localTextureUrls) URL.revokeObjectURL(url);
});
async function registerJava() {
  try {
    const path = await open({
      multiple: false,
      title: t("选择 Java 可执行文件"),
    });
    if (typeof path === "string")
      await run(async () => {
        const registered = await library<{ path: string }>("register_java", {
          path,
        });
        await refresh();
        javaPath.value = registered.path;
        addingJava.value = false;
      });
  } catch (error) {
    emit("error", error);
  }
}
async function install() {
  await run(async () => {
    const previous = new Set(runtimes.value.map((runtime) => runtime.path));
    installing.value = true;
    try {
      await props.installJava(Number(major.value));
      await refresh();
      javaPath.value =
        runtimes.value.find((runtime) => !previous.has(runtime.path))?.path ||
        runtimes.value.find((runtime) => runtime.major === Number(major.value))
          ?.path ||
        javaPath.value;
      addingJava.value = false;
    } finally {
      installing.value = false;
    }
  });
}
function removeJava(runtime: Runtime) {
  confirm.value = {
    title: runtime.source === "managed" ? t("移入回收站") : t("取消登记"),
    detail:
      runtime.source === "managed"
        ? runtime.path
        : t("只取消登记，保留原文件。"),
    run: async () => {
      await library("remove_java", { path: runtime.path });
      await refresh();
    },
  };
}
async function importPaths(paths: string[]) {
  await run(async () => {
    for (const path of paths) {
      const value = await library<SavedSkin>("import_skin", { path });
      skinPicked = true;
      skinId.value = value.id;
    }
    await refresh();
  });
}
async function importSkins() {
  try {
    const paths = await open({
      multiple: true,
      filters: [{ name: "Minecraft skin", extensions: ["png"] }],
    });
    if (paths) await importPaths(typeof paths === "string" ? [paths] : paths);
  } catch (error) {
    emit("error", error);
  }
}
async function editSkin(slim = skin.value?.slim) {
  if (!skin.value || skin.value.account || skin.value.builtin) return;
  await run(async () => {
    await library("edit_skin", {
      id: skinId.value,
      name: skinName.value,
      slim: !!slim,
    });
    await refresh();
    emit("skin-changed");
  });
}
async function exportSkin() {
  const selected = skin.value;
  if (!selected) return;
  try {
    const path = await save({
      defaultPath: `${selected.name.replace(/[<>:"/\\|?*]/g, "_")}.png`,
      filters: [{ name: "PNG", extensions: ["png"] }],
    });
    if (path)
      await run(async () => {
        await library(
          selected.account ? "export_account_skin" : "export_skin",
          { id: selected.account || selected.id, path },
        );
      });
  } catch (error) {
    emit("error", error);
  }
}
function deleteSkin() {
  if (skin.value && !skin.value.account && !skin.value.builtin)
    confirm.value = {
      title: t("移入回收站"),
      detail: skin.value.name,
      run: async () => {
        await library("delete_skin", { id: skinId.value });
        await refresh();
        emit("skin-changed");
      },
    };
}
function clearCache(entry: CacheEntry) {
  if (entry.kind !== "partials") return;
  confirm.value = {
    title: t("清理") + " · " + t(cacheLabels[entry.kind]),
    detail: t("清理后需要重新下载这些文件。"),
    run: async () => {
      await library("clear_cache", { category: entry.kind });
      await refresh();
    },
  };
}
async function accept() {
  const pending = confirm.value;
  confirm.value = undefined;
  if (pending) await run(pending.run);
}
async function openJava(path: string) {
  try {
    await library("open_java", { path });
  } catch (error) {
    emit("error", error);
  }
}
async function changed(row: ResourceRow) {
  const owner = row.owners[0]!;
  const request = generation,
    kind = resourceKind.value;
  try {
    const content = await loader(owner.id, owner.version, kind, {
      fresh: true,
    });
    if (generation !== request) return;
    const updated = new Map(
      content.files.map((file) => [modFileName(file.file_name), file]),
    );
    resourceGroups.set(
      row.group,
      (resourceGroups.get(row.group) || []).flatMap((previous) => {
        const file = updated.get(modFileName(previous.file.file_name));
        return file ? [{ ...previous, file }] : [];
      }),
    );
    resources.value = [...resourceGroups.values()].flat();
  } catch (error) {
    emit("error", error);
  }
}
function showMod(id: string, version: string) {
  emit("manage", id, version, "mods");
}
async function drop(paths: string[]) {
  if (tab.value === "skins") {
    const images = paths.filter((p) => /\.png$/i.test(p));
    if (!images.length) emit("error", t("请选择 PNG 皮肤文件"));
    else await importPaths(images);
  } else if (tab.value === "java" && paths.length === 1)
    await run(async () => {
      const registered = await library<{ path: string }>("register_java", {
        path: paths[0],
      });
      await refresh();
      javaPath.value = registered.path;
      addingJava.value = false;
    });
  else
    emit("error", t("请在皮肤页拖入 PNG，或在 Java 页拖入 Java 可执行文件。"));
}
defineExpose({ drop, refresh });
</script>

<template>
  <section class="modal glass-surface resource-manager">
    <header class="modal-header">
      <nav
        class="rm-tabs dialog-heading"
        tabindex="-1"
        :aria-label="t('资源管理')"
      >
        <button
          v-for="item in tabs"
          :key="item.id"
          :style="{ '--tab-color': item.color }"
          :class="{ active: tab === item.id }"
          :disabled="mutating"
          :aria-pressed="tab === item.id"
          @click="tab = item.id"
        >
          <component :is="item.icon" :size="18" /><span>{{
            t(item.label)
          }}</span>
        </button>
      </nav>
      <button
        class="icon-button"
        :title="t('关闭')"
        :aria-label="t('关闭')"
        :disabled="mutating"
        @click="emit('close')"
      >
        <X :size="20" />
      </button>
    </header>
    <ContentSwitch
      :content-key="tab === 'resources' ? `${tab}-${resourceKind}` : tab"
    >
      <div v-if="tab === 'java'" class="rm-page">
        <div class="rm-tile-grid rm-scroll" data-list-viewport>
          <button
            v-for="(runtime, index) in runtimes"
            :key="runtime.path"
            class="rm-tile rm-java-tile list-reveal-item"
            :style="listRevealStyle(index)"
            :class="{ active: !addingJava && javaPath === runtime.path }"
            :aria-pressed="!addingJava && javaPath === runtime.path"
            :aria-label="`Java ${runtime.major} · ${javaVersion(runtime)} · ${runtime.arch} · ${javaSource(runtime)}`"
            :title="`${javaVersion(runtime)} · ${runtime.arch} · ${javaSource(runtime)}\n${runtime.path}`"
            :disabled="mutating"
            @click="
              javaPath = runtime.path;
              addingJava = false;
            "
          >
            <Coffee :size="34" :stroke-width="1.3" class="rm-tile-icon" />
            <span class="rm-tile-name">Java {{ runtime.major }}</span>
            <span class="rm-tile-caption"
              >{{ runtime.jdk ? "JDK" : "JRE" }} · {{ runtime.arch }}</span
            >
          </button>
          <button
            class="rm-tile rm-add-tile rm-java-add list-reveal-item"
            :style="listRevealStyle(runtimes.length)"
            :class="{ active: addingJava }"
            :aria-pressed="addingJava"
            :aria-label="t('添加 Java')"
            :title="t('添加 Java')"
            :disabled="busy || mutating"
            @click="addingJava = !addingJava"
          >
            <Plus :size="24" :stroke-width="1.4" />
          </button>
        </div>
        <div v-if="addingJava || selectedJava" class="rm-java-details">
          <ContentSwitch :content-key="addingJava ? 'add-java' : javaPath">
            <div
              v-if="addingJava"
              class="rm-java-add-actions list-reveal-item"
              :style="listRevealStyle(0)"
            >
              <div class="rm-actions">
                <GlassSelect
                  id="library-java"
                  v-model="major"
                  :disabled="busy || mutating"
                  :options="
                    [8, 17, 21, 25, 26].map((n) => ({
                      value: String(n),
                      label: `Java ${n}`,
                    }))
                  "
                  compact
                  floating
                />
                <button
                  class="text-action"
                  :disabled="busy || mutating"
                  @click="install"
                >
                  <Download :size="15" />{{ t("获取") }}
                </button>
              </div>
              <button
                class="text-action"
                :disabled="busy || mutating"
                @click="registerJava"
              >
                <FolderOpen :size="16" />{{ t("登记本地") }}
              </button>
            </div>
            <div
              v-else-if="selectedJava"
              class="rm-java-detail list-reveal-item"
              :style="listRevealStyle(0)"
            >
              <div class="rm-description">
                <div class="rm-title">
                  <strong>Java {{ javaVersion(selectedJava) }}</strong>
                  <span
                    >{{ selectedJava.arch }} ·
                    {{ selectedJava.jdk ? "JDK" : "JRE" }}</span
                  >
                  <small>{{ javaSource(selectedJava) }}</small>
                </div>
                <button class="rm-path" @click="openJava(selectedJava.path)">
                  <FolderOpen :size="13" /><span>{{ selectedJava.path }}</span>
                </button>
                <div v-if="selectedJava.used_by.length" class="rm-used">
                  {{ selectedJava.used_by.join(" · ") }}
                </div>
              </div>
              <span v-if="selectedJava.size" class="rm-muted rm-size">{{
                formatBytes(selectedJava.size)
              }}</span>
              <button
                v-if="['managed', 'registered'].includes(selectedJava.source)"
                class="icon-button"
                :disabled="
                  busy ||
                  mutating ||
                  running.length > 0 ||
                  selectedJava.used_by.length > 0
                "
                :title="
                  selectedJava.used_by.length
                    ? t('正在被实例使用')
                    : t(
                        selectedJava.source === 'managed'
                          ? '移入回收站'
                          : '取消登记',
                      )
                "
                :aria-label="t('移除')"
                @click="removeJava(selectedJava)"
              >
                <Trash2 :size="16" />
              </button>
            </div>
          </ContentSwitch>
        </div>
        <footer v-if="installing" class="rm-footer">
          <span
            ><LoaderCircle class="spinning" :size="14" />{{
              t("正在获取 Java")
            }}
            <button
              class="text-action"
              @click="rpc('cancel').catch((e) => emit('error', e))"
            >
              {{ t("取消") }}
            </button></span
          >
        </footer>
      </div>
      <div v-else-if="tab === 'resources'" class="rm-page">
        <div
          class="rm-toolbar list-reveal-item"
          :style="listRevealStyle(0)"
          data-list-footer
        >
          <div class="rm-actions">
            <button
              v-for="kind in kinds"
              :key="kind.value"
              class="rm-kind"
              :class="{ active: resourceKind === kind.value }"
              @click="resourceKind = kind.value as ContentKind"
            >
              <component :is="kind.icon" :size="15" />{{ t(kind.label) }}
            </button>
          </div>
          <div class="rm-resource-tools">
            <label class="rm-search">
              <Search :size="14" /><input
                v-model="query"
                :placeholder="t('搜索')"
                :aria-label="t('搜索')"
              />
            </label>
            <span class="rm-muted rm-resource-count">{{
              filtered.length
            }}</span>
          </div>
        </div>
        <div
          class="rm-tile-grid rm-scroll rm-resources"
          data-list-viewport
          :data-list-stagger="resourceRowStagger"
        >
          <button
            v-for="(row, index) in filtered"
            :key="row.key"
            class="rm-tile rm-resource-tile list-reveal-item"
            :style="listRevealStyle(index, 20, resourceRowStagger)"
            :class="{
              active: resourceKey === row.key,
              'resource-drag-handle':
                isResourceCopyKind(resourceKind) &&
                !!transferResource &&
                !locked(row),
              'is-disabled': resourceKind === 'mods' && !row.file.enabled,
            }"
            :aria-pressed="resourceKey === row.key"
            :aria-label="resourceName(row)"
            :title="`${resourceName(row)}${row.file.version ? ' · ' + row.file.version : ''}\n${row.owners.map((owner) => owner.label).join(' · ')}${resourceKind === 'mods' && !row.file.enabled ? '\n' + t('已禁用') : ''}`"
            @click="resourceKey = resourceKey === row.key ? '' : row.key"
            @pointerdown="beginResourceDrag($event, row)"
            @dragstart.prevent
          >
            <component
              :is="resourceIcon"
              :size="32"
              :stroke-width="1.3"
              class="rm-tile-icon"
            />
            <span class="rm-tile-name">{{ resourceName(row) }}</span>
            <span
              v-if="resourceKind === 'mods' && !row.file.enabled"
              class="rm-tile-status"
              :aria-label="t('已禁用')"
            />
          </button>
          <p v-if="loaded && !filtered.length" class="rm-empty rm-grid-empty">
            {{ t(query ? "没有匹配的资源" : "暂无本地资源") }}
          </p>
        </div>
        <div v-if="selectedResource" class="rm-resource-details">
          <ContentSwitch :content-key="selectedResource.key">
            <div
              class="rm-resource-detail-scroll rm-scroll list-reveal-item"
              :style="listRevealStyle(0)"
            >
              <button
                class="icon-button rm-detail-close"
                :aria-label="t('收起详情')"
                :title="t('收起详情')"
                @click="resourceKey = ''"
              >
                <X :size="16" />
              </button>
              <VersionContentItem
                ref="resourceDetail"
                :file="selectedResource.file"
                :instance-id="selectedResource.owners[0]!.id"
                :version="selectedResource.owners[0]!.version"
                :kind="resourceKind"
                :icon="resourceIcon"
                :locked="locked(selectedResource)"
                :mod-actions="modActions"
                :load-metadata="loadMetadata"
                :open-folder="openFolder"
                :load-source="loadSource"
                :dependency-targets="
                  dependencies.get(selectedResource.directory) ||
                  emptyDependencies
                "
                @changed="resourceChanged"
                @show-mod="showMod"
                @error="emit('error', $event)"
              />
              <div class="rm-owners">
                <button
                  v-for="owner in selectedResource.owners"
                  :key="owner.id + owner.version"
                  @click="emit('manage', owner.id, owner.version, resourceKind)"
                >
                  {{ owner.label }}<ChevronRight :size="12" />
                </button>
              </div>
            </div>
          </ContentSwitch>
        </div>
        <footer
          v-if="loading || scanIssues.length"
          class="rm-footer"
          data-list-footer
        >
          <span v-if="loading"
            ><LoaderCircle class="spinning" :size="13" />{{ scanned }} /
            {{ totalGroups }}</span
          ><button
            v-else-if="scanIssues.length"
            class="text-action"
            @click="emit('error', scanIssues.join('\n'))"
          >
            {{ t("部分目录无法读取") }} ({{ scanIssues.length }})
          </button>
        </footer>
      </div>
      <div v-else-if="tab === 'skins'" class="rm-page rm-skins-page">
        <div class="rm-skin-grid rm-scroll">
          <article
            v-for="(item, index) in skins"
            :key="item.id"
            class="rm-skin-card list-reveal-item"
            :style="listRevealStyle(index)"
            :class="{ active: skinId === item.id }"
            :title="skinLabel(item)"
            :aria-busy="applyingSkin === item.id"
            @pointerdown.capture="beginSkinPointer"
            @pointermove.capture="moveSkinPointer"
            @pointercancel="skinDragged = true"
            @click="pickSkin(item, $event)"
          >
            <button
              class="rm-skin-pick"
              :aria-label="skinLabel(item)"
              :aria-pressed="skinId === item.id"
              :disabled="mutating || !!applyingSkin"
            >
              <SkinLibraryItem
                :key="item.id + (item.textureKey || '')"
                :load="() => loadSkin(item)"
                :render-model="thumbnails.render"
                :slim="item.slim"
                :in-use="item.users?.map((user) => user.name).join(', ')"
                :guest="isGuestSkin(item, guestSkin)"
                :live="skinId === item.id && liveSkinReady"
              />
            </button>
            <div
              :id="`rm-skin-live-${index}`"
              class="rm-skin-live"
              :class="{ ready: skinId === item.id && liveSkinReady }"
            />
            <span
              v-if="applyingSkin === item.id"
              class="rm-skin-applying"
              role="status"
              :aria-label="t('正在应用皮肤')"
            >
              <LoaderCircle class="spinning" :size="14" />
            </span>
          </article>
          <button
            class="rm-skin-import list-reveal-item"
            :style="listRevealStyle(skins.length)"
            :disabled="busy || mutating"
            :aria-label="t('导入 PNG')"
            :title="t('导入 PNG')"
            @click="importSkins"
          >
            <Plus :size="24" :stroke-width="1.4" />
          </button>
        </div>
        <Teleport v-if="skin" :to="skinTarget" defer>
          <SkinStage
            :texture="texture"
            :slim="skin.slim"
            :loading="textureLoading"
            :paused="tab !== 'skins' || textureLoading"
            :caption="skinLabel(skin)"
            issue=""
            @avatar="liveTextureKey = previewTextureKey"
            @failed="emit('error', $event)"
          />
        </Teleport>
        <footer
          v-if="skin"
          class="rm-skin-details list-reveal-item"
          :style="listRevealStyle(1)"
        >
          <span v-if="skin.builtin" class="rm-skin-name">{{ skin.name }}</span>
          <input
            v-else-if="!skin.account"
            v-model="skinName"
            maxlength="80"
            :aria-label="t('皮肤名称')"
            :disabled="mutating || !!applyingSkin"
            @change="editSkin()"
            @keydown.enter="($event.target as HTMLInputElement).blur()"
          />
          <GlassSelect
            v-if="!skin.account && !skin.builtin"
            id="skin-model"
            :model-value="skin.slim ? 'slim' : 'classic'"
            :disabled="busy || mutating || !!applyingSkin"
            :options="[
              { value: 'classic', label: '经典模型' },
              { value: 'slim', label: '纤细模型' },
            ]"
            floating
            compact
            @update:model-value="editSkin($event === 'slim')"
          />
          <div class="rm-actions">
            <button
              v-if="!skin.account || skin.textureKey"
              class="text-action"
              :disabled="mutating"
              @click="exportSkin"
            >
              <Download :size="15" />{{ t("导出") }}
            </button>
            <button
              v-if="!skin.account && !skin.builtin"
              class="icon-button"
              :disabled="busy || mutating || !!applyingSkin"
              :title="t('移入回收站')"
              :aria-label="t('移入回收站')"
              @click="deleteSkin"
            >
              <Trash2 :size="16" />
            </button>
          </div>
        </footer>
      </div>
      <div v-else class="rm-page">
        <div class="rm-scroll">
          <article
            v-for="(entry, index) in cache"
            :key="entry.kind"
            class="rm-cache-entry list-reveal-item"
            :style="listRevealStyle(index)"
            :data-kind="entry.kind"
          >
            <div class="rm-row">
              <HardDrive :size="22" class="rm-cache-icon" />
              <div class="rm-description rm-title">
                <strong>{{ t(cacheLabels[entry.kind]) }}</strong>
                <span>{{ entry.files }} {{ t("个文件") }}</span>
              </div>
              <span class="rm-size">{{ formatBytes(entry.size) }}</span>
              <button
                v-if="entry.kind === 'partials'"
                class="icon-button"
                :disabled="busy || mutating || !entry.files"
                :title="t('清理')"
                :aria-label="t('清理') + ' ' + t(cacheLabels[entry.kind])"
                @click="clearCache(entry)"
              >
                <Trash2 :size="16" />
              </button>
            </div>
            <div v-if="entry.kind === 'resources'" class="rm-usage">
              <div
                class="rm-usage-bar"
                role="img"
                :aria-label="cacheUsageLabel(entry) || t('资源缓存')"
              >
                <span
                  v-for="part in (entry.parts || []).filter((p) => p.size > 0)"
                  :key="part.kind"
                  class="rm-usage-segment"
                  :style="{
                    width: `${(part.size / entry.size) * 100}%`,
                    '--resource-color': cachePartColors[part.kind],
                  }"
                  :title="`${t(cachePartLabels[part.kind])} · ${formatBytes(part.size)}`"
                />
              </div>
              <div class="rm-usage-legend">
                <span
                  v-for="part in entry.parts || []"
                  :key="part.kind"
                  class="rm-usage-key"
                  :style="{ '--resource-color': cachePartColors[part.kind] }"
                >
                  <i aria-hidden="true" />
                  <span>{{ t(cachePartLabels[part.kind]) }}</span>
                  <span class="rm-size">{{ formatBytes(part.size) }}</span>
                </span>
              </div>
            </div>
          </article>
        </div>
      </div>
    </ContentSwitch>
    <Transition name="toast"
      ><div v-if="confirm" class="rm-confirm" role="alert">
        <div>
          <strong>{{ confirm.title }}</strong>
          <p>{{ confirm.detail }}</p>
        </div>
        <button class="text-action" @click="confirm = undefined">
          {{ t("取消") }}</button
        ><button class="text-action" @click="accept">{{ t("确认") }}</button>
      </div></Transition
    >
    <div
      v-if="loading && !loaded && tab !== 'resources'"
      class="rm-loading"
      role="status"
      :aria-label="t('加载中')"
    >
      <LoaderCircle class="spinning" :size="15" />
    </div>
  </section>
</template>

<style scoped>
.resource-manager {
  width: min(940px, calc(100vw - 48px));
  height: min(620px, calc(100dvh - 64px));
  display: flex;
  flex-direction: column;
  position: relative;
  overflow: hidden;
  padding: 24px 28px 16px;
  gap: 0;
}
.resource-manager .modal-header {
  margin: 0 0 12px;
  flex: none;
  padding: 0 0 12px;
  min-height: 58px;
}
.dialog-heading {
  display: flex;
  align-items: center;
  gap: 10px;
}
.rm-actions {
  display: flex;
  align-items: center;
  gap: 14px;
}
.rm-actions :deep(.glass-select) {
  min-width: 86px;
}
.rm-tabs {
  display: flex;
  justify-content: space-evenly;
  gap: 16px;
  margin: 0;
  min-width: 0;
  flex: 1;
}
.rm-tabs button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  padding: 10px 14px;
  color: var(--muted);
  background: none;
  border: 0;
  font: inherit;
  font-size: 14px;
  cursor: pointer;
}
.rm-tabs button.active {
  color: var(--tab-color);
  text-shadow: 0 0 14px color-mix(in srgb, var(--tab-color) 55%, transparent);
}
.rm-tabs button.active svg {
  filter: drop-shadow(0 0 5px var(--tab-color));
}
.rm-page {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
}
.rm-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  min-height: 45px;
  flex: none;
  margin-bottom: 8px;
}
.rm-scroll {
  overflow: auto;
  overflow-x: hidden;
  min-height: 0;
  flex: 1;
  scrollbar-width: thin;
  scrollbar-gutter: stable;
}
.rm-row {
  display: flex;
  gap: 16px;
  align-items: center;
  padding: 17px 4px;
}
.rm-description {
  min-width: 0;
  flex: 1;
}
.rm-title {
  display: flex;
  align-items: baseline;
  gap: 10px;
  flex-wrap: wrap;
}
.rm-title strong {
  font-size: 14px;
  font-weight: 500;
}
.rm-title span,
.rm-title small {
  font-size: 11px;
  color: var(--muted);
}
.rm-title small {
  margin-left: auto;
}
.rm-path {
  display: flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  padding: 4px 0;
  background: none;
  border: 0;
  color: var(--muted);
  font: inherit;
  font-size: 11px;
  cursor: pointer;
  text-align: left;
}
.rm-path span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.rm-path svg {
  flex: none;
}
.rm-path:hover {
  color: var(--ink);
}
.rm-used {
  font-size: 10px;
  opacity: 0.65;
  padding-top: 3px;
}
.rm-muted {
  color: var(--muted);
  font-size: 12px;
}
.rm-description p {
  font-size: 11px;
  margin: 5px 0 0;
  line-height: 1.5;
}
.rm-java-icon {
  color: #e7bc76;
  flex: none;
}
.rm-cache-icon {
  color: #7dbfee;
  flex: none;
}
.rm-usage {
  padding: 0 4px 15px 42px;
}
.rm-usage-bar {
  display: flex;
  height: 10px;
  overflow: hidden;
  background: color-mix(in srgb, var(--ink) 8%, transparent);
  border-radius: 2px;
}
.rm-usage-segment {
  display: block;
  flex: none;
  height: 100%;
  background: var(--resource-color);
  box-shadow: inset 0 1px 0 #ffffff30;
  transition: width 350ms cubic-bezier(0.16, 1, 0.3, 1);
}
.rm-usage-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 10px 24px;
  margin-top: 10px;
}
.rm-usage-key {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  color: var(--muted);
  font-size: 11px;
}
.rm-usage-key i {
  width: 6px;
  height: 6px;
  background: var(--resource-color);
  box-shadow: 0 0 7px color-mix(in srgb, var(--resource-color) 40%, transparent);
}
.rm-usage-key .rm-size {
  color: var(--ink);
  font-size: inherit;
}
@media (prefers-reduced-motion: reduce) {
  .rm-usage-segment {
    transition: none;
  }
}
.rm-size {
  font-size: 12px;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}
.rm-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 32px;
  padding-top: 10px;
  font-size: 10px;
  color: var(--muted);
  flex: none;
}
.rm-footer > span {
  display: flex;
  align-items: center;
  gap: 8px;
}
.text-action {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 6px 0;
  border: 0;
  background: none;
  color: var(--ink);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
  white-space: nowrap;
}
.text-action:hover,
.rm-kind:hover {
  color: var(--blue);
  text-shadow: 0 0 12px var(--blue);
}
button:disabled {
  opacity: 0.35;
  cursor: default;
}
.rm-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 180px;
  color: var(--muted);
  font-size: 12px;
  text-align: center;
}
.rm-kind {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 0;
  background: none;
  border: 0;
  color: var(--muted);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.rm-kind.active {
  color: var(--blue);
}
.rm-search {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.rm-resource-tools {
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
  margin-left: auto;
  color: var(--muted);
}
.rm-resource-count,
.rm-search svg {
  flex-shrink: 0;
}
.rm-resource-count {
  font-variant-numeric: tabular-nums;
}
.rm-search input {
  font-size: 11px;
  background: none;
  border: 0;
  outline: none;
  width: 145px;
  min-width: 0;
  padding: 5px;
  color: var(--ink);
}
.rm-tile-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, 100px);
  grid-auto-rows: 100px;
  align-content: start;
  gap: 20px;
  padding: 16px 4px;
}
.rm-tile {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 7px;
  width: 100px;
  height: 100px;
  box-sizing: border-box;
  padding: 10px 7px;
  border: 1px solid color-mix(in srgb, var(--ink) 12%, transparent);
  background: color-mix(in srgb, var(--surface) 42%, transparent);
  color: var(--muted);
  font: inherit;
  cursor: pointer;
  --tile-color: #69dcb5;
  transition:
    background-color 180ms ease,
    box-shadow 180ms ease,
    color 180ms ease;
}
.rm-java-tile,
.rm-java-add {
  --tile-color: #e7bc76;
}
.rm-tile:hover:not(:disabled),
.rm-tile.active,
.rm-tile:focus-visible {
  background: color-mix(in srgb, var(--tile-color) 8%, transparent);
  box-shadow: 0 0 18px color-mix(in srgb, var(--tile-color) 12%, transparent);
  color: var(--tile-color);
}
.rm-tile.active {
  border-color: color-mix(in srgb, var(--tile-color) 42%, transparent);
}
.rm-tile-icon {
  flex: none;
  color: var(--tile-color);
}
.rm-tile-name {
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
  max-width: 100%;
  color: var(--ink);
  font-size: 11px;
  line-height: 1.25;
  overflow-wrap: anywhere;
}
.rm-tile-caption {
  font-size: 9px;
  line-height: 1;
}
.rm-tile.is-disabled .rm-tile-icon {
  opacity: 0.4;
}
.rm-tile-status {
  position: absolute;
  top: 8px;
  right: 8px;
  width: 6px;
  height: 6px;
  border: 1px solid currentColor;
}
.rm-add-tile {
  padding: 0;
  border-style: dashed;
}
.rm-add-tile:hover svg {
  filter: drop-shadow(0 0 6px var(--tile-color));
}
.rm-grid-empty {
  grid-column: 1 / -1;
  min-height: 100px;
}
.rm-java-details {
  display: flex;
  flex: none;
  min-height: 74px;
  padding-top: 12px;
}
.rm-java-details .content-switch {
  min-height: 74px;
}
.rm-java-detail,
.rm-java-add-actions {
  display: flex;
  align-items: center;
  gap: 16px;
  min-height: 62px;
  padding: 4px;
}
.rm-java-add-actions {
  justify-content: space-between;
}
.rm-resource-details {
  display: flex;
  flex: 0 1 230px;
  min-height: 0;
  padding-top: 8px;
}
.rm-resource-detail-scroll {
  position: relative;
  padding: 0 32px 8px 0;
}
.rm-detail-close {
  position: absolute;
  right: 0;
  top: 7px;
}
.rm-owners {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
  padding-left: 34px;
}
.rm-owners button {
  display: flex;
  align-items: center;
  gap: 4px;
  background: none;
  border: 0;
  padding: 0;
  color: var(--muted);
  font: inherit;
  font-size: 10px;
  cursor: pointer;
}
.rm-owners button:hover {
  color: var(--blue);
}
.rm-resource-details :deep(.content-item) {
  border: 0;
}
.rm-skin-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, 100px);
  grid-auto-rows: 100px;
  align-content: start;
  gap: 20px;
  padding: 16px 4px;
}
.rm-skin-card,
.rm-skin-import {
  position: relative;
  width: 100px;
  height: 100px;
  box-sizing: border-box;
  background: color-mix(in srgb, var(--surface) 42%, transparent);
  border: 1px solid color-mix(in srgb, var(--ink) 12%, transparent);
  color: var(--muted);
  transition:
    background-color 180ms ease,
    box-shadow 180ms ease,
    color 180ms ease;
}
.rm-skin-card:hover,
.rm-skin-card.active,
.rm-skin-import:hover {
  background: color-mix(in srgb, var(--blue) 8%, transparent);
  box-shadow: 0 0 18px color-mix(in srgb, var(--blue) 12%, transparent);
  color: var(--blue);
}
.rm-skin-card.active {
  border-color: color-mix(in srgb, var(--blue) 42%, transparent);
}
.rm-skin-pick {
  position: absolute;
  inset: -1px;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  background: none;
  border: 0;
  color: inherit;
  cursor: pointer;
}
.rm-skin-live {
  position: absolute;
  inset: -1px;
  z-index: 1;
  opacity: 0;
  pointer-events: none;
}
.rm-skin-live.ready {
  opacity: 1;
  pointer-events: auto;
}
.rm-skin-live :deep(.skin-stage) {
  inset: 0;
}
.rm-skin-applying {
  position: absolute;
  top: 7px;
  right: 7px;
  z-index: 2;
  color: var(--blue);
  pointer-events: none;
  filter: drop-shadow(0 0 5px currentColor);
}
.rm-skin-import {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  border-style: dashed;
  cursor: pointer;
}
.rm-skin-import:hover svg {
  filter: drop-shadow(0 0 6px var(--blue));
}
.rm-skin-details {
  display: flex;
  align-items: center;
  gap: 18px;
  flex: none;
  min-height: 42px;
  padding-top: 12px;
}
.rm-skin-details > input {
  font: inherit;
  font-size: 13px;
  color: var(--ink);
  background: transparent;
  border: 0;
  border-bottom: 1px solid color-mix(in srgb, var(--blue) 20%, transparent);
  padding: 8px 0;
  width: 180px;
  min-width: 0;
}
.rm-skin-name {
  color: var(--ink);
  font-size: 13px;
}
.rm-skin-details .rm-actions {
  margin-left: auto;
}
.rm-confirm {
  position: absolute;
  bottom: 12px;
  left: 20px;
  right: 20px;
  display: flex;
  align-items: center;
  gap: 18px;
  padding: 14px 18px;
  background: var(--surface);
  box-shadow: 0 4px 30px #0005;
  border: 1px solid color-mix(in srgb, var(--blue) 25%, transparent);
  z-index: 5;
}
.rm-confirm > div {
  flex: 1;
  min-width: 0;
}
.rm-confirm strong {
  font-size: 13px;
  font-weight: 500;
}
.rm-confirm p {
  font-size: 11px;
  color: var(--muted);
  margin: 5px 0 0;
  overflow-wrap: anywhere;
}
.rm-loading {
  position: absolute;
  bottom: 22px;
  right: 30px;
  pointer-events: none;
  color: var(--muted);
}
.spinning {
  animation: rm-spin 1.4s linear infinite;
}
@keyframes rm-spin {
  to {
    transform: rotate(360deg);
  }
}
@media (prefers-reduced-motion: reduce) {
  .spinning {
    animation: none;
  }
}
@media (max-width: 700px) {
  .resource-manager {
    padding: 18px;
    width: calc(100vw - 28px);
    height: calc(100dvh - 40px);
  }
  .rm-row {
    gap: 10px;
  }
  .rm-title small {
    margin-left: 0;
  }
  .rm-skin-details {
    gap: 14px;
  }
  .rm-skin-details > input {
    width: 120px;
  }
  .rm-tabs {
    gap: 2px;
  }
  .rm-tabs button {
    padding: 10px 4px;
  }
  .rm-toolbar .rm-actions {
    gap: 10px;
  }
  .rm-footer {
    font-size: 9px;
  }
  .rm-kind {
    font-size: 11px;
  }
  .rm-search input {
    width: 110px;
  }
}
</style>
