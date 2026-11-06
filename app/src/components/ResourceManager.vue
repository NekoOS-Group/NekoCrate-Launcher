<script setup lang="ts">
import WindowControls from "./WindowControls.vue";
import { resourceArrival, arrivalFileKey } from "../resourceArrival";
import {
  cacheResourceParts,
  createResourceLibraryRpc,
  type CacheResourceKind,
} from "../resourceLibrary";
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
import AnimatedCount from "./AnimatedCount.vue";
import { prepareDialogLeave, finishDialogLeave } from "../dialogMotion";
import ToolWindow from "./ToolWindow.vue";
import { focusToolWindow } from "../toolWindows";
import ResourceLibraryBrowser from "./ResourceLibraryBrowser.vue";
import type { ResourceKind } from "../resources";
import GlassSelect from "./GlassSelect.vue";
import ContentSwitch from "./ContentSwitch.vue";
import SkinStage from "./SkinStage.vue";
import SkinLibraryItem from "./SkinLibraryItem.vue";
import ResourceSelectionBar from "./ResourceSelectionBar.vue";
import ResourceTransferOverlay from "./ResourceTransferOverlay.vue";
import { useTrashDrag } from "../useTrashDrag";
import {
  useResourceSelection,
  changeSelectedMods,
} from "../useResourceSelection";
import VersionContentItem from "./VersionContentItem.vue";
import LocalContentImage from "./LocalContentImage.vue";
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
  resourceRemovalKey,
  isResourceDragKind,
  type ResourceCopySource,
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
  VersionManifest,
} from "../types";
import type { ModActions } from "../modManagement";

type Rpc = <T>(op: string, data?: Record<string, unknown>) => Promise<T>;
type Runtime = {
  path: string;
  major: number;
  version: string;
  arch: string;
  jdk: boolean;
  vendor: string;
  source: string;
  size: number;
};
type Owner = { id: string; version: string; label: string };
type Group = { directory: string; owners: Owner[]; global: boolean };
type ResourceRow = {
  group: string;
  key: string;
  file: LocalContentFile;
  directory: string;
  owners: Owner[];
  global: boolean;
};
type CachePart = { kind: CacheResourceKind; size: number; files: number };
type CacheEntry = {
  kind: "metadata" | "resources" | "partials";
  size: number;
  files: number;
  parts: CachePart[];
};
const arrivalKeys = ref<string[]>([]);
let arrivalTimer: ReturnType<typeof setTimeout> | undefined;
let scrolledArrival = "";
async function highlightArrivals() {
  const arrival = resourceArrival.value;
  if (!arrival || Date.now() - arrival.time > 2400) return;
  arrivalKeys.value = filtered.value
    .filter(
      (row) =>
        arrival.files.includes(
          arrivalFileKey(resourceKind.value, row.file.file_name),
        ) &&
        row.owners.some(
          (owner) =>
            owner.id === arrival.instance && owner.version === arrival.version,
        ),
    )
    .map((row) => row.key);
  const first = arrivalKeys.value[0];
  if (first && scrolledArrival !== `${arrival.time}:${first}`) {
    scrolledArrival = `${arrival.time}:${first}`;
    await nextTick();
    document
      .querySelector(".rm-resource-tile.is-arriving")
      ?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }
}
watch(resourceArrival, (arrival) => {
  if (!arrival) return;
  resourceKind.value = arrival.kind as typeof resourceKind.value;
  query.value = "";
  void highlightArrivals();
  clearTimeout(arrivalTimer);
  arrivalTimer = setTimeout(() => {
    arrivalKeys.value = [];
  }, 2400);
});
onBeforeUnmount(() => clearTimeout(arrivalTimer));
const props = defineProps<{
  rpc: Rpc;
  manifest: VersionManifest | null;
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
  link: [url: string];
  close: [];
  error: [error: unknown];
  busy: [value: boolean];
  "skin-changed": [];
  "java-changed": [];
  manage: [id: string, version: string, kind: ContentKind];
}>();
const transferResource = inject(resourceTransferKey, undefined);
const removeResources = inject(resourceRemovalKey, undefined);
async function removeSelectedResources() {
  if (!removeResources || !canRemoveSelected.value || selectionLocked.value)
    return;
  try {
    await removeResources(selectedResources.value.map(resourceSource));
  } catch (error) {
    emit("error", error);
  }
}
function beginResourceDrag(event: PointerEvent, row: ResourceRow) {
  if (!isResourceDragKind(resourceKind.value) || locked(row)) return;
  if (event.shiftKey || event.metaKey || event.ctrlKey) return;
  const rows = selection.drag(row);
  if (rows.some(locked)) return;
  transferResource?.(event, rows.map(resourceSource));
}
function resourceSource(row: ResourceRow): ResourceCopySource {
  return {
    global: row.global,
    removable: row.global,
    kind: resourceKind.value as ResourceCopySource["kind"],
    instance: row.owners[0]?.id || "",
    version: row.owners[0]?.version || "",
    file: row.file.file_name,
    name: resourceName(row),
  };
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
      managed: "已缓存",
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
const cacheUsageLabel = (entry: CacheEntry) =>
  (entry.parts || [])
    .map(
      (part) =>
        `${t(cacheResourceParts[part.kind].label)} ${formatBytes(part.size)}`,
    )
    .join(" · ");
function showCachePart(kind: CacheResourceKind) {
  browsing.value = false;
  query.value = "";
  if (cacheResourceParts[kind].tab === "resources") {
    resourceKind.value = kind as ContentKind;
    cachedOnly.value = true;
  }
  tab.value = cacheResourceParts[kind].tab;
}
const confirm = shallowRef<{
  title: string;
  detail: string;
  run: () => Promise<void>;
}>();
const resourceKind = ref<ContentKind>("mods");
const cachedOnly = ref(false);
const browsing = ref(false);
const libraryDownloading = ref(false);
function downloadBusy(value: boolean) {
  libraryDownloading.value = value;
  emit("busy", value);
}
function downloadedResource(kind: ResourceKind) {
  resourceKind.value = kind as ContentKind;
  cachedOnly.value = true;
  void nextTick(refresh);
}
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
const resourceRpc = createResourceLibraryRpc(props.rpc);
let controller: AbortController | undefined;
let generation = 0;
let indexedAllResources = false;
let skinGeneration = 0;
const library = <T = unknown,>(
  kind: string,
  data: Record<string, unknown> = {},
) => props.rpc<T>("resource_manager", { action: { kind, ...data } });
const filtered = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase();
  return resources.value.filter(
    (row) =>
      (!cachedOnly.value || row.global) &&
      (!needle ||
        [
          row.file.name,
          row.file.file_name,
          ...row.owners.map((o) => o.label),
        ].some((s) => s.toLocaleLowerCase().includes(needle))),
  );
});
const selection = useResourceSelection(
  () => filtered.value,
  (row) => row.key,
);
const selectedResources = selection.selected;
const canRemoveSelected = computed(
  () =>
    selectedResources.value.length > 0 &&
    selectedResources.value.every((row) => row.global),
);
const selectedHasEnabled = computed(() =>
  selectedResources.value.some((row) => row.file.enabled),
);
const selectedHasDisabled = computed(() =>
  selectedResources.value.some((row) => !row.file.enabled),
);
const selectionLocked = computed(
  () => props.busy || mutating.value || selectedResources.value.some(locked),
);
watch([tab, resourceKind, query, cachedOnly], () => selection.clear());
function selectResource(event: MouseEvent, row: ResourceRow) {
  if (mutating.value) return;
  if (selection.click(event, row)) return;
  resourceKey.value = resourceKey.value === row.key ? "" : row.key;
}
async function setSelectedEnabled(enabled: boolean) {
  if (
    selectionLocked.value ||
    resourceKind.value !== "mods" ||
    selectedResources.value.some((r) => r.global)
  )
    return;
  const rows = [...selectedResources.value];
  const scope = generation;
  await run(async () => {
    const failures = await changeSelectedMods(
      rows,
      enabled,
      (row) => row.file.enabled,
      (row, state) =>
        props.modActions.setEnabled(
          row.owners[0]!.id,
          row.owners[0]!.version,
          row.file.file_name,
          state,
        ),
      (row, name) => {
        if (scope !== generation) return;
        const entries = resourceGroups.get(row.group) || [];
        resourceGroups.set(
          row.group,
          entries.map((entry) =>
            entry.key === row.key
              ? { ...entry, file: { ...entry.file, file_name: name, enabled } }
              : entry,
          ),
        );
        resources.value = [...resourceGroups.values()].flat();
      },
    );
    if (scope === generation)
      for (const row of new Map(rows.map((row) => [row.group, row])).values())
        await changed(row);
    if (failures.length)
      emit(
        "error",
        failures
          .map(({ item, error }) => `${resourceName(item)}: ${String(error)}`)
          .join("\n"),
      );
  });
}
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
  resourceRpc<ContentMetadata>("version_content_metadata", {
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
  await resourceRpc("open_content_folder", { id, version, kind, file, source });
};
const loadSource = (
  id: string,
  version: string,
  kind: ContentKind,
  file: string,
  source: string,
) =>
  resourceRpc<string>("version_content_source", {
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
      const onlyCached = cachedOnly.value;
      indexedAllResources = false;
      const kind = resourceKind.value;
      const index = await library<{ groups: Group[]; issues: string[] }>(
        "groups",
        { content: kind },
      );
      if (request !== generation) return;
      scanIssues.value = index.issues;
      scanned.value = 0;
      const groups = index.groups.filter(
        (group) => !onlyCached || group.global,
      );
      totalGroups.value = groups.length;
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
              global: group.global,
            })),
        );
        resources.value = [...results.values()].flat();
      };
      resources.value = [];
      // Scan one shared working directory once; metadata batches paint progressively.
      for (const group of groups) {
        if (signal.aborted) return;
        const owner = group.owners[0];
        try {
          publish(
            group,
            group.global
              ? await library<VersionContent>("cached_resources", {
                  content: kind,
                })
              : await loader(owner.id, owner.version, kind, {
                  signal,
                  fresh: true,
                  onProgress: (c) => publish(group, c),
                }),
          );
        } catch (error) {
          if (signal.aborted) return;
          scanIssues.value.push(
            `${owner?.label || t("全局缓存")}: ${String(error)}`,
          );
        }
        if (request === generation) {
          readyGroups.value = new Set([...readyGroups.value, group.directory]);
          scanned.value++;
        }
      }
      if (request === generation) indexedAllResources = !onlyCached;
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
const resourceCount = ref<number>();
watch([filtered, loading], () => {
  if (!loading.value) resourceCount.value = filtered.value.length;
});
watch(cachedOnly, (only) => {
  resourceKey.value = "";
  if (!only && tab.value === "resources" && !indexedAllResources)
    void refresh();
});
watch(filtered, () => void highlightArrivals());
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
const javaRemovalLocked = computed(
  () =>
    props.busy ||
    mutating.value ||
    libraryDownloading.value ||
    props.running.length > 0,
);
const javaDrag = useTrashDrag<Runtime>({
  canRemove: (runtime) =>
    tab.value === "java" &&
    !javaRemovalLocked.value &&
    runtimes.value.some(
      (current) =>
        current.path === runtime.path && current.source === "managed",
    ),
  describe: (runtime) => ({
    kind: "java",
    name: `Java ${javaVersion(runtime)} · ${runtime.vendor}`,
  }),
  remove: (runtime) => run(() => deleteJava(runtime)),
});
async function deleteJava(runtime: Runtime) {
  // Recheck ownership at confirmation/drop time as the inventory may change.
  if (
    runtime.source !== "managed" ||
    props.running.length ||
    !runtimes.value.some(
      (current) =>
        current.path === runtime.path && current.source === "managed",
    )
  )
    return;
  await library("remove_java", { path: runtime.path });
  emit("java-changed");
  await refresh();
}
function removeJava(runtime: Runtime) {
  if (runtime.source !== "managed" || javaRemovalLocked.value) return;
  confirm.value = {
    title: t("移入回收站"),
    detail: runtime.path,
    run: () => deleteJava(runtime),
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
  if (row.global) {
    await refresh();
    return;
  }
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
function showMod(id: string, version: string, file?: string) {
  if (!id) {
    const row = resources.value.find(
      (row) => row.global && row.file.file_name === file,
    );
    if (row) resourceKey.value = row.key;
    return;
  }
  emit("manage", id, version, "mods");
}
async function importResources(paths: string[], kind = resourceKind.value) {
  await run(async () => {
    await library("cache_resources", {
      inputs: paths.map((path) => ({ path, kind })),
    });
    tab.value = "resources";
    resourceKind.value = kind;
    cachedOnly.value = true;
    await refresh();
  });
}
function browseResources() {
  if (props.busy || mutating.value) return;
  browsing.value = true;
  void nextTick(() => focusToolWindow("library-browser"));
}
async function closeBrowser() {
  if (libraryDownloading.value) return;
  browsing.value = false;
  await refresh();
}
async function cacheResources(rows: ResourceRow[]) {
  await run(async () => {
    await library("cache_resources", { sources: rows.map(resourceSource) });
    cachedOnly.value = true;
    await refresh();
  });
}
async function drop(paths: string[], destination = tab.value) {
  if (
    destination === "resources" ||
    kinds.some((k) => k.value === destination)
  ) {
    await importResources(
      paths,
      destination === "resources"
        ? resourceKind.value
        : (destination as ContentKind),
    );
  } else if (destination === "skins") {
    const images = paths.filter((p) => /\.png$/i.test(p));
    if (!images.length) emit("error", t("请选择 PNG 皮肤文件"));
    else await importPaths(images);
  } else if (destination === "java" && paths.length === 1)
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
async function resourceUpdated(id: string, version: string) {
  const seen = new Set<string>();
  for (const row of resources.value) {
    if (
      !seen.has(row.group) &&
      row.owners.some((owner) => owner.id === id && owner.version === version)
    ) {
      seen.add(row.group);
      await changed(row);
    }
  }
}
defineExpose({ drop, refresh, resourceUpdated });
</script>

<template>
  <section
    class="modal glass-surface resource-manager"
    :data-drop-kind="tab === 'resources' ? resourceKind : tab"
    :data-resource-library="tab === 'resources' ? '' : undefined"
  >
    <header class="modal-header">
      <nav
        class="rm-tabs dialog-heading"
        tabindex="-1"
        :aria-label="t('资源管理')"
      >
        <button
          v-for="item in tabs"
          :key="item.id"
          :data-drop-kind="item.id === 'resources' ? resourceKind : item.id"
          :data-resource-library="item.id === 'resources' ? '' : undefined"
          :style="{ '--tab-color': item.color }"
          :class="{ active: tab === item.id }"
          :disabled="mutating || libraryDownloading"
          :aria-pressed="tab === item.id"
          @click="tab = item.id"
        >
          <component :is="item.icon" :size="18" /><span>{{
            t(item.label)
          }}</span>
        </button>
      </nav>
      <WindowControls />
      <button
        class="icon-button"
        :title="t('关闭')"
        :aria-label="t('关闭')"
        :disabled="mutating || libraryDownloading"
        @click="emit('close')"
      >
        <X :size="20" />
      </button>
    </header>
    <ContentSwitch :content-key="tab">
      <div v-if="tab === 'java'" class="rm-page">
        <div class="rm-tile-grid rm-scroll" data-list-viewport>
          <button
            v-for="(runtime, index) in runtimes"
            :key="runtime.path"
            class="rm-tile rm-java-tile list-reveal-item"
            :style="listRevealStyle(index)"
            :class="{
              active: !addingJava && javaPath === runtime.path,
              'resource-drag-handle':
                runtime.source === 'managed' && !javaRemovalLocked,
            }"
            :aria-pressed="!addingJava && javaPath === runtime.path"
            :aria-label="`Java ${runtime.major} · ${runtime.vendor} · ${javaVersion(runtime)} · ${runtime.arch} · ${javaSource(runtime)}`"
            :title="`${runtime.vendor} · ${javaVersion(runtime)} · ${runtime.jdk ? 'JDK' : 'JRE'} · ${runtime.arch} · ${javaSource(runtime)}\n${runtime.path}`"
            :disabled="mutating || libraryDownloading"
            @pointerdown="javaDrag.start($event, runtime)"
            @dragstart.prevent
            @click="
              javaPath = runtime.path;
              addingJava = false;
            "
          >
            <Coffee :size="34" :stroke-width="1.3" class="rm-tile-icon" />
            <span
              v-if="runtime.source === 'managed'"
              class="rm-java-cached"
              :title="t('已缓存')"
              :aria-label="t('已缓存')"
              ><HardDrive :size="12"
            /></span>
            <span class="rm-tile-name">Java {{ runtime.major }}</span>
            <span class="rm-tile-caption">{{
              runtime.vendor ||
              `${runtime.jdk ? "JDK" : "JRE"} · ${runtime.arch}`
            }}</span>
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
                    ><template v-if="selectedJava.vendor"
                      >{{ selectedJava.vendor }} · </template
                    >{{ selectedJava.arch }} ·
                    {{ selectedJava.jdk ? "JDK" : "JRE" }}</span
                  >
                  <small>{{ javaSource(selectedJava) }}</small>
                </div>
                <button class="rm-path" @click="openJava(selectedJava.path)">
                  <FolderOpen :size="13" /><span>{{ selectedJava.path }}</span>
                </button>
              </div>
              <span v-if="selectedJava.size" class="rm-muted rm-size">{{
                formatBytes(selectedJava.size)
              }}</span>
              <button
                v-if="selectedJava.source === 'managed'"
                class="icon-button"
                :disabled="javaRemovalLocked"
                :title="t('移入回收站')"
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
      <div
        v-else-if="tab === 'resources'"
        class="rm-page"
        @keydown="!mutating && selection.keyboard($event)"
      >
        <div
          class="rm-toolbar list-reveal-item"
          :style="listRevealStyle(0)"
          data-list-exit-y="-10"
          data-list-footer
        >
          <div class="rm-actions">
            <button
              v-for="kind in kinds"
              :key="kind.value"
              class="rm-kind"
              :data-drop-kind="kind.value"
              :class="{ active: resourceKind === kind.value }"
              @click="resourceKind = kind.value as ContentKind"
            >
              <component :is="kind.icon" :size="15" />{{ t(kind.label) }}
            </button>
          </div>
          <div class="rm-resource-tools">
            <button
              class="icon-button rm-cache-filter"
              :class="{ active: cachedOnly }"
              :aria-pressed="cachedOnly"
              :title="t('仅显示全局缓存')"
              :aria-label="t('仅显示全局缓存')"
              @click="cachedOnly = !cachedOnly"
            >
              <HardDrive :size="15" />
            </button>
            <label class="rm-search">
              <Search :size="14" /><input
                v-model="query"
                :placeholder="t('搜索')"
                :aria-label="t('搜索')"
              />
            </label>
            <span class="rm-muted rm-resource-count"
              ><AnimatedCount :value="resourceCount" bare
            /></span>
          </div>
        </div>
        <ResourceSelectionBar
          class="rm-selection"
          :count="selectedResources.length"
          :mods="
            resourceKind === 'mods' &&
            !selectedResources.some((row) => row.global)
          "
          :busy="selectionLocked"
          :enable="selectedHasDisabled"
          :disable="selectedHasEnabled"
          :removable="!!removeResources && canRemoveSelected"
          @remove="removeSelectedResources"
          @clear="selection.clear"
          @enabled="setSelectedEnabled"
        >
          <button
            v-if="selectedResources.some((row) => !row.global)"
            class="icon-button"
            :disabled="selectionLocked"
            :title="t('缓存到启动器')"
            :aria-label="t('缓存到启动器')"
            @click="
              cacheResources(selectedResources.filter((row) => !row.global))
            "
          >
            <HardDrive :size="15" />
          </button>
        </ResourceSelectionBar>
        <ContentSwitch
          class="rm-resource-list"
          :content-key="`${resourceKind}:${cachedOnly}`"
        >
          <div
            class="rm-tile-grid rm-scroll rm-resources"
            data-list-viewport
            :data-list-stagger="resourceRowStagger"
          >
            <button
              class="rm-tile rm-add-tile list-reveal-item"
              :style="listRevealStyle(0, 20, resourceRowStagger)"
              :disabled="busy || mutating"
              :aria-label="t('添加资源')"
              :title="t('添加资源')"
              @click="browseResources"
            >
              <Plus :size="24" :stroke-width="1.4" />
            </button>
            <button
              v-for="(row, index) in filtered"
              :key="row.key"
              class="rm-tile rm-resource-tile list-reveal-item"
              :style="listRevealStyle(index + 1, 20, resourceRowStagger)"
              :class="{
                active: resourceKey === row.key,
                'is-arriving': arrivalKeys.includes(row.key),
                'is-selected': selection.contains(row),
                'resource-drag-handle':
                  isResourceDragKind(resourceKind) &&
                  !!transferResource &&
                  !locked(row),
                'is-disabled': resourceKind === 'mods' && !row.file.enabled,
              }"
              :aria-pressed="selection.contains(row) || resourceKey === row.key"
              :aria-label="resourceName(row)"
              :title="`${resourceName(row)}${row.file.version ? ' · ' + row.file.version : ''}\n${row.global ? t('全局缓存') : row.owners.map((owner) => owner.label).join(' · ')}${resourceKind === 'mods' && !row.file.enabled ? '\n' + t('已禁用') : ''}`"
              @click="selectResource($event, row)"
              @pointerdown="beginResourceDrag($event, row)"
              @dragstart.prevent
            >
              <LocalContentImage
                v-if="
                  ['mods', 'resourcepacks', 'shaderpacks'].includes(
                    resourceKind,
                  )
                "
                :rpc="resourceRpc"
                :instance-id="row.owners[0]?.id || ''"
                :version="row.owners[0]?.version || ''"
                :kind="resourceKind"
                :file="row.file"
                :size="40"
                class="rm-tile-icon"
                ><component :is="resourceIcon" :size="32" :stroke-width="1.3"
              /></LocalContentImage>
              <component
                v-else
                :is="resourceIcon"
                :size="32"
                :stroke-width="1.3"
                class="rm-tile-icon"
              />
              <span class="rm-tile-name">{{ resourceName(row) }}</span>
              <span
                v-if="row.global"
                class="rm-resource-cached"
                :aria-label="t('全局缓存')"
                ><HardDrive :size="12"
              /></span>
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
        </ContentSwitch>
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
                :rpc="resourceRpc"
                @link="emit('link', $event)"
                ref="resourceDetail"
                :file="selectedResource.file"
                :instance-id="selectedResource.owners[0]?.id || ''"
                :version="selectedResource.owners[0]?.version || ''"
                :global="selectedResource.global"
                :can-remove="selectedResource.global"
                :kind="resourceKind"
                :icon="resourceIcon"
                :locked="locked(selectedResource)"
                :mod-actions="selectedResource.global ? undefined : modActions"
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
                  v-if="!selectedResource.global"
                  :disabled="locked(selectedResource)"
                  @click="cacheResources([selectedResource])"
                >
                  <HardDrive :size="13" />{{ t("缓存到启动器") }}
                </button>
                <template v-else>
                  <span class="rm-muted"
                    ><HardDrive :size="13" /> {{ t("全局缓存") }}</span
                  >
                </template>
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
              :disabled="mutating || libraryDownloading"
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
                role="group"
                :aria-label="cacheUsageLabel(entry) || t('资源缓存')"
              >
                <button
                  v-for="part in (entry.parts || []).filter((p) => p.size > 0)"
                  :key="part.kind"
                  class="rm-usage-segment"
                  :style="{
                    width: `${(part.size / entry.size) * 100}%`,
                    '--resource-color': cacheResourceParts[part.kind].color,
                  }"
                  :title="`${t(cacheResourceParts[part.kind].label)} · ${formatBytes(part.size)}`"
                  :aria-label="`${t(cacheResourceParts[part.kind].label)} · ${formatBytes(part.size)}`"
                  @click="showCachePart(part.kind)"
                />
              </div>
              <div class="rm-usage-legend">
                <button
                  v-for="part in entry.parts || []"
                  :key="part.kind"
                  class="rm-usage-key"
                  :style="{
                    '--resource-color': cacheResourceParts[part.kind].color,
                  }"
                  @click="showCachePart(part.kind)"
                >
                  <i aria-hidden="true" />
                  <span>{{ t(cacheResourceParts[part.kind].label) }}</span>
                  <span class="rm-size">{{ formatBytes(part.size) }}</span>
                </button>
              </div>
            </div>
          </article>
        </div>
      </div>
    </ContentSwitch>
    <!-- Keep the nested dialog in the theme scope while outside the parent panel. -->
    <Teleport to=".shell">
      <Transition
        name="dialog"
        appear
        @before-leave="prepareDialogLeave"
        @after-leave="finishDialogLeave"
      >
        <ToolWindow
          v-if="browsing"
          window-id="library-browser"
          :initial-size="{ width: 820, height: 580 }"
          :label="t('添加资源')"
          :locked="libraryDownloading"
          @close="closeBrowser"
        >
          <ResourceLibraryBrowser
            :rpc="rpc"
            :manifest="manifest"
            :kind="resourceKind as ResourceKind"
            :busy="busy && !libraryDownloading"
            @back="closeBrowser"
            @busy="downloadBusy"
            @downloaded="downloadedResource"
            @error="emit('error', $event)"
            @link="emit('link', $event)"
          />
        </ToolWindow>
      </Transition>
    </Teleport>
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
    <ResourceTransferOverlay :drag="javaDrag.drag.value" trash-only />
  </section>
</template>

<style scoped>
.rm-selection {
  padding-bottom: 8px;
}
.rm-resource-tile.is-selected {
  color: var(--blue);
  background: color-mix(in srgb, var(--blue) 9%, transparent);
}
.rm-resource-tile.is-selected .rm-tile-icon {
  filter: drop-shadow(0 0 8px color-mix(in srgb, var(--blue) 65%, transparent));
}
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
.rm-toolbar.list-reveal-item {
  animation-name: resource-toolbar-enter, list-row-fade;
}
:global(.dialog-leave-active)
  :is(.rm-toolbar, .rm-tile, .rm-skin-card, .rm-skin-import) {
  animation: none;
  transition:
    opacity var(--motion-fade) var(--motion-ease),
    transform var(--motion-panel) var(--motion-ease);
}
:global(.dialog-leave-to)
  :is(.rm-toolbar, .rm-tile, .rm-skin-card, .rm-skin-import) {
  opacity: 0;
  /* Offset the dialog's 8px descent, matching the list's upward exit. */
  transform: translateY(-16px);
}
@keyframes resource-toolbar-enter {
  from {
    transform: translateY(-12px);
  }
  to {
    transform: none;
  }
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
  border: 0;
  padding: 0;
  cursor: pointer;
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
  border: 0;
  padding: 3px 0;
  background: none;
  font: inherit;
  cursor: pointer;
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
.rm-usage-key:hover,
.rm-usage-key:focus-visible {
  color: var(--resource-color);
}
.rm-usage-segment:hover,
.rm-usage-segment:focus-visible {
  filter: brightness(1.3);
}
.rm-usage-segment:focus-visible {
  outline: 1px solid var(--ink);
  outline-offset: -1px;
}
.rm-resource-cached {
  position: absolute;
  top: 7px;
  right: 7px;
  display: flex;
  color: var(--tile-color);
}
.rm-cache-filter,
.rm-cache-filter.active,
.rm-cache-filter:hover:not(:disabled) {
  border-color: transparent;
  background: transparent;
  box-shadow: none;
  filter: none;
}
.rm-cache-filter.active,
.rm-cache-filter:focus-visible {
  color: var(--blue);
  outline: none;
}
.rm-cache-filter > svg {
  transition: filter 180ms;
}
.rm-cache-filter.active > svg,
.rm-cache-filter:focus-visible > svg {
  filter: drop-shadow(0 0 5px var(--blue));
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
  display: flex;
  justify-content: flex-end;
  width: 5ch;
  font-variant-numeric: tabular-nums;
}
.rm-resource-count :deep(.animated-count) {
  top: 0;
}
.rm-resources {
  position: relative;
}
.rm-search input {
  font-size: 11px;
  background: none;
  border: 0;
  box-shadow: none;
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
.rm-java-cached {
  position: absolute;
  top: 7px;
  right: 7px;
  display: flex;
  color: var(--tile-color);
}
.rm-java-tile .rm-tile-caption {
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
  position: absolute;
  inset: 0;
  margin: 0;
  min-height: 0;
  pointer-events: none;
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

<style scoped>
.rm-resource-tile.is-arriving {
  filter: drop-shadow(0 0 8px var(--accent));
  outline: 1px dashed var(--accent);
  outline-offset: -3px;
}
</style>
