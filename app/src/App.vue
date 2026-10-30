<script setup lang="ts">
import { t, locale, setLanguage, translateMessage } from "./i18n";
import {
  computed,
  defineAsyncComponent,
  nextTick,
  onMounted,
  onUnmounted,
  reactive,
  ref,
  watch,
  watchEffect,
} from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Plus,
  Boxes,
  Settings2,
  UsersRound,
  UserRound,
  FolderOpen,
  Play,
  Download,
  Square,
  RefreshCw,
  ChevronDown,
  ChevronUp,
  Terminal,
  X,
  Copy,
  Trash2,
  ExternalLink,
  LoaderCircle,
  CheckCircle2,
  Check,
} from "lucide-vue-next";
import { backdropThemes } from "./backdropThemes";
import { resolveInstanceIcon } from "./instanceIcons";
import {
  moveInstanceBefore,
  VISIBLE_INSTANCES,
  instanceVisibleLimit,
} from "./instanceOrder";
import { useInstanceSorting } from "./useInstanceSorting";
import {
  prepareDialogLeave,
  finishDialogLeave,
  pinPanelLeave,
  resetPanelLeave,
} from "./dialogMotion";
import { version as launcherVersion } from "../package.json";
import { prepareAndLaunch } from "./launch";
import {
  createLoaderCatalog,
  type LoaderVersion as LoaderChoice,
} from "./loaderCatalog";
import { signInWithMicrosoft } from "./login";
import { createAccountSkinLoader } from "./accountSkins";
import { builtinSkins } from "./builtinSkins";
import type { SavedSkin } from "./skinLibrary";
import type { LogLine } from "./logPresentation";
import { createLogStore } from "./logStore";
import LogPanel from "./components/LogPanel.vue";
import GlassDialog from "./components/GlassDialog.vue";
import DownloadWorkflow from "./components/DownloadWorkflow.vue";
import ErrorNotice from "./components/ErrorNotice.vue";
import FolderDrop from "./components/FolderDrop.vue";
import { useFolderDrop } from "./useFolderDrop";
import {
  createWorkflow,
  applyWorkflowEvent,
  resumeWorkflow,
  finishWorkflow,
  waitForManualFiles,
  setWorkflowPaused,
  clearWorkflowFiles,
} from "./downloadWorkflow";
import SettingsAbout from "./components/SettingsAbout.vue";
import { vPanelSize } from "./panelSize";
import {
  validProxyUrl,
  validProxySettings,
  proxyKind,
  proxyAddressForKind,
  type ProxyKind,
} from "./networkSettings";
import SkinStage from "./components/SkinStage.vue";
import WorldBackdrop from "./components/WorldBackdrop.vue";
import VersionMenu from "./components/VersionMenu.vue";
import VersionDownload from "./components/VersionDownload.vue";
import ModpackImport from "./components/ModpackImport.vue";
import ModpackMissing from "./components/ModpackMissing.vue";
import type { ModpackPreview, ModpackStatus } from "./types";
import VersionManager from "./components/VersionManager.vue";
import ResourceTransferOverlay from "./components/ResourceTransferOverlay.vue";
import DropTargetOverlay from "./components/DropTargetOverlay.vue";
import { dropInteraction, showDropHint, clearDropHint } from "./dropSurface";
import {
  useResourceTransfer,
  type ResourceCopySource,
  type ResourceDestination,
} from "./useResourceTransfer";
import ResourceBrowser from "./components/ResourceBrowser.vue";
const ResourceManager = defineAsyncComponent(
  () => import("./components/ResourceManager.vue"),
);
import ContentSwitch from "./components/ContentSwitch.vue";
import { listRevealStyle } from "./listMotion";
import ModpackBrowser from "./components/ModpackBrowser.vue";
import {
  resourceDropTarget,
  routeResourceDrop,
  resourceDropIssue,
  resourceKindLabel,
  type ResourceSession,
  type ResourcePlan,
  type DetectedResource,
  type ResourceKind,
  type ResourcePlatform,
} from "./resources";
import { versionName } from "./versionDescription";
import { createVersionContentLoader } from "./versionContentLoader";
import AccountAvatar from "./components/AccountAvatar.vue";
import InstanceHeader from "./components/InstanceHeader.vue";
import DownloadSourceSelect from "./components/DownloadSourceSelect.vue";
import GlassSelect from "./components/GlassSelect.vue";
import ThemePicker from "./components/ThemePicker.vue";
import InstanceIcon from "./components/InstanceIcon.vue";
import InstancePicker from "./components/InstancePicker.vue";
import InstanceLocation from "./components/InstanceLocation.vue";
import InstanceProgress from "./components/InstanceProgress.vue";
import type { FolderImportResult } from "./useFolderDrop";
import type {
  Settings,
  Instance,
  SuggestedInstance,
  Snapshot,
  VersionManifest,
  LauncherEvent,
  Challenge,
  Task,
  Account,
  UpdateStatus,
  LauncherUpdateProgress,
  JavaInfo,
  MemoryMode,
  MemoryOptions,
  ContentLoadOptions,
  VersionContentCounts,
  VersionMetadata,
  ContentKind,
  ContentMetadata,
} from "./types";

const desktop = isTauri();
const utility = ref<"accounts" | "downloads" | "settings" | null>(null);
const snapshot = ref<Snapshot>({
  root: "",
  system_locale: navigator.language,
  database: {
    settings: {
      microsoft_client_id: "",
      download_source: "auto",
      mod_download_source: "auto",
      proxy_mode: "system",
      proxy_url: "",
      proxy_username: "",
      proxy_password_key: null,
      concurrency: 8,
      adaptive_concurrency: false,
      retries: 2,
      auto_java: true,
      language: "system",
      apply_language_to_game: false,
    },
    instances: [],
    accounts: [],
    selected_account: null,
    guest_name: "Player",
    guest_skin: "builtin:steve",
  },
  running: [],
});
const settings = reactive<Settings>({ ...snapshot.value.database.settings });
const settingsSaving = ref(false);
let modalSession = 0;
const settingsTabs = [
  { id: "general", label: "通用设置" },
  { id: "network", label: "网络设置" },
  { id: "about", label: "关于" },
] as const;
const settingsTab = ref<(typeof settingsTabs)[number]["id"]>("general");
const settingsSaveError = ref("");
const editableSettings = [
  "language",
  "apply_language_to_game",
  "download_source",
  "mod_download_source",
  "proxy_mode",
  "proxy_url",
  "proxy_username",
  "proxy_password",
  "concurrency",
  "adaptive_concurrency",
  "retries",
] as const;
type EditableSetting = (typeof editableSettings)[number];
const pendingSettings = new Map<EditableSetting, number>();
let settingsRevision = 0;
let hydratingSettings = false;
let settingsWrite: Promise<void> | null = null;
const proxyFields = [
  "proxy_mode",
  "proxy_url",
  "proxy_username",
  "proxy_password",
] as const;
const proxyPasswordLoading = ref(false);
const proxyPasswordError = ref("");
let proxyPasswordRequest = 0;
let proxyPasswordEdit = 0;
let proxyPasswordDirty = false;
const proxyChoice = ref<ProxyKind>("system");
const proxyEnabled = computed({
  get: () => settings.proxy_mode !== "none",
  set: (enabled: boolean) => {
    settings.proxy_mode = enabled ? proxyChoice.value : "none";
  },
});
function selectProxyKind(value: string) {
  const kind = value as ProxyKind;
  proxyChoice.value = kind;
  const address = proxyAddressForKind(settings.proxy_url, kind);
  settings.proxy_mode = kind;
  settings.proxy_url = address;
}
function hydrateSettings() {
  hydratingSettings = true;
  settings.proxy_password = undefined;
  Object.assign(settings, snapshot.value.database.settings);
  if (settings.mod_download_source === "official")
    settings.mod_download_source = "auto";
  if (settings.proxy_mode !== "none")
    proxyChoice.value = proxyKind(settings.proxy_mode, settings.proxy_url);
  hydratingSettings = false;
}
async function loadProxyPassword() {
  const request = ++proxyPasswordRequest;
  const edit = proxyPasswordEdit;
  proxyPasswordError.value = "";
  proxyPasswordLoading.value = false;
  if (!settings.proxy_password_key || pendingSettings.has("proxy_password"))
    return;
  proxyPasswordLoading.value = true;
  try {
    const password = await rpc<string>("proxy_password");
    if (
      request === proxyPasswordRequest &&
      edit === proxyPasswordEdit &&
      utility.value === "settings"
    ) {
      hydratingSettings = true;
      settings.proxy_password = password;
      hydratingSettings = false;
    }
  } catch (e) {
    if (request === proxyPasswordRequest) proxyPasswordError.value = message(e);
  } finally {
    if (request === proxyPasswordRequest) proxyPasswordLoading.value = false;
  }
}
function validSetting(key: EditableSetting) {
  if (key === "proxy_mode" || key === "proxy_url")
    return validProxySettings(settings.proxy_mode, settings.proxy_url);
  if (key === "concurrency")
    return (
      Number.isInteger(settings.concurrency) &&
      settings.concurrency >= 1 &&
      settings.concurrency <= 256
    );
  if (key === "retries")
    return (
      Number.isInteger(settings.retries) &&
      settings.retries >= 0 &&
      settings.retries <= 5
    );
  return true;
}
watch(
  () => editableSettings.map((key) => settings[key]),
  (values, previous) => {
    if (hydratingSettings) return;
    const passwordIndex = editableSettings.indexOf("proxy_password");
    if (values[passwordIndex] !== previous[passwordIndex]) {
      proxyPasswordEdit++;
      proxyPasswordDirty = true;
      proxyPasswordError.value = "";
    }
    editableSettings.forEach((key, index) => {
      if ((proxyFields as readonly string[]).includes(key)) return;
      if (values[index] === previous[index] || !validSetting(key)) return;
      Object.assign(snapshot.value.database.settings, { [key]: settings[key] });
      pendingSettings.set(key, ++settingsRevision);
    });
    const proxyChanged = proxyFields.some((key) => {
      const index = editableSettings.indexOf(key as EditableSetting);
      return values[index] !== previous[index];
    });
    if (proxyChanged && validSetting("proxy_mode")) {
      // Mode + address become valid together. Incomplete custom input must not
      // overwrite the live proxy or block unrelated settings from saving.
      for (const key of proxyFields) {
        if (
          key === "proxy_password" &&
          (!proxyPasswordDirty || settings[key] === undefined)
        )
          continue;
        const value =
          key === "proxy_url" ? settings[key].trim() : settings[key];
        if (key === "proxy_url" && value && !validProxyUrl(value)) continue;
        if (snapshot.value.database.settings[key] === value) continue;
        Object.assign(snapshot.value.database.settings, { [key]: value });
        pendingSettings.set(key, ++settingsRevision);
      }
      proxyPasswordDirty = false;
    }
    if (pendingSettings.size) void persistSettings();
  },
  { flush: "sync" },
);
const languages = [
  { value: "system", label: "跟随系统" },
  { value: "zh-CN", label: "简体中文" },
  { value: "en-US", label: "English" },
];
const proxyModes = [
  { value: "system", label: "跟随系统" },
  { value: "http", label: "HTTP" },
  { value: "socks", label: "SOCKS5" },
];
watchEffect(() => {
  setLanguage(
    snapshot.value.database.settings.language,
    snapshot.value.system_locale,
  );
  document.documentElement.lang = locale.value;
  document.title = t("猫咪盒子启动器");
});
watch(
  locale,
  () => {
    if (desktop)
      void getCurrentWindow()
        .setTitle(t("猫咪盒子启动器"))
        .catch(() => {});
  },
  { immediate: true },
);
const updateStatus = ref<UpdateStatus | null>(null);
const updateChecking = ref(false);
const updateError = ref("");
const updateProgress = ref<LauncherUpdateProgress>({
  stage: "idle",
  downloaded: 0,
  total: null,
  version: null,
  error: null,
});
const updateActive = computed(() =>
  ["checking", "downloading", "verifying", "installing", "restarting"].includes(
    updateProgress.value.stage,
  ),
);
const updateProgressLabel = computed(() => {
  const progress = updateProgress.value;
  if (progress.stage === "downloading")
    return progress.total
      ? t("下载更新 {progress}%", {
          progress: Math.min(
            100,
            Math.floor((progress.downloaded / progress.total) * 100),
          ),
        })
      : t("正在下载更新");
  if (progress.stage === "verifying") return t("正在验证更新");
  if (progress.stage === "installing") return t("正在安装更新");
  if (progress.stage === "restarting") return t("正在重启启动器");
  return t("正在检查更新");
});
async function installLauncherUpdate() {
  if (updateProgress.value.stage !== "ready" || updateActive.value) return;
  try {
    await rpc("install_launcher_update");
  } catch (e) {
    error.value = message(e);
  }
}
async function downloadLauncherUpdate() {
  if (!updateStatus.value?.available || updateActive.value) return;
  updateProgress.value = {
    stage: "checking",
    downloaded: 0,
    total: null,
    version: updateStatus.value.latest_version,
    error: null,
  };
  try {
    updateProgress.value = await rpc<LauncherUpdateProgress>(
      "download_launcher_update",
      { version: updateStatus.value.latest_version },
    );
    if (
      updateProgress.value.stage === "ready" &&
      !busy.value &&
      !snapshot.value.running.length
    )
      await installLauncherUpdate();
  } catch (e) {
    updateProgress.value = {
      ...updateProgress.value,
      stage: "error",
      error: message(e),
    };
    error.value = message(e);
    appendLog({ time: stamp(), kind: "error", message: message(e) });
  }
}
async function cancelLauncherUpdate() {
  try {
    await rpc("cancel_launcher_update");
  } catch (e) {
    error.value = message(e);
  }
}
async function retryLauncherUpdate() {
  updateProgress.value = {
    stage: "idle",
    downloaded: 0,
    total: null,
    version: null,
    error: null,
  };
  await checkLauncherUpdate();
  if (updateStatus.value?.updater_url && !updateStatus.value.install_support)
    await downloadLauncherUpdate();
}
async function checkLauncherUpdate() {
  if (
    updateChecking.value ||
    updateActive.value ||
    updateProgress.value.stage === "ready"
  )
    return;
  updateChecking.value = true;
  updateError.value = "";
  updateStatus.value = null;
  try {
    updateStatus.value = await rpc<UpdateStatus>("check_update");
  } catch (e) {
    updateError.value = message(e);
    appendLog({ time: stamp(), kind: "error", message: updateError.value });
  } finally {
    updateChecking.value = false;
  }
}
function navigateSettingsTabs(event: KeyboardEvent, index: number) {
  let next = index;
  if (event.key === "ArrowRight") next = (index + 1) % settingsTabs.length;
  else if (event.key === "ArrowLeft")
    next = (index + settingsTabs.length - 1) % settingsTabs.length;
  else if (event.key === "Home") next = 0;
  else if (event.key === "End") next = settingsTabs.length - 1;
  else return;
  event.preventDefault();
  settingsTab.value = settingsTabs[next].id;
  document.getElementById(`settings-tab-${settingsTab.value}`)?.focus();
}
const manifest = ref<VersionManifest | null>(null);
const versionsLoading = ref(false);
const downloadSubmitting = ref(false);
let downloadDraftInitialized = false;
const selectedInstanceId = ref("");
const busy = ref(false);
const loginBusy = ref(false);
let loginSession: string | null = null;
let loginCancellation: Promise<void> | null = null;
const directoryPicking = ref(false);
const windowHidden = ref(document.hidden);
function updateVisibility() {
  windowHidden.value = document.hidden;
}
const current = ref<Task | null>(null);
const downloadControlBusy = ref(false);
const tasks = ref<Task[]>([]);
const displayedTask = computed(() => current.value || tasks.value[0] || null);
const taskInstance = computed(() =>
  snapshot.value.database.instances.find(
    (instance) => instance.id === displayedTask.value?.instance_id,
  ),
);
const notice = ref("");
const loaderNotice = ref("");
let loaderNoticeTimer: ReturnType<typeof setTimeout> | undefined;
function clearLoaderNotice() {
  clearTimeout(loaderNoticeTimer);
  loaderNotice.value = "";
}
function notifyLoaderUnavailable(kind: string) {
  clearLoaderNotice();
  loaderNotice.value = `${t("此游戏版本暂无可用的加载器")} · ${loaderLabel(kind)}`;
  loaderNoticeTimer = setTimeout(clearLoaderNotice, 2000);
}
const error = ref("");
const accountNameError = ref("");
const visibleError = computed(
  () =>
    error.value ||
    accountNameError.value ||
    settingsSaveError.value ||
    proxyPasswordError.value,
);
function dismissError() {
  error.value = "";
  accountNameError.value = "";
  settingsSaveError.value = "";
  proxyPasswordError.value = "";
}
const logStore = createLogStore();
function appendLog(line: LogLine) {
  logStore.append(line);
}
const showLogs = ref(false);
const logScope = ref("all");
const logSession = ref(0);
function openLogs(scope = "all") {
  modal.value = null;
  utility.value = null;
  logScope.value = scope;
  logSession.value++;
  showLogs.value = true;
}
const modal = ref<
  "add" | "create" | "edit" | "delete" | "version" | "manage" | "library" | null
>(null);
watch(modal, () => modalSession++, { flush: "sync" });
const modalTitles = {
  add: "添加游戏目录",
  create: "新建实例",
  edit: "实例设置",
  delete: "移除实例",
  version: "添加新的版本",
  manage: "管理版本",
  library: "资源管理",
};
const suggestedInstances = ref<SuggestedInstance[]>([]);
const discoveringInstances = ref(false);
const linkingDirectory = ref("");
let instanceDiscoveryRequest = 0;
const editing = ref<Instance | null>(null);
const managing = ref(false);
const managedInstance = computed(() =>
  snapshot.value.database.instances.find((i) => i.id === editing.value?.id),
);
const instanceWrites = reactive(new Set<string>());
const instanceSaving = computed(
  () => !!editing.value && instanceWrites.has(editing.value.id),
);
const instanceHeader = ref<InstanceType<typeof InstanceHeader>>();
const editingDirectory = ref("");
const directoryLoading = ref(false);
const form = reactive({
  name: "",
  location: "managed",
  external_directory: "",
  version: "1.21.1",
  memory_mb: 2048,
  memory_mode: "auto" as MemoryMode,
  java_path: "",
  icon: 0,
  loader: "vanilla",
  loader_version: "",
});
const invalidCreationLocation = computed(
  () =>
    (modal.value === "create" ||
      (modal.value === "version" && !editing.value)) &&
    form.location === "external" &&
    !form.external_directory.trim(),
);
const memoryOptions = ref<MemoryOptions | null>(null);
const memoryChoices = computed(() => [
  {
    value: "minimum",
    label: `${t("最小")} (${memoryOptions.value?.minimum_mb ?? "—"})`,
  },
  {
    value: "recommended",
    label: `${t("推荐")} (${memoryOptions.value?.recommended_mb ?? "—"})`,
  },
  { value: "auto", label: t("自动") },
  { value: "custom", label: t("自定义") },
]);
const validMemory = (value: number) =>
  Number.isInteger(value) && value >= 512 && value <= 65536;
const invalidCustomMemory = computed(
  () => form.memory_mode === "custom" && !validMemory(form.memory_mb),
);
async function loadMemoryOptions(id: string) {
  const session = modalSession;
  memoryOptions.value = null;
  try {
    const options = await rpc<MemoryOptions>("memory_options", { id });
    if (
      modal.value === "edit" &&
      editing.value?.id === id &&
      session === modalSession
    )
      memoryOptions.value = options;
  } catch {
    // A failed preview must not prevent saving; launch samples RAM independently.
  }
}
const loaderCatalog = createLoaderCatalog((game, kind) =>
  rpc<LoaderChoice[]>("loader_versions", { game_version: game, kind }),
);
const loaderLabel = (kind: string) =>
  ({ fabric: "Fabric", forge: "Forge", neoforge: "NeoForge" })[
    kind as "fabric" | "forge" | "neoforge"
  ] || t("原版");
const loaderVersions = ref<LoaderChoice[]>([]);
const unavailableLoaders = ref<string[]>([]);
const loaderLoading = ref(false);
const loaderOptions = computed(() =>
  loaderVersions.value.map((item) => ({
    value: item.version,
    label: item.version,
    detail: item.recommended
      ? t("推荐")
      : item.stable
        ? t("稳定版")
        : form.loader === "forge"
          ? undefined
          : t("测试版"),
  })),
);
const loaderUnavailable = computed(
  () =>
    modal.value === "version" &&
    form.loader !== "vanilla" &&
    (loaderLoading.value || !form.loader_version),
);
const packPreview = ref<ModpackPreview | null>(null);
const additionSection = ref<"game" | "modpacks" | "resources">("game");
const onlinePack = ref(false);
const additionVersion = ref("");
const additionVersions = computed(() =>
  (editing.value?.versions || []).map((version) => ({
    value: version.id,
    label: versionName(version),
  })),
);
const additionResourceSession = computed<ResourceSession | undefined>(() => {
  const instance =
    editing.value && instances.value.find((i) => i.id === editing.value!.id);
  if (!instance) return;
  const version =
    instance.versions.find((v) => v.id === additionVersion.value)?.id ||
    instance.versions.find((v) => v.id === instance.selected_version)?.id ||
    instance.versions[0]?.id;
  if (!version) return;
  return {
    instance,
    version,
    kind: "mods",
    platform:
      snapshot.value.database.settings.mod_download_source === "curseforge"
        ? "curseforge"
        : "modrinth",
  };
});
watch(
  () => editing.value?.id,
  () => {
    additionVersion.value = "";
  },
);
const additionResourceTask = computed(() =>
  tasks.value.find(
    (task) => task.resource_task && task.instance_id === editing.value?.id,
  ),
);
const modpackDownloadTask = computed(() =>
  tasks.value.find(
    (task) => task.workflow.steps[0]?.id === "pack_archive_resolve",
  ),
);
const packPath = ref("");
let packPreviewRequest = 0;
const canSubmitPack = computed(
  () =>
    !busy.value &&
    !downloadSubmitting.value &&
    !directoryPicking.value &&
    !downloadTargetRunning.value &&
    !invalidCreationLocation.value &&
    (!!editing.value || !!form.name.trim()),
);
const downloadTargetRunning = computed(
  () => !!editing.value && snapshot.value.running.includes(editing.value.id),
);
const canSubmitDownload = computed(
  () =>
    !busy.value &&
    !downloadSubmitting.value &&
    !directoryPicking.value &&
    !versionsLoading.value &&
    !loaderUnavailable.value &&
    !downloadTargetRunning.value &&
    !invalidCreationLocation.value &&
    (!!editing.value || !!form.name.trim()) &&
    !!manifest.value?.versions.some((version) => version.id === form.version),
);
let loaderRequest = 0;
async function loadLoaderVersions(refresh = false) {
  if (
    modal.value !== "version" ||
    packPreview.value ||
    form.loader === "vanilla" ||
    !form.version.trim()
  )
    return;
  const request = ++loaderRequest;
  const game = form.version.trim();
  const kind = form.loader;
  clearLoaderNotice();
  error.value = "";
  const cached = loaderCatalog.peek(game, kind);
  if (cached) {
    loaderVersions.value = cached.versions;
    form.loader_version = loaderCatalog.selected(game, kind, cached.versions);
  } else {
    loaderVersions.value = [];
    form.loader_version = "";
  }
  loaderLoading.value = !cached;
  try {
    const result = await loaderCatalog.load(game, kind, refresh);
    if (
      request !== loaderRequest ||
      modal.value !== "version" ||
      form.loader !== kind ||
      form.version.trim() !== game
    )
      return;
    if (!result.length) {
      if (!unavailableLoaders.value.includes(kind))
        unavailableLoaders.value.push(kind);
      form.loader = "vanilla";
      notifyLoaderUnavailable(kind);
      return;
    }
    unavailableLoaders.value = unavailableLoaders.value.filter(
      (item) => item !== kind,
    );
    loaderVersions.value = result;
    form.loader_version = loaderCatalog.selected(game, kind, result);
  } catch (e) {
    if (request === loaderRequest) error.value = message(e);
  } finally {
    if (request === loaderRequest) loaderLoading.value = false;
  }
}
watch(
  () => [modal.value, form.loader, form.version],
  (_, __, cleanup) => {
    clearLoaderNotice();
    ++loaderRequest;
    loaderVersions.value = [];
    form.loader_version = "";
    loaderLoading.value = false;
    if (
      modal.value !== "version" ||
      form.loader === "vanilla" ||
      !form.version.trim()
    )
      return;
    const cached = loaderCatalog.peek(form.version, form.loader);
    if (cached) {
      loaderVersions.value = cached.versions;
      form.loader_version = loaderCatalog.selected(
        form.version,
        form.loader,
        cached.versions,
      );
    }
    loaderLoading.value = !cached;
    const request = loaderRequest;
    const timer = setTimeout(() => {
      if (request === loaderRequest) void loadLoaderVersions();
    }, 300);
    cleanup(() => {
      clearTimeout(timer);
      ++loaderRequest;
    });
  },
  { flush: "sync" },
);
watch(
  () => form.loader_version,
  (version) => {
    if (version) loaderCatalog.remember(form.version, form.loader, version);
  },
  { flush: "sync" },
);
let availabilityRequest = 0;
watch(
  () => [modal.value, form.version],
  (_, __, cleanup) => {
    const request = ++availabilityRequest;
    unavailableLoaders.value = ["fabric", "forge", "neoforge"].filter(
      (kind) => loaderCatalog.peek(form.version, kind)?.versions.length === 0,
    );
    if (modal.value !== "version" || !form.version || packPreview.value) return;
    const game = form.version;
    const timer = setTimeout(() => {
      for (const kind of ["fabric", "forge", "neoforge"]) {
        if (form.loader === kind) continue;
        void loaderCatalog
          .load(game, kind)
          .then((result) => {
            if (
              request === availabilityRequest &&
              !result.length &&
              !unavailableLoaders.value.includes(kind)
            )
              unavailableLoaders.value.push(kind);
          })
          .catch(() => {});
      }
    }, 350);
    cleanup(() => {
      clearTimeout(timer);
      ++availabilityRequest;
    });
  },
  { flush: "sync" },
);
const javaRuntimes = ref<JavaInfo[]>([]);
const javaDiscovering = ref(false);
const javaPicking = ref(false);
const javaDiscoveryError = ref("");
const chosenJavaPaths = ref<string[]>([]);
try {
  const stored = JSON.parse(
    localStorage.getItem("nekocrate.java-paths") || "[]",
  );
  if (Array.isArray(stored))
    chosenJavaPaths.value = stored
      .filter(
        (path): path is string => typeof path === "string" && !!path.trim(),
      )
      .slice(0, 12);
} catch {
  /* Optional path history; existing instance configuration remains authoritative. */
}
const javaChoices = computed(() => {
  const choices: { value: string; label: string; detail?: string }[] = [
    {
      value: "",
      label: "自动",
      detail: javaDiscovering.value ? "正在探测 Java…" : "按游戏版本自动匹配",
    },
  ];
  const seen = new Set([""]);
  for (const runtime of [...javaRuntimes.value].sort(
    (a, b) => b.major - a.major || a.path.localeCompare(b.path),
  )) {
    if (seen.has(runtime.path)) continue;
    seen.add(runtime.path);
    const version = runtime.version.match(/"([^"]+)"/)?.[1] || runtime.major;
    choices.push({
      value: runtime.path,
      label: `${runtime.jdk === undefined ? "Java" : runtime.jdk ? "JDK" : "JRE"} ${version}${runtime.arch ? ` · ${runtime.arch === "aarch64" ? "ARM64" : runtime.arch}` : ""}`,
      detail: runtime.path,
    });
  }
  for (const path of [
    form.java_path,
    ...chosenJavaPaths.value,
    ...snapshot.value.database.instances.map((instance) => instance.java_path),
  ]) {
    if (!path || seen.has(path)) continue;
    seen.add(path);
    choices.push({ value: path, label: path, detail: "手动指定的 Java" });
  }
  return choices;
});
async function discoverJava() {
  if (javaDiscovering.value) return;
  javaDiscovering.value = true;
  javaDiscoveryError.value = "";
  try {
    javaRuntimes.value = await rpc<JavaInfo[]>("java");
  } catch (e) {
    javaDiscoveryError.value = message(e);
  } finally {
    javaDiscovering.value = false;
  }
}
const challenge = ref<Challenge | null>(null);
const loginAddress = computed(() =>
  challenge.value?.verification_uri.replace(/^https:\/\/(?:www\.)?/, ""),
);
const themes = backdropThemes;
const theme = ref("tide");
const hoveredInstance = ref<Instance | null>(null);
const instanceTipPosition = ref({ left: "0px", top: "0px" });
function previewInstance(event: MouseEvent | FocusEvent, instance: Instance) {
  if (draggedInstanceId.value) return;
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  hoveredInstance.value = instance;
  instanceTipPosition.value = {
    left: `${box.left + box.width / 2}px`,
    top: `${Math.min(box.bottom + 4, window.innerHeight - 24)}px`,
  };
}
function setTheme(id: string) {
  theme.value = id;
  try {
    localStorage.setItem("nekocrate.theme", id);
  } catch {
    /* optional persistence */
  }
}
const avatar = ref("");
const skinTexture = ref<string | null>(null);
const skinLoading = ref(false);
const skinError = ref("");
let skinGeneration = 0;
let unlisten: UnlistenFn | undefined;
let unlistenUpdate: UnlistenFn | undefined;
const instances = computed(() => snapshot.value.database.instances);
const instanceOrderSaving = ref(false);
const versionOrderSaving = ref(false);
const {
  strip: instanceStrip,
  toggle: instanceOverflowToggle,
  expanded: instanceDockExpanded,
  ghost: instanceGhost,
  trash: instanceTrash,
  trashVisible: instanceTrashVisible,
  trashAllowed: instanceTrashAllowed,
  trashHovered: instanceTrashHovered,
  draggedId: draggedInstanceId,
  draggedInstance,
  displayInstances: dockInstances,
  ghostPosition: instanceGhostPosition,
  ghostArtStyle: instanceGhostArtStyle,
  ...instanceSort
} = useInstanceSorting(
  instances,
  computed(
    () =>
      instanceOrderSaving.value ||
      !!modal.value ||
      !!utility.value ||
      showLogs.value,
  ),
  (id, beforeId) => void reorderInstance(id, beforeId),
  () => {
    hoveredInstance.value = null;
  },
  { allowed: canRemoveDraggedInstance, remove: removeDraggedInstance },
);
watch([modal, utility, showLogs], ([dialog, panel, logs]) => {
  if (dialog || panel || logs) instanceDockExpanded.value = false;
});
const dockLimit = computed(() => instanceVisibleLimit(instances.value.length));
const dockSections = computed(() => [
  {
    id: "main" as const,
    items: dockInstances.value.slice(0, dockLimit.value),
  },
  {
    id: "overflow" as const,
    items: dockInstances.value.slice(dockLimit.value),
  },
]);
const hiddenInstanceCount = computed(() =>
  Math.max(0, instances.value.length - dockLimit.value),
);
const selectedAccount = computed(() =>
  snapshot.value.database.accounts.find(
    (a) => a.id === snapshot.value.database.selected_account,
  ),
);
const guestName = computed(
  () => snapshot.value.database.guest_name || "Player",
);
const guestSkinId = computed(
  () => snapshot.value.database.guest_skin || "builtin:steve",
);
const guestTexture = ref<string | null>(null);
const guestSlim = ref(false);
const guestTextureLoading = ref(false);
const guestTextureError = ref("");
let guestTextureGeneration = 0;
function textureFromBytes(bytes: number[] | null): string | null {
  if (!bytes) return null;
  let binary = "";
  for (let i = 0; i < bytes.length; i += 8192)
    binary += String.fromCharCode(...bytes.slice(i, i + 8192));
  return `data:image/png;base64,${btoa(binary)}`;
}
async function loadGuestSkin() {
  const generation = ++guestTextureGeneration;
  const id = guestSkinId.value;
  const builtin = builtinSkins.find((skin) => skin.id === id);
  guestTextureError.value = "";
  guestTextureLoading.value = false;
  if (builtin) {
    guestTexture.value =
      builtin.id === "builtin:steve" ? null : builtin.texture;
    guestSlim.value = builtin.slim;
    return;
  }
  guestTextureLoading.value = true;
  try {
    const result = await rpc<{ skin: SavedSkin; bytes: number[] | null }>(
      "resource_manager",
      {
        action: { kind: "guest_skin" },
      },
    );
    if (generation !== guestTextureGeneration || result.skin.id !== id) return;
    guestTexture.value = textureFromBytes(result.bytes);
    guestSlim.value = result.skin.slim;
  } catch (error) {
    if (generation === guestTextureGeneration) {
      guestTexture.value = null;
      guestSlim.value = false;
      guestTextureError.value = message(error);
    }
  } finally {
    if (generation === guestTextureGeneration)
      guestTextureLoading.value = false;
  }
}
watch(
  () =>
    JSON.stringify([
      guestSkinId.value,
      snapshot.value.database.accounts.find(
        (account) => `account:${account.id}` === guestSkinId.value,
      )?.skin,
    ]),
  loadGuestSkin,
  { immediate: true },
);
let guestSkinWrite: Promise<void> = Promise.resolve();
let accountProfileWrite: Promise<unknown> = Promise.resolve();
function changeAccountProfile(
  account: string,
  request: () => Promise<Account>,
): Promise<Account> {
  const write = accountProfileWrite
    .catch(() => {})
    .then(async () => {
      const updated = await request();
      const index = snapshot.value.database.accounts.findIndex(
        (item) => item.id === account,
      );
      if (index >= 0 && updated.id === account) {
        accountRevision++;
        snapshot.value.database.accounts[index] = updated;
      }
      return updated;
    });
  accountProfileWrite = write;
  return write;
}
function chooseSkin(id: string): Promise<Account | null> {
  const account = snapshot.value.database.selected_account;
  if (!account) return chooseGuestSkin(id).then(() => null);
  return changeAccountProfile(account, () =>
    rpc<Account>("resource_manager", {
      action: { kind: "apply_account_skin", account, id },
    }),
  );
}
function chooseGuestSkin(id: string): Promise<void> {
  const write = guestSkinWrite
    .catch(() => {})
    .then(async () => {
      const saved = await rpc<string>("resource_manager", {
        action: { kind: "set_guest_skin", id },
      });
      accountRevision++;
      snapshot.value.database.guest_skin = saved;
    });
  guestSkinWrite = write;
  return write;
}
async function refreshGuestSkin() {
  try {
    await sync();
    await loadGuestSkin();
  } catch (e) {
    error.value = message(e);
  }
}
const guestNameDraft = ref(guestName.value);
const guestNameSaving = ref(false);
const guestNameEditing = ref(false);
const guestNameInput = ref<HTMLInputElement>();
const accountNameEditing = ref<string | null>(null);
const accountNameDraft = ref("");
const accountNameSaving = ref(false);
const accountNameInputs = ref<HTMLInputElement[]>([]);
let accountNameWrite: Promise<void> | null = null;
const skinDirection = ref(1);
watch(
  () => snapshot.value.database.selected_account,
  (id, previous) => {
    const order = [
      null,
      ...snapshot.value.database.accounts.map((account) => account.id),
    ];
    skinDirection.value = order.indexOf(id) >= order.indexOf(previous) ? 1 : -1;
  },
);
let guestNameWrite: Promise<void> | null = null;
watch(guestName, (name) => {
  guestNameDraft.value = name;
});
const loadAccountTexture = createAccountSkinLoader(async (id) => {
  const bytes = await rpc<number[] | null>("account_skin", { id });
  return textureFromBytes(bytes);
});
function fetchAccountSkin(id: string): Promise<string | null> {
  const account = snapshot.value.database.accounts.find((a) => a.id === id);
  return account ? loadAccountTexture(account) : Promise.resolve(null);
}
const avatarTexture = ref<string | null>(null);
function updateAvatar(url: string) {
  avatar.value = url;
  avatarTexture.value = skinTexture.value;
}
let skinAccountId: string | undefined;
async function loadAccountSkin() {
  const generation = ++skinGeneration;
  const account = selectedAccount.value;
  const identity = account?.id || `guest:${guestSkinId.value}`;
  if (skinAccountId !== identity || (account && !account.skin)) {
    if (skinTexture.value) avatar.value = "";
    skinTexture.value = null;
  }
  skinAccountId = identity;
  skinError.value = "";
  skinLoading.value = false;
  if (!account) {
    skinTexture.value = guestTexture.value;
    skinLoading.value = guestTextureLoading.value;
    skinError.value = guestTextureError.value;
    return;
  }
  if (!account?.skin) return;
  skinLoading.value = true;
  try {
    const texture = await fetchAccountSkin(account.id);
    if (generation !== skinGeneration) return;
    if (!texture) throw new Error("账户暂无皮肤资料，请刷新账户。");
    skinTexture.value = texture;
  } catch (e) {
    if (generation === skinGeneration) {
      skinTexture.value = null;
      skinError.value = message(e);
    }
  } finally {
    if (generation === skinGeneration) skinLoading.value = false;
  }
}
watch(
  () =>
    [
      selectedAccount.value?.id,
      selectedAccount.value?.skin?.url,
      selectedAccount.value?.skin?.slim,
      ...(!selectedAccount.value
        ? [
            guestTexture.value,
            guestTextureLoading.value,
            guestTextureError.value,
          ]
        : []),
    ].join("|"),
  loadAccountSkin,
  { immediate: true },
);
const skinSlim = computed(
  () =>
    !!skinTexture.value &&
    (selectedAccount.value
      ? !!selectedAccount.value.skin?.slim
      : guestSlim.value),
);
const skinCaption = computed(() =>
  skinError.value
    ? "皮肤加载失败 · 显示 Steve"
    : !skinTexture.value
      ? "Steve"
      : "",
);
const selectedInstance = computed(
  () =>
    snapshot.value.database.instances.find(
      (i) => i.id === selectedInstanceId.value,
    ) || snapshot.value.database.instances[0],
);
const selectedVersion = computed(() =>
  selectedInstance.value?.versions.find(
    (v) => v.id === selectedInstance.value?.selected_version,
  ),
);
const selectedRunning = computed(
  () =>
    !!selectedInstance.value &&
    snapshot.value.running.includes(selectedInstance.value.id),
);
const utilityTitles = {
  accounts: "账户",
  downloads: "下载任务",
  settings: "全局设置",
};
const utilityTitle = computed(() =>
  t(
    utility.value === "accounts" && challenge.value
      ? "微软登录"
      : utility.value
        ? utilityTitles[utility.value]
        : "",
  ),
);
watch(utility, (panel) => {
  proxyPasswordRequest++;
  proxyPasswordLoading.value = false;
  if (panel === "settings") {
    hydrateSettings();
    void loadProxyPassword();
    settingsTab.value = "general";
    error.value = "";
    void checkLauncherUpdate();
  } else {
    hydratingSettings = true;
    settings.proxy_password = undefined;
    proxyPasswordDirty = false;
    hydratingSettings = false;
  }
});
function toggleUtility(panel: "accounts" | "downloads" | "settings") {
  showLogs.value = false;
  modal.value = null;
  if (utility.value === panel) closeUtility();
  else utility.value = panel;
}
function closeUtility() {
  if (utility.value === "accounts")
    void endLogin().catch((e) => {
      error.value = message(e);
    });
  utility.value = null;
}
function instanceMaterial(instance: Instance) {
  return resolveInstanceIcon(instance.icon, instance.id);
}
function selectInstance(id: string) {
  selectedInstanceId.value = id;
  try {
    localStorage.setItem("nekocrate.selected-instance", id);
  } catch {
    /* Storage may be unavailable in a restricted webview. */
  }
}
function canRemoveDraggedInstance(id: string) {
  return (
    !busy.value &&
    !snapshot.value.running.includes(id) &&
    !hasInstanceTask(id) &&
    instances.value.some((instance) => instance.id === id)
  );
}
async function removeDraggedInstance(id: string): Promise<boolean> {
  if (!canRemoveDraggedInstance(id)) return false;
  busy.value = true;
  error.value = "";
  let removed = false;
  try {
    await endLogin();
    await rpc("delete", { id });
    removed = true;
    const wasSelected = selectedInstance.value?.id === id;
    const index = instances.value.findIndex((instance) => instance.id === id);
    snapshot.value.database.instances = instances.value.filter(
      (instance) => instance.id !== id,
    );
    if (wasSelected)
      selectInstance(
        instances.value[Math.min(index, instances.value.length - 1)]?.id || "",
      );
    await sync();
  } catch (e) {
    error.value = message(e);
    appendLog({ time: stamp(), kind: "error", message: error.value });
  } finally {
    busy.value = false;
  }
  return removed;
}
async function reorderInstance(id: string, beforeId: string | null) {
  if (instanceOrderSaving.value) return;
  const previous = instances.value;
  const reordered = moveInstanceBefore(previous, id, beforeId);
  if (reordered.every((item, index) => item.id === previous[index]?.id)) return;
  if (selectedInstance.value) selectInstance(selectedInstance.value.id);
  hoveredInstance.value = null;
  snapshot.value.database.instances = reordered;
  instanceOrderSaving.value = true;
  let saved = false;
  try {
    await rpc("move_instance", { id, before_id: beforeId });
    saved = true;
    await sync();
  } catch (e) {
    if (!saved) {
      const positions = new Map(
        previous.map((item, index) => [item.id, index]),
      );
      snapshot.value.database.instances = [...instances.value].sort(
        (a, b) =>
          (positions.get(a.id) ?? Infinity) - (positions.get(b.id) ?? Infinity),
      );
    }
    error.value = message(e);
    appendLog({ time: stamp(), kind: "error", message: error.value });
  } finally {
    instanceOrderSaving.value = false;
  }
}
async function reorderVersion(
  id: string,
  version: string,
  beforeId: string | null,
): Promise<boolean> {
  if (versionOrderSaving.value) return false;
  const instance = instances.value.find((instance) => instance.id === id);
  if (!instance) return false;
  const previous = instance.versions;
  const reordered = moveInstanceBefore(previous, version, beforeId);
  if (reordered.every((item, index) => item.id === previous[index]?.id))
    return true;
  instance.versions = reordered;
  versionOrderSaving.value = true;
  let saved = false;
  try {
    await rpc("move_version", { id, version, before_id: beforeId });
    saved = true;
    await sync();
  } catch (e) {
    if (!saved) {
      const current = instances.value.find((item) => item.id === id);
      const positions = new Map(
        previous.map((item, index) => [item.id, index]),
      );
      if (current)
        current.versions = [...current.versions].sort(
          (a, b) =>
            (positions.get(a.id) ?? Infinity) -
            (positions.get(b.id) ?? Infinity),
        );
    }
    error.value = message(e);
    appendLog({ time: stamp(), kind: "error", message: error.value });
  } finally {
    versionOrderSaving.value = false;
  }
  return saved;
}
async function removeDraggedVersion(
  id: string,
  version: string,
): Promise<boolean> {
  if (
    busy.value ||
    versionOrderSaving.value ||
    snapshot.value.running.includes(id) ||
    hasInstanceTask(id)
  )
    return false;
  const instance = instances.value.find((item) => item.id === id);
  if (!instance?.versions.some((item) => item.id === version)) return false;
  busy.value = true;
  error.value = "";
  let removed = false;
  try {
    await endLogin();
    const result = await rpc<Instance>("delete_version", { id, version });
    removed = true;
    const current = instances.value.find((item) => item.id === id);
    if (current) {
      current.versions = result.versions;
      current.selected_version = result.selected_version;
    }
    await sync();
  } catch (e) {
    error.value = message(e);
    appendLog({ time: stamp(), kind: "error", message: error.value });
  } finally {
    busy.value = false;
  }
  return removed;
}
const stamp = () =>
  new Date().toLocaleTimeString(locale.value, { hour12: false });
async function rpc<T = unknown>(
  op: string,
  data: Record<string, unknown> = {},
): Promise<T> {
  if (!desktop)
    throw new Error("当前为界面预览。请运行桌面版以管理实例和启动游戏。");
  return invoke<T>("command", { request: { op, ...data } });
}
const folderDrop = useFolderDrop({
  rpc,
  sync,
  select: selectInstance,
  busy: () => busy.value,
  error: (text) => {
    error.value = text;
  },
});
const packDragging = ref(false);
const resourceDragging = ref(false);
const dropVersionMenu = ref<{
  revealForDrop: () => void;
  endDrop: () => void;
}>();
const dragBrowseInstance = ref<Instance>();
let browseTimer: ReturnType<typeof setTimeout> | undefined;
let browseKey = "";
let dockBeforeDrop = false;
watch(dropInteraction, (active) => {
  document.documentElement.classList?.toggle("resource-transferring", active);
  if (active) dockBeforeDrop = instanceDockExpanded.value;
});
function browseDropTarget(element: Element | null) {
  const dock = element?.closest<HTMLElement>(
    ".instance-dock [data-resource-instance]",
  );
  const overflow = element?.closest("[data-drop-overflow]");
  const key = overflow ? "overflow" : dock?.dataset.resourceInstance || "";
  if (key !== browseKey) {
    clearTimeout(browseTimer);
    browseKey = key;
    if (key)
      browseTimer = setTimeout(async () => {
        if (key === "overflow") instanceDockExpanded.value = true;
        else {
          dragBrowseInstance.value = instances.value.find(
            (instance) => instance.id === key,
          );
          await nextTick();
          if (dropInteraction.value) dropVersionMenu.value?.revealForDrop();
        }
      }, 320);
  }
  if (element?.closest("[data-resource-menu-trigger]"))
    dropVersionMenu.value?.revealForDrop();
}
function finishDropBrowse() {
  clearTimeout(browseTimer);
  browseKey = "";
  dragBrowseInstance.value = undefined;
  dropVersionMenu.value?.endDrop();
  instanceDockExpanded.value = dockBeforeDrop;
}
const resourceSession = ref<ResourceSession>();
const versionManager = ref<{ refresh: () => Promise<void> }>();
const resourceManager = ref<{
  drop: (paths: string[], kind?: string) => Promise<void>;
  refresh: () => Promise<void>;
}>();
const libraryBusy = ref(false);
const librarySkinPreview = ref(false);
const resourceTransfer = useResourceTransfer({
  instances: () => instances.value,
  running: () => snapshot.value.running,
  busy: () => busy.value || libraryBusy.value || managing.value,
  copy: copyDraggedResource,
  export: (source) => rpc("export_resource", { source }),
  browse: browseDropTarget,
  finishBrowse: finishDropBrowse,
  error: (problem) => {
    error.value = message(problem);
  },
});
async function copyDraggedResource(
  source: ResourceCopySource,
  target: ResourceDestination,
) {
  const copied = await action<boolean>("复制资源", "copy_resource", {
    source: {
      kind: source.kind,
      instance: source.instance,
      version: source.version,
      file: source.file,
    },
    id: target.instance,
    version: target.version,
  });
  if (copied === undefined) return;
  notify(
    copied
      ? t("已复制 {name} 到 {version}", {
          name: source.name,
          version: target.version,
        })
      : t("目标版本已有此资源"),
  );
  if (copied) {
    await Promise.all([
      versionManager.value?.refresh(),
      resourceManager.value?.refresh(),
    ]);
  }
}
const managedInitialVersion = ref<string>();
const managedInitialTab = ref<ContentKind>();
function openResourceManager() {
  utility.value = null;
  showLogs.value = false;
  error.value = "";
  libraryBusy.value = false;
  librarySkinPreview.value = false;
  modal.value = "library";
}
function manageResourceVersion(id: string, version: string, kind: ContentKind) {
  const instance = instances.value.find((i) => i.id === id);
  if (!instance) return;
  manageVersions(instance);
  managedInitialVersion.value = version;
  managedInitialTab.value = kind;
}
function installGlobalJava(major: number) {
  return action(
    "获取 Java",
    "resource_manager",
    { action: { kind: "install_java", major } },
    true,
  );
}
const resourceTask = computed(() =>
  tasks.value.find(
    (task) =>
      task.resource_task &&
      task.instance_id === resourceSession.value?.instance.id,
  ),
);
function openResources(
  instance: Instance,
  version: string,
  kind = "mods",
  files?: DetectedResource[],
) {
  if (busy.value || snapshot.value.running.includes(instance.id)) {
    notify("请先结束游戏或当前任务");
    return;
  }
  resourceSession.value = {
    instance,
    version,
    platform:
      snapshot.value.database.settings.mod_download_source === "curseforge"
        ? "curseforge"
        : "modrinth",
    kind: ([
      "mods",
      "resourcepacks",
      "datapacks",
      "shaderpacks",
      "saves",
    ].includes(kind)
      ? kind
      : "mods") as ResourceKind,
    files,
  };
}
async function prepareResources(op: string, data: Record<string, unknown>) {
  return action<ResourcePlan>("解析资源依赖", op, data);
}
async function installResources(plan: ResourcePlan) {
  const result = await action<boolean>(
    "添加资源",
    "install_resources",
    { id: plan.target.instance, version: plan.target.version, task: plan.id },
    true,
    plan.entries.some((entry) => entry.origin.platform !== "local"),
  );
  return result === true;
}
function resourcesInstalled() {
  void versionManager.value?.refresh();
}
async function handleResourceDrop(
  paths: string[],
  target: ReturnType<typeof resourceDropTarget>,
  inspected?: DetectedResource[],
  kind?: string,
  surface?: Element | null,
  destination?: Instance,
) {
  folderDrop.handle({ type: "leave" });
  try {
    const files =
      inspected ||
      (await rpc<DetectedResource[]>("inspect_resources", { paths }));
    const issue = resourceDropIssue(files, kind);
    if (issue) throw new Error(issue);
    if (busy.value) throw new Error(t("请等待当前任务完成。"));
    const route = routeResourceDrop(files);
    if (route === "folders") {
      if (modal.value === "add") modal.value = null;
      folderDrop.handle({ type: "drop", paths });
      return;
    }
    if (route === "modpack") {
      await openModpack(paths[0], destination);
      return;
    }
    if (route === "mixed")
      throw new Error(
        files.find((f) => f.issue)?.issue || "请分别导入资源、整合包或游戏目录",
      );
    if (!target)
      throw new Error("请先选择目标游戏版本，或将资源拖到版本列表中的指定版本");
    if (snapshot.value.running.includes(target.instance.id))
      throw new Error(t("请先结束目标实例的游戏"));
    const resourceKind =
      kind && !["instance", "version"].includes(kind)
        ? kind
        : files[0]?.kinds[0] || "mods";
    openResources(target.instance, target.version, resourceKind, files);
  } catch (e) {
    rejectDrop(surface || null, message(e));
  }
}
let dropHintTimer: ReturnType<typeof setTimeout> | undefined;
function rejectDrop(surface: Element | null, reason: string) {
  showDropHint(surface, reason, true);
  clearTimeout(dropHintTimer);
  dropHintTimer = setTimeout(clearDropHint, 2600);
}
let unlistenDrop: UnlistenFn | undefined;
let dropDisposed = false;
let incomingPaths: string[] = [];
let incomingFiles: Promise<DetectedResource[]> | undefined;
let incomingRevision = 0;
let lastDropPoint: { x: number; y: number } | undefined;
function incomingElement() {
  return lastDropPoint
    ? document.elementFromPoint(lastDropPoint.x, lastDropPoint.y)
    : null;
}
function dropScope(element: Element | null) {
  const surface =
    element?.closest<HTMLElement>(
      "[data-manual-pack-drop], [data-drop-kind], [data-resource-instance]",
    ) || null;
  const kind = surface?.dataset.dropKind;
  const manual = !!element?.closest("[data-manual-pack-drop]");
  const fallback = resourceSession.value
    ? {
        instance: resourceSession.value.instance.id,
        version: resourceSession.value.version,
      }
    : selectedInstance.value?.selected_version
      ? {
          instance: selectedInstance.value.id,
          version: selectedInstance.value.selected_version,
        }
      : undefined;
  return {
    surface,
    kind,
    manual,
    target: resourceDropTarget(element, instances.value, fallback),
    destination: instances.value.find(
      (instance) =>
        instance.id ===
        element?.closest<HTMLElement>("[data-resource-instance]")?.dataset
          .resourceInstance,
    ),
  };
}
async function previewIncoming() {
  const revision = incomingRevision;
  const element = incomingElement();
  browseDropTarget(element);
  const scope = dropScope(element);
  manualPackDragging.value = scope.manual;
  if (scope.manual) {
    showDropHint(scope.surface, t("导入缺失文件"));
    return;
  }
  if (scope.kind === "skins" || scope.kind === "java") {
    const issue =
      scope.kind === "skins" &&
      !incomingPaths.every((path) => /\.png$/i.test(path))
        ? t("请选择 PNG 皮肤文件")
        : undefined;
    showDropHint(
      scope.surface,
      issue || t(scope.kind === "skins" ? "导入皮肤" : "登记本地 Java"),
      !!issue,
    );
    return;
  }
  showDropHint(scope.surface, t("识别文件内容…"));
  try {
    const files = await incomingFiles;
    if (
      !files ||
      revision !== incomingRevision ||
      element !== incomingElement()
    )
      return;
    const issue = resourceDropIssue(files, scope.kind);
    const label =
      files[0]?.format === "resource"
        ? resourceKindLabel(
            scope.kind && !["instance", "version"].includes(scope.kind)
              ? scope.kind
              : files[0].kinds[0],
          )
        : resourceKindLabel(files[0]?.format || "");
    showDropHint(
      scope.surface,
      issue || t("导入{kind}", { kind: label }),
      !!issue,
    );
  } catch (error) {
    if (revision === incomingRevision)
      showDropHint(scope.surface, message(error), true);
  }
}
function endIncoming() {
  incomingRevision++;
  incomingPaths = [];
  incomingFiles = undefined;
  manualPackDragging.value = false;
  dropInteraction.value = false;
  packDragging.value = resourceDragging.value = false;
  folderDrop.handle({ type: "leave" });
  clearDropHint();
  finishDropBrowse();
}
async function receiveFiles(
  paths: string[],
  scope: ReturnType<typeof dropScope>,
  files?: Promise<DetectedResource[]>,
) {
  const manualTask = scope.manual ? displayedTask.value : null;
  endIncoming();
  if (scope.manual) {
    if (manualTask?.modpack_status?.missing.length)
      await supplyPackFiles(paths, manualTask);
    return;
  }
  if (scope.kind === "java" || scope.kind === "skins") {
    if (
      scope.kind === "skins" &&
      !paths.every((path) => /\.png$/i.test(path))
    ) {
      rejectDrop(scope.surface, t("请选择 PNG 皮肤文件"));
      return;
    }
    await resourceManager.value?.drop(paths, scope.kind);
    return;
  }
  try {
    await handleResourceDrop(
      paths,
      scope.target,
      await files,
      scope.kind,
      scope.surface,
      scope.destination,
    );
  } catch (error) {
    rejectDrop(scope.surface, message(error));
  }
}
async function listenForFolders() {
  try {
    const stop = await getCurrentWindow().onDragDropEvent((e) => {
      const payload = e.payload;
      if ("position" in payload)
        lastDropPoint = {
          x: payload.position.x / window.devicePixelRatio,
          y: payload.position.y / window.devicePixelRatio,
        };
      if (
        resourceTransfer.nativeEvent(
          payload.type,
          lastDropPoint?.x || 0,
          lastDropPoint?.y || 0,
        )
      )
        return;
      if (payload.type === "leave") {
        endIncoming();
        return;
      }
      if (payload.type === "drop") {
        void receiveFiles(
          payload.paths,
          dropScope(incomingElement()),
          incomingFiles,
        );
        return;
      }
      if (payload.type === "enter") {
        clearTimeout(dropHintTimer);
        incomingRevision++;
        incomingPaths = payload.paths;
        dropInteraction.value = true;
        incomingFiles = rpc<DetectedResource[]>("inspect_resources", {
          paths: payload.paths,
        });
        // Handle failures immediately even if the pointer leaves before preview awaits it.
        void incomingFiles.catch(() => {});
      }
      void previewIncoming();
    });
    if (dropDisposed) stop();
    else unlistenDrop = stop;
  } catch (e) {
    if (!dropDisposed) error.value = message(e);
  }
}
let snapshotRequest = 0;
let accountRevision = 0;
async function sync() {
  const request = ++snapshotRequest;
  const accountsAtStart = accountRevision;
  const response = await rpc<{
    state: Snapshot;
    manifest: VersionManifest | null;
    busy: boolean;
  }>("snapshot");
  if (request !== snapshotRequest) return;
  // A state read begun before sign-in/selection may still contain the old
  // account. Preserve the newer identity without dropping other game updates.
  if (accountsAtStart !== accountRevision) {
    response.state.database.accounts = snapshot.value.database.accounts;
    response.state.database.selected_account =
      snapshot.value.database.selected_account;
    response.state.database.guest_name = snapshot.value.database.guest_name;
    response.state.database.guest_skin = snapshot.value.database.guest_skin;
  }
  // Keep edits made while this snapshot was in flight; still accept game state.
  for (const key of pendingSettings.keys()) {
    Object.assign(response.state.database.settings, {
      [key]: snapshot.value.database.settings[key],
    });
  }
  snapshot.value = response.state;
  if (utility.value !== "settings") hydrateSettings();
  manifest.value = response.manifest;
}
function message(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
function notify(text: string) {
  clearTimeout(noticeTimer);
  notice.value = text;
  noticeTimer = setTimeout(() => {
    if (notice.value === text) notice.value = "";
  }, 2000);
}
async function action<T>(
  title: string,
  op: string,
  data: Record<string, unknown> = {},
  task = false,
  resourcePausable = false,
): Promise<T | undefined> {
  if (busy.value) {
    notify("请等待当前任务完成。");
    return;
  }
  busy.value = true;
  error.value = "";
  let entry: Task | undefined;
  if (task) {
    const instance = snapshot.value.database.instances.find(
      (instance) => instance.id === data.id,
    );
    const version =
      op === "install"
        ? instance?.versions.find(
            (version) => version.id === instance.selected_version,
          )
        : undefined;
    const previousTask = tasks.value.find(
      (previous) =>
        previous.operation === op &&
        previous.version_id === (version?.id ?? null) &&
        JSON.stringify(previous.arguments) === JSON.stringify(data) &&
        ["error", "cancelled", "waiting"].includes(previous.status),
    );
    const savedProgress = version
      ? Math.max(
          tasks.value.find(
            (previous) =>
              previous.instance_id === instance?.id &&
              previous.version_id === version.id,
          )?.download_progress ?? 0,
          version.download_progress ?? 0,
        )
      : (previousTask?.download_progress ?? 0);
    // Only the current workflow needs its detailed file list. Keep compact
    // history for per-version resume progress without retaining old file maps.
    for (const previous of tasks.value)
      if (previous !== previousTask) clearWorkflowFiles(previous.workflow);
    if (previousTask) resumeWorkflow(previousTask.workflow);
    entry = reactive<Task>({
      id: Date.now(),
      operation: op,
      arguments: { ...data },
      instance_id: typeof data.id === "string" ? data.id : null,
      version_id: version?.id ?? null,
      resource_task: op === "install_resources" ? String(data.task) : undefined,
      resource_pausable: resourcePausable,
      download_progress: Math.min(99, savedProgress),
      cancellable: [
        "versions",
        "add_loader_version",
        "install",
        "launch",
        "install_resources",
        "download_modpack",
        "download_declared_mod",
        "resource_manager",
      ].includes(op),
      title,
      status: "running",
      message: "正在准备…",
      progress: Math.min(99, savedProgress),
      indeterminate: savedProgress <= 0,
      started: stamp(),
      workflow:
        previousTask?.workflow ||
        createWorkflow(
          op === "install"
            ? version?.modpack
              ? "modpack"
              : version?.loader
                ? "loader"
                : op
            : op,
          title,
        ),
    });
    if (previousTask)
      tasks.value = tasks.value.filter((task) => task !== previousTask);
    tasks.value.unshift(entry);
    current.value = entry;
  }
  try {
    await endLogin();
    if (task && (settingsWrite || pendingSettings.size)) {
      await persistSettings();
      if (settingsSaveError.value) throw new Error(settingsSaveError.value);
    }
    const result = await rpc<T>(op, data);
    if (entry && op === "install" && result === false) {
      entry.status = "waiting";
      entry.indeterminate = false;
      entry.cancellable = false;
      entry.message = "等待手动导入文件";
      waitForManualFiles(entry.workflow);
    } else if (entry) {
      finishWorkflow(entry.workflow, "done");
      entry.status = "done";
      entry.progress = 100;
      if (entry.version_id) entry.download_progress = 100;
      entry.indeterminate = false;
      entry.message = "已完成";
    }
    await sync();
    return result;
  } catch (e) {
    const text = message(e);
    if (entry) {
      entry.status = text.includes("取消") ? "cancelled" : "error";
      finishWorkflow(entry.workflow, entry.status);
      entry.message = text;
    }
    error.value = text;
    appendLog({ time: stamp(), kind: "error", message: text });
  } finally {
    busy.value = false;
    if (entry) current.value = null;
  }
}
async function loadVersions() {
  if (versionsLoading.value || busy.value) return;
  versionsLoading.value = true;
  try {
    const result = await action<VersionManifest>(
      "同步版本目录",
      "versions",
      {},
      true,
    );
    if (result) {
      manifest.value = result;
      if (
        modal.value === "version" &&
        !result.versions.some((item) => item.id === form.version)
      )
        form.version = result.latest.release || result.versions[0]?.id || "";
    }
  } finally {
    versionsLoading.value = false;
  }
}
async function openInstancePicker() {
  utility.value = null;
  showLogs.value = false;
  error.value = "";
  suggestedInstances.value = [];
  modal.value = "add";
  const session = modalSession;
  const request = ++instanceDiscoveryRequest;
  discoveringInstances.value = desktop;
  if (!desktop) return;
  const isCurrent = () =>
    modal.value === "add" &&
    session === modalSession &&
    request === instanceDiscoveryRequest;
  try {
    const found = await rpc<SuggestedInstance[]>("suggested_instances");
    if (isCurrent()) suggestedInstances.value = found;
  } catch (e) {
    if (isCurrent()) error.value = message(e);
  } finally {
    if (isCurrent()) discoveringInstances.value = false;
  }
}
async function linkInstanceDirectory(path: string) {
  if (busy.value || linkingDirectory.value || modal.value !== "add") return;
  const session = modalSession;
  linkingDirectory.value = path;
  try {
    const result = await action<FolderImportResult>(
      "关联实例",
      "import_folders",
      {
        paths: [path],
      },
    );
    if (!result) return;
    const linked = result.added[0] || result.existing[0];
    if (linked) {
      selectInstance(linked.id);
      folderDrop.showLinked([
        { name: linked.name, path: linked.game_directory || path },
      ]);
      if (session === modalSession) modal.value = null;
    } else if (result.failures[0]) {
      error.value = result.failures[0].message;
    }
  } finally {
    linkingDirectory.value = "";
  }
}
async function chooseInstanceDirectory() {
  if (directoryPicking.value || busy.value || modal.value !== "add") return;
  const session = modalSession;
  directoryPicking.value = true;
  try {
    const path = await rpc<string | null>("pick_import_directory");
    if (path && modal.value === "add" && session === modalSession)
      await linkInstanceDirectory(path);
  } catch (e) {
    if (session === modalSession) error.value = message(e);
  } finally {
    directoryPicking.value = false;
  }
}
function resetInstanceDraft() {
  const baseName = t("新实例");
  const names = new Set(
    instances.value.map((instance) => instance.name.trim()),
  );
  let name = baseName;
  for (let suffix = 2; names.has(name); suffix++)
    name = `${baseName} ${suffix}`;
  form.name = name;
  form.location = "managed";
  form.icon = Math.floor(Math.random() * 16);
  form.external_directory = "";
}
function newInstance() {
  utility.value = null;
  error.value = "";
  resetInstanceDraft();
  form.version = manifest.value?.latest.release || "1.21.1";
  modal.value = "create";
}
function switchDownloadTarget(id: string | null) {
  if (
    modal.value !== "version" ||
    busy.value ||
    downloadSubmitting.value ||
    directoryPicking.value
  )
    return;
  if (id === null) {
    if (!downloadDraftInitialized) {
      resetInstanceDraft();
      downloadDraftInitialized = true;
    }
    editing.value = null;
  } else {
    const instance = instances.value.find((item) => item.id === id);
    if (instance) editing.value = instance;
  }
}
async function chooseCreationDirectory(direct = false) {
  if (
    directoryPicking.value ||
    busy.value ||
    !(
      modal.value === "create" ||
      (modal.value === "version" && !editing.value)
    ) ||
    (!direct && form.location !== "external")
  )
    return;
  const session = modalSession;
  directoryPicking.value = true;
  try {
    const path = await rpc<string | null>("pick_instance_location", {
      path: form.external_directory,
    });
    if (
      path &&
      (modal.value === "create" ||
        (modal.value === "version" && !editing.value)) &&
      (direct || form.location === "external") &&
      session === modalSession
    ) {
      form.external_directory = path;
      form.location = "external";
    }
  } catch (e) {
    if (session === modalSession) error.value = message(e);
  } finally {
    directoryPicking.value = false;
  }
}
function hasInstanceTask(id: string) {
  return (
    current.value?.instance_id === id &&
    ["running", "paused"].includes(current.value.status)
  );
}
async function openInstance(instance: Instance) {
  if (snapshot.value.running.includes(instance.id)) {
    openLogs(instance.id);
    return;
  }
  if (hasInstanceTask(instance.id)) {
    showLogs.value = false;
    modal.value = null;
    utility.value = "downloads";
    return;
  }
  await edit(instance);
}
async function edit(i: Instance) {
  // Switching directly between two editors also starts a fresh session.
  modalSession++;
  utility.value = null;
  error.value = "";
  editing.value = i;
  Object.assign(form, {
    name: i.name,
    version: i.selected_version || "",
    memory_mb: i.memory_mb,
    memory_mode: i.memory_mode || "auto",
    java_path: i.java_path || "",
    icon: instanceMaterial(i),
  });
  editingDirectory.value = "";
  directoryLoading.value = true;
  modal.value = "edit";
  void loadMemoryOptions(i.id);
  void discoverJava();
  try {
    const directory = await rpc<string>("instance_directory", { id: i.id });
    if (editing.value?.id === i.id) editingDirectory.value = directory;
  } catch (e) {
    if (editing.value?.id === i.id && modal.value === "edit")
      error.value = message(e);
  } finally {
    if (editing.value?.id === i.id) directoryLoading.value = false;
  }
}
async function chooseJava() {
  if (javaPicking.value || instanceSaving.value || modal.value !== "edit")
    return;
  const session = modalSession;
  const instanceId = editing.value?.id;
  javaPicking.value = true;
  try {
    const path = await open({
      multiple: false,
      directory: false,
      defaultPath: form.java_path || undefined,
      title: t("选择 bin/java 或 bin/java.exe"),
    });
    if (
      typeof path === "string" &&
      modal.value === "edit" &&
      session === modalSession &&
      editing.value?.id === instanceId
    ) {
      form.java_path = path;
      chosenJavaPaths.value = [
        path,
        ...chosenJavaPaths.value.filter((entry) => entry !== path),
      ].slice(0, 12);
      try {
        localStorage.setItem(
          "nekocrate.java-paths",
          JSON.stringify(chosenJavaPaths.value),
        );
      } catch {
        /* Path history is optional; saving the instance still persists its Java selection. */
      }
    }
  } catch (e) {
    if (modal.value === "edit" && session === modalSession)
      error.value = message(e);
  } finally {
    javaPicking.value = false;
  }
}
async function saveEditedInstance() {
  if (modal.value !== "edit" || !editing.value || instanceSaving.value) return;
  if (invalidCustomMemory.value) return;
  if (instanceHeader.value?.finishRename() === false) return;
  const id = editing.value.id;
  const session = modalSession;
  const isCurrentEditor = () =>
    modal.value === "edit" &&
    editing.value?.id === id &&
    modalSession === session;
  instanceWrites.add(id);
  error.value = "";
  try {
    await rpc("update", {
      id,
      name: form.name,
      memory_mb: validMemory(form.memory_mb)
        ? form.memory_mb
        : validMemory(editing.value.memory_mb)
          ? editing.value.memory_mb
          : 2048,
      memory_mode: form.memory_mode,
      java_path: form.java_path || null,
      icon: form.icon,
    });
    await sync();
    if (isCurrentEditor()) modal.value = null;
  } catch (e) {
    const text = message(e);
    if (isCurrentEditor()) error.value = text;
    appendLog({ time: stamp(), kind: "error", message: text });
  } finally {
    instanceWrites.delete(id);
  }
}
async function submit() {
  if (modal.value === "edit") {
    await saveEditedInstance();
    return;
  }
  if (modal.value === "version") {
    await submitDownload();
    return;
  }
  const session = modalSession;
  let result: Instance | boolean | undefined;
  if (modal.value === "create") {
    if (invalidCreationLocation.value || directoryPicking.value) return;
    result = await action<Instance>("创建实例", "create", {
      name: form.name,
      directory: form.location === "external" ? form.external_directory : null,
    });
  }
  if (result !== undefined) {
    if (typeof result === "object") selectInstance(result.id);
    if (session === modalSession) modal.value = null;
  }
}
async function submitDownload() {
  if (modal.value !== "version" || !canSubmitDownload.value) return;
  const session = modalSession;
  const selection = {
    game: form.version,
    loader: form.loader,
    loaderVersion: form.loader_version,
  };
  downloadSubmitting.value = true;
  try {
    let target = editing.value;
    if (!target) {
      const created = await action<Instance>("创建实例", "create", {
        name: form.name.trim(),
        icon: form.icon,
        directory:
          form.location === "external" ? form.external_directory : null,
      });
      if (!created) return;
      target = created;
      // Keep the created directory selected if adding a loader fails. Retrying
      // must not create another directory or discard the user's selection.
      if (session === modalSession) editing.value = created;
    }
    const instance =
      selection.loader !== "vanilla"
        ? await action<Instance>(
            "添加加载器版本",
            "add_loader_version",
            {
              id: target.id,
              game_version: selection.game,
              loader: {
                kind: selection.loader,
                version: selection.loaderVersion,
              },
            },
            true,
          )
        : await action<Instance>("添加版本", "add_version", {
            id: target.id,
            version: selection.game,
          });
    if (instance) {
      selectInstance(instance.id);
      if (session === modalSession) modal.value = null;
      await install(instance);
    }
  } finally {
    downloadSubmitting.value = false;
  }
}
async function openModpack(source?: string, destination?: Instance) {
  if (busy.value || downloadSubmitting.value) {
    notify("请等待当前任务完成。");
    return;
  }
  const request = ++packPreviewRequest;
  try {
    const path =
      source ||
      (await open({
        multiple: false,
        directory: false,
        filters: [
          { name: "Modrinth / CurseForge", extensions: ["mrpack", "zip"] },
        ],
      }));
    if (!path || typeof path !== "string") return;
    const preview = await rpc<ModpackPreview>("preview_modpack", { path });
    if (request !== packPreviewRequest) return;
    if (destination || modal.value !== "version") {
      editing.value = destination || selectedInstance.value || null;
      resetInstanceDraft();
      downloadDraftInitialized = true;
    }
    packPath.value = path;
    onlinePack.value = false;
    packPreview.value = preview;
    utility.value = null;
    modal.value = "version";
  } catch (e) {
    if (request === packPreviewRequest) error.value = message(e);
  }
}
async function downloadOnlineModpack(
  platform: ResourcePlatform,
  project: string,
  version: string,
) {
  const session = modalSession;
  const request = ++packPreviewRequest;
  const result = await action<{ path: string; preview: ModpackPreview }>(
    "下载整合包",
    "download_modpack",
    { platform, project, version },
    true,
    true,
  );
  if (!result) return false;
  // Closing the window while fetching must not reopen it or replace another draft.
  if (
    request !== packPreviewRequest ||
    session !== modalSession ||
    modal.value !== "version"
  )
    return false;
  onlinePack.value = true;
  packPath.value = result.path;
  packPreview.value = result.preview;
  return true;
}
async function submitModpack(name: string, optional: string[]) {
  if (!canSubmitPack.value || !packPreview.value) return;
  const source = packPath.value;
  const session = modalSession;
  downloadSubmitting.value = true;
  try {
    let target = editing.value;
    if (!target) {
      const created = await action<Instance>("创建实例", "create", {
        name: form.name.trim(),
        icon: form.icon,
        directory:
          form.location === "external" ? form.external_directory : null,
      });
      if (!created) return;
      target = created;
      editing.value = created;
    }
    const instance = await action<Instance>("导入整合包", "prepare_modpack", {
      id: target.id,
      path: source,
      name,
      optional,
    });
    if (!instance) return;
    selectInstance(instance.id);
    if (session === modalSession) modal.value = null;
    await install(instance);
  } finally {
    downloadSubmitting.value = false;
  }
}
const retryingFile = ref("");
async function retryDownloadFile(
  file: string,
  task: Task | null | undefined = displayedTask.value,
) {
  if (
    !task ||
    busy.value ||
    task.status !== "error" ||
    !task.workflow.files.get(file)?.retryable
  )
    return;
  busy.value = true;
  retryingFile.value = file;
  current.value = task;
  try {
    await endLogin();
    await rpc("retry_download_file", { file });
    for (const step of task.workflow.steps)
      if (step.fileCount && step.filesDone === step.fileCount)
        step.status = "done";
    task.message = "文件已恢复，点击继续任务完成后续安装";
  } catch (e) {
    task.message = message(e);
    error.value = task.message;
  } finally {
    current.value = null;
    retryingFile.value = "";
    busy.value = false;
  }
}
async function continueDownloadTask(
  task: Task | null | undefined = displayedTask.value,
) {
  if (
    !task?.operation ||
    !task.arguments ||
    busy.value ||
    downloadControlBusy.value
  )
    return;
  downloadControlBusy.value = true;
  try {
    if (task.operation === "install" && task.instance_id && task.version_id) {
      await rpc("select_version", {
        id: task.instance_id,
        version: task.version_id,
      });
      await sync();
    }
    downloadControlBusy.value = false;
    const result = await action<{ path?: string }>(
      task.title,
      task.operation,
      task.arguments,
      true,
      task.resource_pausable,
    );
    if (result?.path && task.operation === "download_modpack")
      await openModpack(result.path);
  } catch (e) {
    error.value = message(e);
  } finally {
    downloadControlBusy.value = false;
  }
}

const manualPackDragging = ref(false);
const manualPackImporting = ref(false);
const manualPackError = ref("");
watch(
  () => displayedTask.value?.id,
  () => {
    manualPackError.value = "";
  },
);
function showManualPack(task: Task) {
  if (current.value && current.value !== task) return;
  tasks.value = [task, ...tasks.value.filter((item) => item !== task)];
  modal.value = null;
  showLogs.value = false;
  resourceSession.value = undefined;
  utility.value = "downloads";
}
async function supplyPackFiles(
  dropped?: string[],
  target: Task | null = displayedTask.value,
) {
  const task = target;
  if (!task?.instance_id || !task.version_id || manualPackImporting.value)
    return;
  manualPackError.value = "";
  manualPackImporting.value = true;
  try {
    const paths = dropped || (await open({ multiple: true, directory: false }));
    if (!paths) return;
    await rpc("supply_modpack_files", {
      id: task.instance_id,
      version: task.version_id,
      paths: Array.isArray(paths) ? paths : [paths],
    });
    task.modpack_status = await rpc<ModpackStatus>("modpack_status", {
      id: task.instance_id,
      version: task.version_id,
    });
    if (
      !task.modpack_status.missing.length &&
      !task.modpack_status.completed &&
      !busy.value
    )
      await resumePack();
  } catch (e) {
    manualPackError.value = message(e);
  } finally {
    manualPackImporting.value = false;
  }
}

async function resumePack() {
  const task = displayedTask.value;
  const instance = instances.value.find((i) => i.id === task?.instance_id);
  if (!task?.version_id || !instance || busy.value) return;
  try {
    await rpc("select_version", { id: instance.id, version: task.version_id });
    await sync();
    await install({ ...instance, selected_version: task.version_id });
  } catch (e) {
    error.value = message(e);
  }
}
function newVersion(instance: Instance) {
  packPreview.value = null;
  additionSection.value =
    current.value?.workflow.steps[0]?.id === "pack_archive_resolve"
      ? "modpacks"
      : "game";
  additionVersion.value = "";
  onlinePack.value = false;
  error.value = "";
  utility.value = null;
  editing.value = instance;
  downloadDraftInitialized = false;
  form.version =
    manifest.value?.latest.release || manifest.value?.versions[0]?.id || "";
  form.loader = "vanilla";
  form.loader_version = "";
  modal.value = "version";
  if (!manifest.value && desktop) void loadVersions();
}
function manageVersions(instance: Instance) {
  managedInitialVersion.value = undefined;
  managedInitialTab.value = undefined;
  error.value = "";
  utility.value = null;
  editing.value = instance;
  managing.value = false;
  modal.value = "manage";
}
function switchManagedInstance(id: string) {
  if (modal.value !== "manage" || managing.value) return;
  const instance = instances.value.find((item) => item.id === id);
  if (!instance) return;
  error.value = "";
  managedInitialVersion.value = undefined;
  managedInitialTab.value = undefined;
  editing.value = instance;
}
async function readVersionContentCounts(version: string) {
  return rpc<VersionContentCounts>("version_content_counts", {
    id: editing.value!.id,
    version,
  });
}
const modActions = {
  download: (id: string, version: string, key: string) =>
    action<string>(
      "下载 Mod",
      "download_declared_mod",
      { id, version, key },
      true,
      true,
    ),
  downloading: (id: string, version: string, key: string) =>
    current.value?.operation === "download_declared_mod" &&
    current.value.arguments?.id === id &&
    current.value.arguments?.version === version &&
    current.value.arguments?.key === key,

  setEnabled: (id: string, version: string, file: string, enabled: boolean) =>
    rpc<string>("set_mod_enabled", { id, version, file, enabled }),
  list: (id: string, version: string, file: string) =>
    rpc<import("./modManagement").ModConfigFile[]>("mod_config_files", {
      id,
      version,
      file,
    }),
  read: (id: string, version: string, file: string, config: string) =>
    rpc<import("./modManagement").ModConfigDocument>("read_mod_config", {
      id,
      version,
      file,
      config,
    }),
  save: (
    id: string,
    version: string,
    file: string,
    config: string,
    revision: string,
    text: string,
  ) =>
    rpc<import("./modManagement").ModConfigDocument>("save_mod_config", {
      id,
      version,
      file,
      config,
      revision,
      text,
    }),
};
const loadVersionContent = createVersionContentLoader(rpc);
async function prefetchVersionContent(version: string, signal: AbortSignal) {
  return loadVersionContent.preload(editing.value!.id, version, signal);
}
async function readVersionContent(
  version: string,
  kind: ContentKind,
  options?: ContentLoadOptions,
) {
  return loadVersionContent(editing.value!.id, version, kind, options);
}
async function readContentMetadata(
  id: string,
  version: string,
  kind: ContentKind,
  file: string,
) {
  return rpc<ContentMetadata>("version_content_metadata", {
    id,
    version,
    kind,
    file,
  });
}
async function readVersionMetadata(version: string) {
  return rpc<VersionMetadata>("version_metadata", {
    id: editing.value!.id,
    version,
  });
}
async function openContentFolder(
  id: string,
  version: string,
  kind: ContentKind,
  file?: string,
  source?: string,
) {
  await rpc("open_content_folder", { id, version, kind, file, source });
}
async function readContentSource(
  id: string,
  version: string,
  kind: ContentKind,
  file: string,
  source: string,
) {
  return rpc<string>("version_content_source", {
    id,
    version,
    kind,
    file,
    source,
  });
}
async function renameVersion(version: string, name: string) {
  await rpc("rename_version", { id: editing.value!.id, version, name });
  await sync();
}
async function deleteVersion(version: string) {
  await rpc("delete_version", { id: editing.value!.id, version });
  await sync();
}
async function selectVersion(instance: Instance, version: string) {
  if (instance.selected_version === version) return;
  await action("切换版本", "select_version", {
    id: instance.id,
    version,
  });
}
async function install(
  i: Instance,
  repair = false,
): Promise<boolean | undefined> {
  const result = await action<boolean>(
    repair ? "校验与修复" : "安装游戏",
    "install",
    { id: i.id, repair },
    true,
  );
  const version = i.versions.find((v) => v.id === i.selected_version);
  if (version?.modpack && result !== true) {
    const task = tasks.value.find(
      (task) => task.instance_id === i.id && task.version_id === version.id,
    );
    if (task) {
      try {
        task.modpack_status = await rpc<ModpackStatus>("modpack_status", {
          id: i.id,
          version: version.id,
        });
      } catch (e) {
        error.value = message(e);
      }
      if (!modal.value && !utility.value && !showLogs.value)
        utility.value = "downloads";
      if (
        result === false &&
        task.modpack_status &&
        !task.modpack_status.completed &&
        !task.modpack_status.missing.length &&
        !manualPackImporting.value
      )
        return install(i, repair);
    }
  }
  return result;
}
async function startGame() {
  if (busy.value || selectedRunning.value || !selectedInstance.value) return;
  const waiting = tasks.value.find(
    (task) =>
      task.status === "waiting" &&
      task.instance_id === selectedInstance.value!.id &&
      task.version_id === selectedInstance.value!.selected_version,
  );
  if (waiting) {
    showManualPack(waiting);
    return;
  }
  await prepareAndLaunch(selectedInstance.value, { install, launch });
}
async function launch(i: Instance) {
  if (guestNameWrite) await guestNameWrite;
  if (accountNameWrite) await accountNameWrite;
  await action("启动游戏", "launch", { id: i.id }, true);
}
async function setDownloadPaused(paused: boolean) {
  const task = current.value;
  if (
    (!task?.version_id && !task?.resource_pausable) ||
    downloadControlBusy.value ||
    task.status !== (paused ? "running" : "paused")
  )
    return;
  downloadControlBusy.value = true;
  try {
    await rpc(paused ? "pause" : "resume");
    if (current.value !== task || !["running", "paused"].includes(task.status))
      return;
    task.status = paused ? "paused" : "running";
    setWorkflowPaused(task.workflow, paused);
    if (paused) task.message = "已暂停";
  } catch (e) {
    if (current.value === task) error.value = message(e);
  } finally {
    downloadControlBusy.value = false;
  }
}
async function cancel() {
  try {
    if (loginSession || loginCancellation) {
      await endLogin();
      return;
    }
    await rpc("cancel");
    notify("正在取消，已校验的文件将保留在缓存中。");
  } catch (e) {
    error.value = message(e);
  }
}
function endLogin(): Promise<void> {
  if (loginCancellation) return loginCancellation;
  const session = loginSession;
  if (!session) return Promise.resolve();
  loginSession = null;
  challenge.value = null;
  loginBusy.value = false;
  const cancellation = rpc("cancel_login", { session_id: session }).then(
    () => {},
  );
  loginCancellation = cancellation.finally(() => {
    loginCancellation = null;
  });
  return loginCancellation;
}
async function login() {
  if (busy.value || loginCancellation) return;
  if (!desktop) {
    error.value = "请在桌面启动器中登录，浏览器预览不支持微软授权。";
    return;
  }
  const session = crypto.randomUUID();
  loginSession = session;
  loginBusy.value = true;
  challenge.value = null;
  const isCurrent = () => loginSession === session;
  error.value = "";
  try {
    const account = await signInWithMicrosoft({
      begin: () => rpc<Challenge>("begin_login", { session_id: session }),
      isCurrent,
      showChallenge: (value) => {
        challenge.value = value;
      },
      copyCode: writeClipboard,
      copyFailed: () => {
        error.value = "无法访问剪贴板，请手动选择并复制。";
      },
      openBrowser: (url) => openUrl(url),
      browserFailed: () => {
        error.value = "未能自动打开浏览器。请点击下方登录链接重试。";
      },
      waitForLogin: (value) =>
        rpc<Account>("wait_login", { session_id: value.session_id }),
    });
    if (!isCurrent() || !account) return;
    // wait_login already persisted this account. Render it in the same update
    // as dismissing the code, without an intervening stale list/full snapshot.
    accountRevision++;
    snapshot.value.database.accounts = [
      ...snapshot.value.database.accounts.filter((a) => a.id !== account.id),
      account,
    ];
    snapshot.value.database.selected_account = account.id;
    challenge.value = null;
    error.value = "";
  } catch (e) {
    if (!isCurrent()) return;
    error.value = message(e);
    challenge.value = null;
  } finally {
    if (isCurrent()) {
      loginSession = null;
      loginBusy.value = false;
    }
  }
}
async function chooseAccount(account: Account) {
  return chooseIdentity(account.id);
}
async function editAccountName(account: Account) {
  if (busy.value || loginBusy.value || accountNameSaving.value) return;
  accountNameEditing.value = account.id;
  accountNameDraft.value = account.username;
  await nextTick();
  accountNameInputs.value[0]?.focus();
  accountNameInputs.value[0]?.select();
}
function cancelAccountNameEdit() {
  accountNameEditing.value = null;
  accountNameDraft.value = "";
}
function blurAccountName(event: FocusEvent) {
  if (
    !accountNameSaving.value &&
    !accountNameError.value &&
    !(event.currentTarget as HTMLElement).contains(
      event.relatedTarget as Node | null,
    )
  )
    cancelAccountNameEdit();
}
function saveAccountName(): Promise<void> {
  if (accountNameWrite) return accountNameWrite;
  const id = accountNameEditing.value;
  const account = snapshot.value.database.accounts.find(
    (item) => item.id === id,
  );
  if (!id || !account || busy.value || loginBusy.value)
    return Promise.resolve();
  const name = accountNameDraft.value.trim();
  if (name === account.username) {
    cancelAccountNameEdit();
    return Promise.resolve();
  }
  if (!/^[A-Za-z0-9_]{3,16}$/.test(name)) {
    error.value = "";
    accountNameError.value = t(
      "Java 版玩家名称须为 3–16 位英文字母、数字或下划线",
    );
    return Promise.resolve();
  }
  accountNameSaving.value = true;
  error.value = "";
  accountNameWrite = changeAccountProfile(id, () =>
    rpc<Account>("rename_account", { id, name }),
  )
    .then(() => {
      accountNameError.value = "";
      if (accountNameEditing.value === id) cancelAccountNameEdit();
    })
    .catch((e) => {
      error.value = "";
      accountNameError.value = message(e);
    })
    .finally(() => {
      accountNameSaving.value = false;
      accountNameWrite = null;
    });
  return accountNameWrite;
}
async function editGuestName() {
  if (busy.value || loginBusy.value || guestNameSaving.value) return;
  guestNameDraft.value = guestName.value;
  guestNameEditing.value = true;
  await nextTick();
  guestNameInput.value?.focus();
  guestNameInput.value?.select();
}
async function finishGuestNameEdit() {
  await saveGuestName();
  if (guestNameDraft.value.trim() === guestName.value)
    guestNameEditing.value = false;
}
function cancelGuestNameEdit() {
  guestNameDraft.value = guestName.value;
  guestNameEditing.value = false;
}
watch(utility, (panel) => {
  if (panel !== "accounts") {
    guestNameEditing.value = false;
    cancelAccountNameEdit();
  }
});
async function chooseIdentity(id: string | null) {
  if (busy.value || loginBusy.value) return;
  if (id === snapshot.value.database.selected_account) {
    if (id !== null) utility.value = null;
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    await rpc("select_account", { id });
    accountRevision++;
    snapshot.value.database.selected_account = id;
    if (id !== null) utility.value = null;
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}
function saveGuestName(): Promise<void> {
  if (guestNameWrite) return guestNameWrite;
  if (busy.value || loginBusy.value) return Promise.resolve();
  const name = guestNameDraft.value.trim();
  if (name === guestName.value) {
    guestNameDraft.value = name;
    return Promise.resolve();
  }
  if (!/^[A-Za-z0-9_]{1,16}$/.test(name)) {
    error.value = t("游客名称须为 1–16 位英文字母、数字或下划线");
    return Promise.resolve();
  }
  guestNameSaving.value = true;
  error.value = "";
  // A blur save must not disable the account/avatar that the user just clicked.
  guestNameWrite = (async () => {
    try {
      const saved = await rpc<string>("set_guest_name", { name });
      accountRevision++;
      snapshot.value.database.guest_name = saved;
      guestNameDraft.value = saved;
    } catch (e) {
      error.value = message(e);
    } finally {
      guestNameSaving.value = false;
      guestNameWrite = null;
    }
  })();
  return guestNameWrite;
}
function persistSettings(): Promise<void> {
  if (settingsWrite) return settingsWrite;
  if (!pendingSettings.size) return Promise.resolve();
  settingsSaving.value = true;
  settingsSaveError.value = "";
  // Coalesce edits from the same event and serialize writes without locking controls.
  settingsWrite = Promise.resolve().then(async () => {
    try {
      while (pendingSettings.size) {
        const submitted = { ...snapshot.value.database.settings };
        const revisions = new Map(pendingSettings);
        try {
          await rpc("save_settings", { settings: submitted });
        } catch (e) {
          settingsSaveError.value = message(e);
          appendLog({
            time: stamp(),
            kind: "error",
            message: settingsSaveError.value,
          });
          break;
        }
        // A completed save must not erase a newer edit to the same field.
        for (const [key, revision] of revisions) {
          if (pendingSettings.get(key) === revision)
            pendingSettings.delete(key);
        }
        if (!pendingSettings.has("proxy_password"))
          delete snapshot.value.database.settings.proxy_password;
        snapshotRequest++;
        try {
          await sync();
        } catch (e) {
          appendLog({
            time: stamp(),
            kind: "error",
            message: message(e),
          });
        }
      }
    } finally {
      settingsSaving.value = false;
      settingsWrite = null;
    }
  });
  return settingsWrite;
}
async function openFolder(id?: string) {
  try {
    await rpc("open_folder", { id: id || null });
  } catch (e) {
    error.value = message(e);
  }
}
async function openLink(url: string) {
  try {
    if (desktop) await openUrl(url);
    else window.open(url, "_blank", "noopener");
  } catch (e) {
    error.value = message(e);
  }
}
async function writeClipboard(text: string): Promise<void> {
  if (desktop) await rpc("copy_text", { text });
  else await navigator.clipboard.writeText(text);
}
async function copy(text: string) {
  try {
    await writeClipboard(text);
    notify("已复制");
  } catch {
    error.value = "无法访问剪贴板，请手动选择并复制。";
  }
}
function onEvent(e: LauncherEvent) {
  if (e.kind === "download_progress_recovered") {
    void sync();
    return;
  }
  if (
    current.value &&
    (!e.instance_id || e.instance_id === current.value.instance_id)
  ) {
    applyWorkflowEvent(current.value.workflow, e);
    if (e.pack_missing) current.value.modpack_status = e.pack_missing;
    if (current.value.status === "paused")
      setWorkflowPaused(current.value.workflow, true);
    if (!retryingFile.value && (e.kind === "stage" || e.kind === "workflow")) {
      current.value.progress = current.value.version_id
        ? current.value.download_progress
        : 0;
      current.value.indeterminate = current.value.progress <= 0;
    }
    if (
      !retryingFile.value &&
      e.kind === "progress" &&
      e.total > 0 &&
      e.overall_progress === undefined
    ) {
      current.value.progress = Math.min(
        100,
        Math.max(
          0,
          e.total_bytes
            ? (e.bytes / e.total_bytes) * 100
            : (e.completed / e.total) * 100,
        ),
      );
      const isPack = current.value.workflow.steps[0]?.id === "pack_files";
      if (isPack) {
        const [base, weight] =
          e.step === "pack_game"
            ? [70, 26]
            : e.step === "java"
              ? [96, 3]
              : [0, 70];
        current.value.progress = base + (current.value.progress * weight) / 100;
      }
      current.value.indeterminate = false;
      if (current.value.version_id) {
        current.value.download_progress =
          e.reset_progress && !isPack
            ? current.value.progress
            : Math.max(current.value.download_progress, current.value.progress);
        if (e.reset_progress) {
          const version = snapshot.value?.database.instances
            .find((instance) => instance.id === current.value!.instance_id)
            ?.versions.find(
              (version) => version.id === current.value!.version_id,
            );
          if (version)
            version.download_progress = current.value.download_progress;
        }
        current.value.progress = current.value.download_progress;
      }
    }
    if (
      !retryingFile.value &&
      typeof e.overall_progress === "number" &&
      Number.isFinite(e.overall_progress)
    ) {
      current.value.download_progress = Math.max(
        current.value.download_progress,
        Math.min(99, Math.max(0, e.overall_progress)),
      );
      current.value.progress = current.value.download_progress;
      current.value.indeterminate = false;
    }
    if (["stage", "progress", "retry", "warning", "complete"].includes(e.kind))
      current.value.message = e.message;
  }
  if (
    ![
      "transfer",
      "progress",
      "file",
      "file_plan",
      "workflow_progress",
    ].includes(e.kind)
  ) {
    appendLog({
      time: stamp(),
      kind: e.kind,
      message: e.message,
      instanceId: e.instance_id,
    });
  }
  if (e.kind === "game_crash") openLogs(e.instance_id || "all");
  if (["game_started", "game_exit", "game_crash"].includes(e.kind)) void sync();
}
onMounted(async () => {
  document.addEventListener("visibilitychange", updateVisibility);
  try {
    selectedInstanceId.value =
      localStorage.getItem("nekocrate.selected-instance") || "";
    const storedTheme = localStorage.getItem("nekocrate.theme");
    if (themes.some((t) => t.id === storedTheme)) theme.value = storedTheme!;
  } catch {
    /* Fall back to the first instance. */
  }
  if (!desktop) return;
  void listenForFolders();
  try {
    unlisten = await listen<LauncherEvent>("launcher-event", (e) =>
      onEvent(e.payload),
    );
    unlistenUpdate = await listen<LauncherUpdateProgress>(
      "launcher-update",
      (e) => {
        updateProgress.value = e.payload;
      },
    );
    updateProgress.value = await rpc<LauncherUpdateProgress>(
      "launcher_update_state",
    );
    await sync();
  } catch (e) {
    error.value = message(e);
  }
});
onUnmounted(() => {
  void endLogin().catch(() => {});
  instanceDiscoveryRequest++;
  dropDisposed = true;
  unlistenDrop?.();
  folderDrop.dispose();
  clearTimeout(noticeTimer);
  clearTimeout(dropHintTimer);
  clearTimeout(browseTimer);
  clearDropHint();
  dropInteraction.value = false;
  clearLoaderNotice();
  skinGeneration++;
  logStore.dispose();
  unlisten?.();
  unlistenUpdate?.();
  document.removeEventListener("visibilitychange", updateVisibility);
});
</script>

<template>
  <div class="shell" :data-theme="theme">
    <ResourceTransferOverlay :drag="resourceTransfer" />
    <DropTargetOverlay />
    <ErrorNotice
      :message="visibleError || loaderNotice"
      :informational="!visibleError && !!loaderNotice"
      :retry="!!settingsSaveError && visibleError === settingsSaveError"
      :retrying="settingsSaving"
      @dismiss="visibleError ? dismissError() : clearLoaderNotice()"
      @retry="persistSettings"
    />
    <FolderDrop
      :controller="folderDrop"
      :pack="packDragging"
      :resource="resourceDragging"
    />
    <WorldBackdrop :key="theme" :theme="theme" :paused="windowHidden" />
    <header class="topbar">
      <button
        class="account-card glass-surface ui-reveal-item"
        :style="listRevealStyle(0)"
        :aria-label="t('账户管理')"
        @click="toggleUtility('accounts')"
      >
        <span class="account-avatar"
          ><img v-if="avatar" :src="avatar" alt="" /><UsersRound
            v-else
            :size="25" /><UserRound
            v-if="!selectedAccount"
            class="guest-avatar-mark"
            :size="11"
            aria-hidden="true"
        /></span>
        <span class="account-identity">
          <strong>{{ selectedAccount?.username || guestName }}</strong>
          <span
            v-if="selectedAccount"
            class="account-status"
            data-online="true"
            >{{
              selectedAccount.expires_at * 1000 > Date.now()
                ? t("已登录")
                : t("待刷新")
            }}</span
          >
          <span v-else class="account-status"> {{ t("游客") }} </span>
        </span>
        <ChevronDown class="account-chevron" :size="14" />
      </button>
      <div class="global-tools">
        <button
          class="icon-button ui-reveal-item"
          :style="listRevealStyle(1)"
          :title="t('资源管理')"
          :aria-label="t('资源管理')"
          @click="openResourceManager"
        >
          <Boxes :size="20" />
        </button>
        <button
          class="icon-button ui-reveal-item"
          :style="listRevealStyle(2)"
          :title="t('全局设置')"
          :aria-label="t('全局设置')"
          @click="toggleUtility('settings')"
        >
          <Settings2 :size="20" />
        </button>
        <button
          class="icon-button ui-reveal-item"
          :style="listRevealStyle(3)"
          :title="t('运行日志')"
          :aria-label="t('运行日志')"
          @click="openLogs()"
        >
          <Terminal :size="20" />
        </button>
      </div>
    </header>

    <main
      class="workspace"
      :style="{ opacity: modal === 'library' && librarySkinPreview ? 0 : 1 }"
      :aria-label="t('玩家皮肤预览')"
    >
      <SkinStage
        idle
        playful
        :identity="selectedAccount?.id || `guest:${guestSkinId}`"
        :direction="skinDirection"
        :texture="skinTexture"
        :slim="skinSlim"
        :loading="skinLoading"
        :paused="!!modal || !!utility || showLogs"
        :caption="t(skinCaption)"
        :issue="translateMessage(skinError)"
        @avatar="updateAvatar"
        @failed="skinError = $event"
        @retry="selectedAccount ? loadAccountSkin() : loadGuestSkin()"
      />
    </main>

    <footer class="bottom-bar">
      <ThemePicker
        class="ui-reveal-item"
        :style="listRevealStyle(3)"
        :model-value="theme"
        @update:model-value="setTheme"
      />
      <div class="game-controls">
        <nav
          class="instance-dock ui-reveal-item"
          :class="{
            empty: !instances.length,
            sorting: !!draggedInstanceId,
            overflowing: hiddenInstanceCount > 0,
          }"
          :style="{
            ...listRevealStyle(4),
            '--overflow-columns': Math.min(
              VISIBLE_INSTANCES,
              hiddenInstanceCount + 1,
            ),
            '--dock-columns': Math.min(
              dockLimit,
              Math.max(1, instances.length),
            ),
          }"
          :aria-label="t('实例')"
          @pointerdown.capture="instanceSort.resetClick"
          @pointermove="instanceSort.track"
          @pointerup="instanceSort.release"
          @pointercancel="instanceSort.cancel"
          @lostpointercapture="instanceSort.lostCapture"
          @click.capture="instanceSort.guardClick"
          @dblclick.capture="instanceSort.guardClick"
          @dragstart.prevent
        >
          <button
            v-if="hiddenInstanceCount"
            ref="instanceOverflowToggle"
            data-drop-overflow
            class="icon-button instance-overflow-toggle"
            :class="{
              expanded: instanceDockExpanded,
              selected: dockSections[1].items.some(
                (item) => item.id === selectedInstance?.id,
              ),
            }"
            :aria-expanded="instanceDockExpanded"
            aria-controls="instance-overflow"
            :aria-label="
              instanceDockExpanded
                ? t('收起实例')
                : t('展开其余 {count} 个实例，可拖动调整顺序', {
                    count: hiddenInstanceCount,
                  })
            "
            :title="
              instanceDockExpanded
                ? t('收起实例')
                : t('展开其余 {count} 个实例，可拖动调整顺序', {
                    count: hiddenInstanceCount,
                  })
            "
            @click="instanceSort.toggleExpanded"
          >
            <ChevronUp :size="20" /><small>{{ hiddenInstanceCount }}</small>
          </button>
          <button
            v-else
            class="icon-button add-instance"
            :disabled="busy"
            :aria-label="t('添加游戏目录')"
            :title="t('添加游戏目录')"
            @click="openInstancePicker"
          >
            <Plus :size="20" />
          </button>
          <Transition
            v-for="section in dockSections"
            :key="section.id"
            name="instance-overflow"
          >
            <div
              v-show="
                section.id === 'main'
                  ? instances.length
                  : instanceDockExpanded && hiddenInstanceCount
              "
              :id="section.id === 'overflow' ? 'instance-overflow' : undefined"
              :ref="(element) => instanceSort.setSurface(section.id, element)"
              class="instance-strip"
              :class="{
                'instance-overflow glass-surface': section.id === 'overflow',
              }"
              :aria-label="
                section.id === 'overflow' ? t('更多实例') : undefined
              "
              @scroll="hoveredInstance = null"
            >
              <TransitionGroup
                tag="div"
                name="instance-sort"
                class="instance-track"
              >
                <div
                  v-for="instance in section.items"
                  :key="instance.id"
                  class="instance-item"
                  :data-instance-id="instance.id"
                  :data-resource-instance="instance.id"
                  data-drop-kind="instance"
                  :data-resource-version="
                    instance.selected_version || undefined
                  "
                  :class="{
                    dragging: draggedInstanceId === instance.id,
                  }"
                >
                  <button
                    class="instance-tile"
                    :class="{ selected: selectedInstance?.id === instance.id }"
                    :aria-label="
                      t(
                        snapshot.running.includes(instance.id)
                          ? '{name}，{version}，双击查看运行日志，按 F2 设置，拖动或按 Alt 加左右方向键排序'
                          : hasInstanceTask(instance.id)
                            ? '{name}，{version}，双击查看下载进度，按 F2 设置，拖动或按 Alt 加左右方向键排序'
                            : '{name}，{version}，双击或按 F2 设置，拖动或按 Alt 加左右方向键排序',
                        {
                          name: instance.name,
                          version:
                            versionName(
                              instance.versions.find(
                                (v) => v.id === instance.selected_version,
                              ),
                            ) || t('尚无版本'),
                        },
                      )
                    "
                    :aria-pressed="selectedInstance?.id === instance.id"
                    aria-keyshortcuts="Alt+ArrowLeft Alt+ArrowRight F2"
                    @pointerdown="instanceSort.start($event, instance.id)"
                    @keydown="instanceSort.keyboard($event, instance.id)"
                    @click="selectInstance(instance.id)"
                    @dblclick="openInstance(instance)"
                    @contextmenu.prevent="edit(instance)"
                    @keydown.f2.prevent="edit(instance)"
                    @mouseenter="previewInstance($event, instance)"
                    @mouseleave="hoveredInstance = null"
                    @focus="previewInstance($event, instance)"
                    @blur="hoveredInstance = null"
                  >
                    <InstanceIcon :icon="instanceMaterial(instance)" />
                    <i
                      v-if="snapshot.running.includes(instance.id)"
                      class="running-dot"
                    />
                  </button>
                  <InstanceProgress
                    v-if="
                      current?.instance_id === instance.id &&
                      ['running', 'paused'].includes(current.status)
                    "
                    :title="current.title"
                    :message="current.message"
                    :progress="current.progress"
                    :indeterminate="
                      current.status === 'running' && current.indeterminate
                    "
                    @open="openInstance(instance)"
                  />
                </div>
                <button
                  v-if="section.id === 'overflow'"
                  key="add-directory"
                  class="instance-tile overflow-add-instance"
                  :disabled="busy"
                  :aria-label="t('添加游戏目录')"
                  :title="t('添加游戏目录')"
                  @click="openInstancePicker"
                >
                  <Plus :size="20" />
                </button>
              </TransitionGroup>
            </div>
          </Transition>
        </nav>
        <div
          class="launch-control glass-surface ui-reveal-item"
          :style="listRevealStyle(5)"
          :class="{ running: selectedRunning, download: !selectedVersion }"
        >
          <button
            class="launch-button"
            :class="{ unavailable: !selectedInstance }"
            :disabled="busy || !selectedInstance"
            :title="
              !selectedInstance
                ? t('请先新建实例')
                : !selectedVersion
                  ? t('为 {name} 下载游戏版本', { name: selectedInstance.name })
                  : selectedRunning
                    ? t('停止游戏')
                    : selectedVersion.installed
                      ? t('启动 {version}', {
                          version: versionName(selectedVersion),
                        })
                      : t('下载并启动 {version}', {
                          version: versionName(selectedVersion),
                        })
            "
            @click="
              selectedRunning
                ? action('停止游戏', 'stop', { id: selectedInstance?.id })
                : selectedVersion
                  ? startGame()
                  : selectedInstance && newVersion(selectedInstance)
            "
          >
            <span class="launch-content-position">
              <Transition name="launch-content">
                <span
                  :key="
                    selectedRunning
                      ? 'stop'
                      : selectedVersion
                        ? selectedVersion.installed
                          ? 'play'
                          : 'install'
                        : 'download'
                  "
                  class="launch-content"
                  :class="{
                    'has-version': !!selectedVersion,
                    'needs-install':
                      selectedVersion &&
                      !selectedVersion.installed &&
                      !selectedRunning,
                  }"
                >
                  <span class="launch-primary">
                    <LoaderCircle
                      v-if="
                        busy && current?.instance_id === selectedInstance?.id
                      "
                      class="spin"
                      :size="18"
                    /><Square
                      v-else-if="selectedRunning"
                      class="stop-icon"
                      :size="16"
                    /><Download
                      v-else-if="!selectedVersion?.installed"
                      :size="18"
                    /><Play v-else :size="18" />
                    <span class="launch-label">{{
                      selectedRunning
                        ? t("停止游戏")
                        : selectedVersion
                          ? selectedVersion.installed
                            ? t("启动游戏")
                            : t("下载并启动游戏")
                          : t("下载游戏")
                    }}</span>
                  </span>
                  <Transition name="launch-version">
                    <small
                      v-if="selectedVersion"
                      :key="selectedVersion.id"
                      class="launch-version"
                      >{{ versionName(selectedVersion) }}</small
                    >
                  </Transition>
                </span>
              </Transition>
            </span>
          </button>
          <Transition name="version-control">
            <VersionMenu
              ref="dropVersionMenu"
              v-if="selectedVersion || dragBrowseInstance"
              :instance="dragBrowseInstance || selectedInstance"
              :tasks="tasks"
              :locked="busy || selectedRunning || versionOrderSaving"
              :disabled="
                !dropInteraction &&
                (!selectedInstance || !!modal || !!utility || showLogs)
              "
              :move-version="reorderVersion"
              :remove-version="removeDraggedVersion"
              @select="
                selectedInstance && selectVersion(selectedInstance, $event)
              "
              @download="selectedInstance && newVersion(selectedInstance)"
              @manage="selectedInstance && manageVersions(selectedInstance)"
            />
          </Transition>
        </div>
      </div>
    </footer>
    <Transition name="instance-trash">
      <div
        v-if="instanceTrashVisible"
        ref="instanceTrash"
        class="instance-trash-target"
        :class="{
          active: instanceTrashHovered && instanceTrashAllowed,
          unavailable: !instanceTrashAllowed,
        }"
        role="img"
        :aria-label="
          t(
            instanceTrashAllowed
              ? '拖到此处移除实例'
              : '运行或任务处理中，暂不可移除实例',
          )
        "
      >
        <Trash2 :size="28" />
      </div>
    </Transition>
    <Transition name="instance-ghost">
      <div
        v-if="draggedInstance"
        ref="instanceGhost"
        class="instance-drag-ghost"
        :style="instanceGhostPosition"
        aria-hidden="true"
      >
        <InstanceIcon
          :icon="instanceMaterial(draggedInstance)"
          :style="instanceGhostArtStyle"
        />
      </div>
    </Transition>
    <div
      v-if="hoveredInstance && !modal && !utility && !showLogs"
      class="instance-tooltip"
      role="tooltip"
      :style="instanceTipPosition"
    >
      {{ hoveredInstance.name }}
    </div>

    <Transition
      name="dialog"
      appear
      @before-leave="prepareDialogLeave"
      @after-leave="finishDialogLeave"
    >
      <GlassDialog
        :notice="translateMessage(notice)"
        v-if="utility"
        :label="utilityTitle"
        @close="closeUtility"
      >
        <section
          :key="utility"
          v-panel-size="utility === 'settings'"
          class="utility-panel glass-surface"
          :class="{
            'account-panel': utility === 'accounts',
            'account-authorizing': utility === 'accounts' && !!challenge,
            'global-settings': utility === 'settings',
            'download-panel': utility === 'downloads',
          }"
        >
          <header v-if="utility !== 'downloads'" class="utility-panel-header">
            <div
              v-if="utility === 'settings'"
              class="settings-tabs dialog-heading"
              role="tablist"
              :aria-label="t('设置分类')"
              tabindex="-1"
              autofocus
            >
              <button
                v-for="(tab, index) in settingsTabs"
                :id="`settings-tab-${tab.id}`"
                :key="tab.id"
                type="button"
                role="tab"
                class="settings-tab"
                :aria-selected="settingsTab === tab.id"
                :aria-controls="`settings-panel-${tab.id}`"
                :tabindex="settingsTab === tab.id ? 0 : -1"
                @click="settingsTab = tab.id"
                @keydown="navigateSettingsTabs($event, index)"
              >
                {{ t(tab.label) }}
              </button>
            </div>
            <h2
              v-else
              class="dialog-heading selection-label"
              tabindex="-1"
              autofocus
            >
              <Transition name="selection-text"
                ><span :key="utilityTitle">{{ utilityTitle }}</span></Transition
              >
            </h2>
            <div class="inline-actions">
              <button
                v-if="utility === 'accounts' && selectedAccount && !challenge"
                class="icon-button"
                :disabled="busy"
                :aria-label="t('刷新账户与皮肤')"
                :title="t('刷新账户与皮肤')"
                @click="action('刷新账户', 'refresh_account')"
              >
                <RefreshCw :size="18" />
              </button>
              <button
                class="icon-button"
                :aria-label="t('关闭面板')"
                @click="closeUtility"
              >
                <X :size="16" />
              </button>
            </div>
          </header>
          <div class="utility-panel-content">
            <ContentSwitch
              v-if="utility === 'accounts'"
              class="page-inner account-pages"
              :content-key="
                challenge ? `login:${challenge.session_id}` : 'accounts'
              "
            >
              <div v-if="challenge" class="login-panel">
                <button
                  type="button"
                  class="device-code list-reveal-item"
                  :style="listRevealStyle(0)"
                  :title="t('复制验证码')"
                  :aria-label="
                    t('复制验证码 {code}', { code: challenge.user_code })
                  "
                  @click="copy(challenge.user_code)"
                >
                  <span class="device-code-value">{{
                    challenge.user_code
                  }}</span>
                  <Copy :size="16" aria-hidden="true" />
                </button>
                <div
                  class="login-status list-reveal-item"
                  :style="listRevealStyle(1)"
                >
                  <span class="waiting" role="status">
                    <LoaderCircle :size="12" class="spin" aria-hidden="true" />
                    {{ t("等待授权…") }}
                  </span>
                  <span class="login-expiry">
                    {{
                      t("有效期约 {minutes} 分钟", {
                        minutes: Math.ceil(challenge.expires_in / 60),
                      })
                    }}
                  </span>
                </div>
                <div
                  class="login-footer list-reveal-item"
                  :style="listRevealStyle(2)"
                >
                  <a
                    class="text-button login-address"
                    :href="challenge.verification_uri"
                    :title="challenge.verification_uri"
                    :aria-label="t('打开微软登录页')"
                    @click.prevent="openLink(challenge.verification_uri)"
                  >
                    <span>{{ loginAddress }}</span>
                    <ExternalLink :size="12" aria-hidden="true" />
                  </a>
                </div>
              </div>
              <template v-else>
                <TransitionGroup
                  name="account-choice"
                  tag="div"
                  @before-leave="pinPanelLeave"
                  @after-leave="resetPanelLeave"
                  @leave-cancelled="resetPanelLeave"
                  class="account-picker"
                  role="group"
                  :aria-label="t('选择启动账户')"
                >
                  <div
                    key="guest"
                    class="account-choice account-guest list-reveal-item"
                    :style="listRevealStyle(0)"
                  >
                    <button
                      class="account-select"
                      :class="{
                        current: snapshot.database.selected_account === null,
                      }"
                      :disabled="busy || loginBusy"
                      :aria-pressed="
                        snapshot.database.selected_account === null
                      "
                      :aria-label="
                        t('以游客身份启动 · {name}', { name: guestName })
                      "
                      @click="chooseIdentity(null)"
                    >
                      <span class="account-avatar-frame">
                        <AccountAvatar
                          :load-texture="fetchAccountSkin"
                          :texture="guestTexture"
                        />
                        <UserRound
                          class="guest-avatar-mark"
                          :size="14"
                          aria-hidden="true"
                        />
                      </span>
                    </button>
                    <input
                      v-if="guestNameEditing"
                      ref="guestNameInput"
                      v-model="guestNameDraft"
                      class="guest-name-input"
                      :aria-label="t('游客名称')"
                      :readonly="guestNameSaving"
                      maxlength="16"
                      autocomplete="off"
                      autocapitalize="off"
                      :spellcheck="false"
                      @blur="finishGuestNameEdit"
                      @keydown.enter.prevent="finishGuestNameEdit"
                      @keydown.esc.stop.prevent="cancelGuestNameEdit"
                    />
                    <button
                      v-else
                      class="account-name guest-name-button"
                      :disabled="busy || loginBusy || guestNameSaving"
                      :aria-label="t('修改游客名称')"
                      @click.stop="editGuestName"
                    >
                      {{ t("游客 · {name}", { name: guestName }) }}
                    </button>
                  </div>
                  <div
                    v-for="(account, index) in snapshot.database.accounts"
                    :key="account.id"
                    class="account-choice list-reveal-item"
                    :style="listRevealStyle(index + 1)"
                  >
                    <button
                      class="account-select"
                      :class="{
                        current:
                          account.id === snapshot.database.selected_account,
                      }"
                      :disabled="busy"
                      :aria-pressed="
                        account.id === snapshot.database.selected_account
                      "
                      :aria-label="
                        t('使用 {name} 启动游戏', { name: account.username })
                      "
                      @click="chooseAccount(account)"
                    >
                      <span class="account-avatar-frame">
                        <AccountAvatar
                          :account="account"
                          :preview="
                            account.id === selectedAccount?.id &&
                            !skinLoading &&
                            avatarTexture === skinTexture
                              ? avatar
                              : undefined
                          "
                          :load-texture="fetchAccountSkin"
                        />
                      </span>
                    </button>
                    <div
                      v-if="accountNameEditing === account.id"
                      class="account-name-editor"
                      :aria-busy="accountNameSaving"
                      @focusout="blurAccountName"
                      @keydown.esc.stop.prevent="cancelAccountNameEdit"
                    >
                      <input
                        ref="accountNameInputs"
                        v-model="accountNameDraft"
                        :aria-label="t('Java 版玩家名称')"
                        aria-describedby="account-name-hint"
                        :readonly="accountNameSaving"
                        maxlength="16"
                        autocomplete="off"
                        autocapitalize="off"
                        :spellcheck="false"
                        @keydown.enter.prevent="saveAccountName"
                      />
                      <button
                        class="icon-button"
                        :disabled="accountNameSaving"
                        :title="t('确认改名')"
                        :aria-label="t('确认改名')"
                        @pointerdown.prevent
                        @click.stop="saveAccountName"
                      >
                        <LoaderCircle
                          v-if="accountNameSaving"
                          :size="13"
                          class="spin"
                        />
                        <Check v-else :size="13" />
                      </button>
                    </div>
                    <button
                      v-else
                      class="account-name account-name-button"
                      :disabled="busy || loginBusy || accountNameSaving"
                      :aria-label="
                        t('{name} · 点击修改名称', { name: account.username })
                      "
                      @click.stop="editAccountName(account)"
                    >
                      {{ account.username }}
                    </button>
                    <button
                      class="icon-button account-remove"
                      :disabled="busy"
                      :aria-label="
                        t('移除账户 {name}', { name: account.username })
                      "
                      :title="t('移除账户')"
                      @click="
                        action('移除账户', 'remove_account', { id: account.id })
                      "
                    >
                      <X :size="14" />
                    </button>
                  </div>
                  <button
                    key="add-account"
                    class="account-add list-reveal-item"
                    :style="
                      listRevealStyle(snapshot.database.accounts.length + 1)
                    "
                    :disabled="busy || loginBusy"
                    :aria-busy="loginBusy"
                    :aria-label="t('添加微软账户')"
                    :title="t('添加微软账户')"
                    @click="login"
                  >
                    <span class="account-add-icon"
                      ><LoaderCircle
                        v-if="loginBusy"
                        :size="20"
                        class="spin" /><Plus v-else :size="24"
                    /></span>
                  </button>
                </TransitionGroup>
                <Transition name="selection-text">
                  <p
                    v-if="accountNameEditing"
                    id="account-name-hint"
                    class="account-name-hint"
                  >
                    {{ t("30天可修改一次") }}
                  </p>
                </Transition>
              </template>
            </ContentSwitch>

            <template v-if="utility === 'downloads'">
              <DownloadWorkflow
                v-if="displayedTask"
                :task="displayedTask"
                :control-pending="downloadControlBusy"
                :retrying="retryingFile"
                :retry-enabled="!busy && !downloadControlBusy"
                @retry="retryDownloadFile"
                @continue="continueDownloadTask"
                :name="taskInstance?.name || t(displayedTask.title)"
                :version="
                  versionName(
                    taskInstance?.versions.find(
                      (v) =>
                        v.id ===
                        (displayedTask.version_id ||
                          taskInstance?.selected_version),
                    ),
                  ) ||
                  displayedTask.version_id ||
                  taskInstance?.selected_version ||
                  undefined
                "
                @close="utility = null"
                @cancel="cancel"
                @pause="setDownloadPaused(true)"
                @resume="setDownloadPaused(false)"
              >
                <template #step-details="{ step }">
                  <ModpackMissing
                    v-if="
                      displayedTask.modpack_status &&
                      step.id ===
                        (displayedTask.workflow.steps.some(
                          (item) => item.id === 'pack_manual',
                        )
                          ? 'pack_manual'
                          : 'pack_files') &&
                      displayedTask.modpack_status.missing.length > 0
                    "
                    :status="displayedTask.modpack_status"
                    :disabled="manualPackImporting"
                    :dragging="manualPackDragging"
                    :error="manualPackError"
                    :running="
                      ['running', 'paused'].includes(displayedTask.status)
                    "
                    @supply="supplyPackFiles()"
                    @resume="resumePack"
                    @link="openLink"
                  />
                </template>
              </DownloadWorkflow>
              <div v-else class="workflow-empty">
                <span>{{ t("暂无下载任务") }}</span>
                <button
                  class="icon-button"
                  :aria-label="t('关闭面板')"
                  @click="utility = null"
                >
                  <X :size="16" />
                </button>
              </div>
            </template>

            <form
              v-if="utility === 'settings'"
              id="global-settings-form"
              class="page-inner settings-form"
              novalidate
              @submit.prevent
            >
              <div class="settings-pages">
                <ContentSwitch :content-key="settingsTab">
                  <div
                    v-if="settingsTab === 'general'"
                    key="general"
                    id="settings-panel-general"
                    class="settings-page"
                    role="tabpanel"
                    aria-labelledby="settings-tab-general"
                  >
                    <div
                      class="settings-language list-reveal-item"
                      :style="listRevealStyle(0)"
                    >
                      <div class="field">
                        <label id="language-label" for="language-trigger">{{
                          t("语言")
                        }}</label>
                        <GlassSelect
                          id="language"
                          compact
                          v-model="settings.language"
                          :options="languages"
                        />
                      </div>
                      <div class="toggle-row settings-language-sync">
                        <span id="game-language-label">{{
                          t("游戏启动时跟随")
                        }}</span>
                        <input
                          v-model="settings.apply_language_to_game"
                          type="checkbox"
                          role="switch"
                          aria-labelledby="game-language-label"
                        />
                      </div>
                    </div>
                    <div
                      class="settings-preference settings-update list-reveal-item"
                      :style="listRevealStyle(1)"
                    >
                      <div class="field settings-version">
                        <span>{{ t("启动器版本") }}</span>
                        <strong>v{{ launcherVersion }}</strong>
                      </div>
                      <div
                        v-if="updateActive"
                        class="settings-update-download"
                        role="status"
                        aria-live="polite"
                      >
                        <span class="settings-update-state"
                          ><LoaderCircle :size="14" class="spin" />{{
                            updateProgressLabel
                          }}</span
                        >
                        <button
                          v-if="
                            ['checking', 'downloading', 'verifying'].includes(
                              updateProgress.stage,
                            )
                          "
                          type="button"
                          class="text-button"
                          @click="cancelLauncherUpdate"
                        >
                          {{ t("取消") }}
                        </button>
                      </div>
                      <button
                        v-else-if="updateProgress.stage === 'ready'"
                        type="button"
                        class="text-button"
                        @click="installLauncherUpdate"
                      >
                        {{ t("安装并重启") }}
                      </button>
                      <button
                        v-else-if="updateProgress.stage === 'error'"
                        type="button"
                        class="text-button"
                        :title="translateMessage(updateProgress.error || '')"
                        @click="retryLauncherUpdate"
                      >
                        {{ t("更新失败，点击重试") }}
                      </button>
                      <span
                        v-else-if="updateChecking"
                        class="settings-update-state"
                        role="status"
                        ><LoaderCircle :size="14" class="spin" />
                        {{ t("正在检查更新") }}
                      </span>
                      <button
                        v-else-if="updateError"
                        type="button"
                        class="text-button"
                        :title="translateMessage(updateError)"
                        @click="checkLauncherUpdate"
                      >
                        {{ t("检查失败，点击重试") }}
                      </button>
                      <span
                        v-else-if="updateStatus && !updateStatus.available"
                        class="settings-update-state"
                        role="status"
                      >
                        {{ t("你已经是最新版") }}
                      </span>
                      <button
                        v-else-if="updateStatus?.available"
                        type="button"
                        class="text-button"
                        @click="
                          updateStatus.updater_url &&
                          !updateStatus.install_support
                            ? downloadLauncherUpdate()
                            : openLink(updateStatus.download_url)
                        "
                        :title="
                          updateStatus.install_support === 'package_manager'
                            ? t('DEB 安装版请通过软件包管理器更新')
                            : updateStatus.install_support
                              ? t('开发模式或未安装的程序请先安装发布版')
                              : ''
                        "
                      >
                        {{
                          t(
                            updateStatus.updater_url &&
                              !updateStatus.install_support
                              ? "最新版为 {version} · 更新并重启"
                              : "最新版为 {version} · 下载安装包",
                            {
                              version: updateStatus.latest_version,
                            },
                          )
                        }}
                      </button>
                    </div>
                    <button
                      type="button"
                      class="settings-directory list-reveal-item"
                      :style="listRevealStyle(2)"
                      :disabled="!snapshot.root"
                      :title="snapshot.root"
                      :aria-label="t('打开启动器数据目录')"
                      @click="openFolder()"
                    >
                      <FolderOpen :size="20" />
                      <span>
                        <strong> {{ t("启动器目录") }} </strong>
                        <small>{{
                          snapshot.root || t("使用系统应用数据目录")
                        }}</small>
                      </span>
                      <ExternalLink :size="15" />
                    </button>
                  </div>
                  <div
                    v-else-if="settingsTab === 'network'"
                    key="network"
                    id="settings-panel-network"
                    class="settings-page settings-fields"
                    role="tabpanel"
                    aria-labelledby="settings-tab-network"
                  >
                    <div
                      class="field settings-source-field list-reveal-item"
                      :style="listRevealStyle(0)"
                    >
                      <label
                        id="game-download-source-label"
                        for="game-download-source-trigger"
                      >
                        {{ t("游戏本体下载源") }}
                      </label>
                      <DownloadSourceSelect
                        kind="game"
                        v-model="settings.download_source"
                      />
                    </div>
                    <div
                      class="field settings-source-field list-reveal-item"
                      :style="listRevealStyle(1)"
                    >
                      <label
                        id="mod-download-source-label"
                        for="mod-download-source-trigger"
                        >{{ t("Mod 下载源") }}</label
                      >
                      <DownloadSourceSelect
                        kind="mod"
                        v-model="settings.mod_download_source"
                      />
                    </div>
                    <div
                      class="network-concurrency network-control-heading list-reveal-item"
                      :style="listRevealStyle(2)"
                    >
                      <label for="settings-concurrency">{{
                        t("并发数")
                      }}</label>
                      <div
                        class="network-range"
                        :class="{ disabled: settings.adaptive_concurrency }"
                      >
                        <span
                          class="network-range-fill"
                          :style="{
                            width: `calc((100% - 12px) * ${(settings.concurrency - 1) / 255})`,
                          }"
                          aria-hidden="true"
                        />
                        <input
                          id="settings-concurrency"
                          v-model.number="settings.concurrency"
                          type="range"
                          min="1"
                          max="256"
                          step="1"
                          :disabled="settings.adaptive_concurrency"
                          :aria-valuetext="
                            t('{count} 个并发下载', {
                              count: settings.concurrency,
                            })
                          "
                        />
                        <output
                          for="settings-concurrency"
                          :style="{
                            left: `calc(6px + (100% - 12px) * ${(settings.concurrency - 1) / 255})`,
                          }"
                          >{{ settings.concurrency }}</output
                        >
                      </div>
                      <div class="toggle-row network-inline-toggle">
                        <span id="adaptive-concurrency-label">{{
                          t("自适应")
                        }}</span>
                        <input
                          v-model="settings.adaptive_concurrency"
                          type="checkbox"
                          role="switch"
                          aria-labelledby="adaptive-concurrency-label"
                        />
                      </div>
                    </div>
                    <div
                      class="network-proxy list-reveal-item"
                      :style="listRevealStyle(3)"
                    >
                      <div class="field network-proxy-toggle">
                        <span id="proxy-enabled-label">{{ t("代理") }}</span>
                        <div class="toggle-row">
                          <input
                            v-model="proxyEnabled"
                            type="checkbox"
                            role="switch"
                            aria-labelledby="proxy-enabled-label"
                            aria-controls="proxy-options"
                            :aria-expanded="proxyEnabled"
                          />
                        </div>
                      </div>
                      <Transition name="network-options">
                        <div
                          v-if="proxyEnabled"
                          id="proxy-options"
                          class="proxy-options"
                        >
                          <div class="proxy-options-inner">
                            <div class="field">
                              <label
                                id="network-proxy-label"
                                for="network-proxy-trigger"
                                >{{ t("代理类型") }}</label
                              >
                              <GlassSelect
                                id="network-proxy"
                                floating
                                compact
                                :model-value="proxyChoice"
                                :options="proxyModes"
                                @update:model-value="selectProxyKind"
                              />
                            </div>
                            <label
                              v-if="proxyChoice !== 'system'"
                              class="field"
                            >
                              {{ t("代理地址") }}
                              <input
                                v-model.lazy="settings.proxy_url"
                                type="text"
                                spellcheck="false"
                                autocomplete="off"
                                :placeholder="
                                  proxyChoice === 'http'
                                    ? 'http://127.0.0.1:7890'
                                    : 'socks5h://127.0.0.1:1080'
                                "
                                :aria-invalid="!validSetting('proxy_url')"
                              />
                            </label>
                          </div>
                          <Transition name="network-options">
                            <div
                              v-if="proxyChoice !== 'system'"
                              class="proxy-auth-fields"
                            >
                              <label class="field">
                                {{ t("账号") }}
                                <input
                                  v-model.lazy="settings.proxy_username"
                                  type="text"
                                  autocomplete="off"
                                  spellcheck="false"
                                  :placeholder="t('留空不验证')"
                                />
                              </label>
                              <label class="field">
                                {{ t("密码") }}
                                <input
                                  v-model.lazy="settings.proxy_password"
                                  type="password"
                                  autocomplete="new-password"
                                  :disabled="proxyPasswordLoading"
                                  :aria-invalid="!!proxyPasswordError"
                                  :placeholder="t('留空不验证')"
                                />
                              </label>
                            </div>
                          </Transition>
                        </div>
                      </Transition>
                    </div>
                  </div>
                  <SettingsAbout
                    v-else
                    key="about"
                    :version="launcherVersion"
                    @open-link="openLink"
                  />
                </ContentSwitch>
              </div>
            </form>
          </div>
        </section>
      </GlassDialog>
    </Transition>
    <Transition
      name="dialog"
      appear
      @before-leave="prepareDialogLeave"
      @after-leave="finishDialogLeave"
    >
      <GlassDialog
        :notice="translateMessage(notice)"
        v-if="showLogs"
        :label="t('运行日志')"
        @close="showLogs = false"
      >
        <LogPanel
          :store="logStore"
          :instances="snapshot.database.instances"
          :initial-scope="logScope"
          :session="logSession"
          @copy="copy"
          @close="showLogs = false"
        />
      </GlassDialog>
    </Transition>

    <Transition name="toast"
      ><div v-if="notice" class="toast glass-surface" role="status">
        <CheckCircle2 :size="16" />{{ translateMessage(notice) }}
      </div></Transition
    >
    <Transition
      name="dialog"
      appear
      @before-leave="prepareDialogLeave"
      @after-leave="finishDialogLeave"
    >
      <GlassDialog
        :notice="translateMessage(notice)"
        v-if="modal"
        :label="modal === 'edit' ? form.name : t(modalTitles[modal])"
        :locked="
          modal === 'library'
            ? libraryBusy
            : modal === 'manage'
              ? managing
              : modal === 'version'
                ? downloadSubmitting
                : modal !== 'edit' && busy
        "
        @close="modal = null"
      >
        <ResourceManager
          v-if="modal === 'library'"
          ref="resourceManager"
          :rpc="rpc"
          :busy="busy"
          :running="snapshot.running"
          :mod-actions="modActions"
          :install-java="installGlobalJava"
          :accounts="snapshot.database.accounts"
          :selected-account="snapshot.database.selected_account"
          :guest-skin="guestSkinId"
          :guest-name="guestName"
          :select-skin="chooseSkin"
          :load-account-skin="fetchAccountSkin"
          @busy="libraryBusy = $event"
          @skin-preview="librarySkinPreview = $event"
          @skin-changed="refreshGuestSkin"
          @error="error = message($event)"
          @close="modal = null"
          @manage="manageResourceVersion"
        />
        <VersionManager
          ref="versionManager"
          v-else-if="modal === 'manage' && managedInstance"
          :initial-version="managedInitialVersion"
          :initial-tab="managedInitialTab"
          :instance="managedInstance"
          :instances="instances"
          :locked="busy || snapshot.running.includes(managedInstance.id)"
          :downloading="
            busy &&
            current?.instance_id === managedInstance.id &&
            current?.status === 'running'
          "
          :download-version="
            current?.version_id ||
            (current?.operation === 'download_declared_mod'
              ? String(current.arguments?.version || '')
              : undefined)
          "
          :mod-actions="modActions"
          :load-content="readVersionContent"
          :prefetch-content="prefetchVersionContent"
          :load-content-metadata="readContentMetadata"
          :open-content-folder="openContentFolder"
          :load-content-source="readContentSource"
          :load-counts="readVersionContentCounts"
          @select-instance="switchManagedInstance"
          @add-resources="
            (version, kind) =>
              managedInstance && openResources(managedInstance, version, kind)
          "
          :load-metadata="readVersionMetadata"
          :rename="renameVersion"
          :remove="deleteVersion"
          @error="error = message($event)"
          @busy="managing = $event"
          @close="modal = null"
        />
        <ModpackImport
          v-else-if="modal === 'version' && packPreview"
          :preview="packPreview"
          :online="onlinePack"
          @back="
            packPreview = null;
            additionSection = 'modpacks';
          "
          :target="editing"
          :instances="instances"
          v-model:name="form.name"
          v-model:icon="form.icon"
          v-model:location="form.location"
          :directory="form.external_directory"
          :root="snapshot.root"
          :disabled="busy || downloadSubmitting"
          :picking="directoryPicking"
          :can-submit="canSubmitPack"
          @target="switchDownloadTarget"
          @browse="chooseCreationDirectory(true)"
          @submit="submitModpack"
          @choose="openModpack()"
          @close="modal = null"
        />
        <VersionDownload
          v-else-if="modal === 'version'"
          v-model:section="additionSection"
          v-model:version="form.version"
          v-model:loader="form.loader"
          v-model:icon="form.icon"
          v-model:loader-version="form.loader_version"
          v-model:name="form.name"
          v-model:location="form.location"
          :target="editing"
          :instances="instances"
          :manifest="manifest"
          :loader-options="loaderOptions"
          :loader-loading="loaderLoading"
          :unavailable-loaders="unavailableLoaders"
          @retry-loader="loadLoaderVersions(true)"
          :versions-loading="versionsLoading"
          :disabled="busy || downloadSubmitting"
          :submitting="downloadSubmitting"
          :can-submit="canSubmitDownload"
          :directory="form.external_directory"
          :root="snapshot.root"
          :picking="directoryPicking"
          :running="downloadTargetRunning"
          @target="switchDownloadTarget"
          @refresh="loadVersions"
          @browse="chooseCreationDirectory(true)"
          @import-pack="openModpack()"
          @submit="submitDownload"
          @close="modal = null"
        >
          <template #content>
            <ModpackBrowser
              v-if="additionSection === 'modpacks'"
              :busy="busy"
              :rpc="rpc"
              :download="downloadOnlineModpack"
              @close="modal = null"
              :task="modpackDownloadTask"
              :control-pending="downloadControlBusy"
              :retrying="retryingFile"
              :retry-enabled="!busy && !downloadControlBusy"
              @retry="retryDownloadFile($event, modpackDownloadTask)"
              @continue="continueDownloadTask(modpackDownloadTask)"
              @error="error = message($event)"
              @link="openLink"
              @import="openModpack()"
              @pause="setDownloadPaused(true)"
              @resume="setDownloadPaused(false)"
              @cancel="cancel"
            />
            <template v-else-if="additionSection === 'resources'">
              <div
                v-if="additionResourceSession"
                class="addition-version-target"
              >
                <span>{{ t("版本") }}</span>
                <GlassSelect
                  id="addition-resource-version"
                  :model-value="additionResourceSession.version"
                  :options="additionVersions"
                  :disabled="busy || downloadTargetRunning"
                  literal-labels
                  floating
                  compact
                  @update:model-value="additionVersion = $event"
                />
              </div>
              <ResourceBrowser
                v-if="additionResourceSession && !downloadTargetRunning"
                embedded
                :key="`${additionResourceSession.instance.id}/${additionResourceSession.version}`"
                :session="additionResourceSession"
                :busy="busy"
                :rpc="rpc"
                :prepare="prepareResources"
                :install="installResources"
                :task="additionResourceTask"
                :control-pending="downloadControlBusy"
                :retrying="retryingFile"
                :retry-enabled="!busy && !downloadControlBusy"
                @retry="retryDownloadFile($event, additionResourceTask)"
                @continue="continueDownloadTask(additionResourceTask)"
                @error="error = message($event)"
                @link="openLink"
                @installed="resourcesInstalled"
                @pause="setDownloadPaused(true)"
                @resume="setDownloadPaused(false)"
                @cancel="cancel"
                @close="modal = null"
              />
              <div v-else class="addition-empty">
                <span>{{
                  t(
                    downloadTargetRunning
                      ? "请先停止此实例的游戏"
                      : "选择已有游戏版本后添加资源",
                  )
                }}</span>
                <button
                  v-if="!downloadTargetRunning"
                  class="text-button"
                  @click="additionSection = 'game'"
                >
                  {{ t("添加游戏版本") }}
                </button>
              </div>
            </template>
          </template>
        </VersionDownload>
        <section
          v-else
          :key="`${modal}:${editing?.id || ''}`"
          :data-drop-kind="modal === 'add' ? 'directory' : 'blocked'"
          class="modal glass-surface"
          :class="{ 'instance-settings': modal === 'edit' }"
        >
          <div class="modal-header">
            <InstanceHeader
              v-if="modal === 'edit'"
              ref="instanceHeader"
              v-model:name="form.name"
              v-model:icon="form.icon"
              :directory="editingDirectory"
              :directory-loading="directoryLoading"
              :disabled="instanceSaving"
              @open-directory="editing && openFolder(editing.id)"
            />
            <h2 v-else class="dialog-heading" tabindex="-1" autofocus>
              {{ t(modalTitles[modal]) }}
            </h2>
            <button
              class="icon-button"
              :disabled="modal !== 'edit' && busy"
              :aria-label="t('关闭对话框')"
              @click="modal = null"
            >
              <X :size="16" />
            </button>
          </div>
          <InstancePicker
            v-if="modal === 'add'"
            :candidates="suggestedInstances"
            :loading="discoveringInstances"
            :disabled="busy || directoryPicking"
            :linking="linkingDirectory"
            :picking="directoryPicking"
            @select="linkInstanceDirectory"
            @import="chooseInstanceDirectory"
            @create="newInstance"
          />
          <template v-else-if="modal === 'delete'"
            ><p
              class="delete-message list-reveal-item"
              :style="listRevealStyle(0)"
            >
              {{
                editing?.game_directory
                  ? t("仅从列表移除「{name}」，原目录和游戏文件保留在原处。", {
                      name: editing.name,
                    })
                  : t("「{name}」将移入系统回收站，可在其中恢复或清空。", {
                      name: editing?.name || "",
                    })
              }}
            </p>
            <div
              class="modal-actions list-reveal-item"
              :style="listRevealStyle(1)"
            >
              <button class="button" @click="modal = 'edit'">
                {{ t("取消") }}</button
              ><button
                class="button destructive"
                :disabled="busy"
                @click="
                  action('移除实例', 'delete', { id: editing?.id }).then(
                    (result) => {
                      if (result) modal = null;
                    },
                  )
                "
              >
                {{ t("移除实例") }}
              </button>
            </div></template
          >
          <form v-else @submit.prevent="submit">
            <div class="modal-fields">
              <label
                v-if="modal === 'create'"
                class="field list-reveal-item"
                :style="listRevealStyle(0)"
              >
                {{ t("实例名称") }}
                <input
                  v-model="form.name"
                  :placeholder="t('例如：日常游戏')"
                  :disabled="busy"
                  required
                  maxlength="80"
              /></label>
              <InstanceLocation
                v-if="modal === 'create'"
                class="list-reveal-item"
                :style="listRevealStyle(1)"
                v-model="form.location"
                :directory="form.external_directory"
                :root="snapshot.root"
                :disabled="busy || directoryPicking"
                :picking="directoryPicking"
                @browse="chooseCreationDirectory"
              />
              <template v-if="modal === 'edit'">
                <div
                  class="field instance-memory list-reveal-item"
                  :style="listRevealStyle(0)"
                >
                  <label
                    id="instance-memory-label"
                    for="instance-memory-trigger"
                    >{{ t("启动内存") }}</label
                  >
                  <div class="memory-input">
                    <GlassSelect
                      id="instance-memory"
                      v-model="form.memory_mode"
                      v-model:editable-value="form.memory_mb"
                      :editable="form.memory_mode === 'custom'"
                      :input-label="t('自定义内存')"
                      :input-min="512"
                      :input-max="65536"
                      :options="memoryChoices"
                      :disabled="instanceSaving"
                      literal-labels
                      floating
                      compact
                    />
                    <span>MB</span>
                  </div>
                </div>
                <div
                  class="field instance-java list-reveal-item"
                  :style="listRevealStyle(1)"
                >
                  <label id="instance-java-label" for="instance-java-trigger">{{
                    t("Java 可执行文件")
                  }}</label>
                  <GlassSelect
                    id="instance-java"
                    v-model="form.java_path"
                    class="java-select"
                    :disabled="instanceSaving || javaPicking"
                    :options="javaChoices"
                    :action-label="t('选择文件…')"
                    @action="chooseJava"
                  />
                  <small
                    v-if="javaDiscoveryError"
                    :title="translateMessage(javaDiscoveryError)"
                    >{{ t("探测失败，仍可自动匹配或选择文件。") }}</small
                  >
                </div>
              </template>
            </div>
            <div
              class="modal-actions list-reveal-item"
              :style="listRevealStyle(2)"
            >
              <div v-if="modal === 'edit'" class="instance-maintenance-actions">
                <button
                  type="button"
                  class="text-button"
                  :disabled="
                    busy || instanceSaving || !editing?.selected_version
                  "
                  @click="editing && install(editing, true)"
                >
                  <RefreshCw :size="16" /> {{ t("校验修复") }}
                </button>
                <button
                  type="button"
                  class="text-button danger"
                  :disabled="busy || instanceSaving"
                  @click="modal = 'delete'"
                >
                  <Trash2 :size="16" /> {{ t("移除实例") }}
                </button>
              </div>
              <button
                v-if="modal === 'create'"
                type="button"
                class="text-button import-link"
                :disabled="busy"
                @click="openInstancePicker"
              >
                {{ t("返回") }}
              </button>
              <button
                type="button"
                class="button"
                :disabled="modal !== 'edit' && busy"
                @click="modal = null"
              >
                {{ t("取消") }}</button
              ><button
                type="submit"
                class="button primary"
                :disabled="
                  modal === 'edit'
                    ? instanceSaving || invalidCustomMemory
                    : busy ||
                      loaderUnavailable ||
                      invalidCreationLocation ||
                      (modal === 'create' && directoryPicking)
                "
              >
                <LoaderCircle
                  v-if="modal === 'edit' ? instanceSaving : busy"
                  class="spin"
                  :size="16"
                />{{ modal === "edit" ? t("保存") : t("创建实例") }}
              </button>
            </div>
          </form>
        </section>
      </GlassDialog>
    </Transition>
    <Transition
      name="dialog"
      appear
      @before-leave="prepareDialogLeave"
      @after-leave="finishDialogLeave"
    >
      <GlassDialog
        v-if="resourceSession"
        :label="t('添加资源')"
        :locked="busy && !current?.resource_task"
        @close="resourceSession = undefined"
      >
        <ResourceBrowser
          :key="`${resourceSession.instance.id}:${resourceSession.version}`"
          :session="resourceSession"
          :busy="busy"
          :task="resourceTask"
          :control-pending="downloadControlBusy"
          :retrying="retryingFile"
          :retry-enabled="!busy && !downloadControlBusy"
          @retry="retryDownloadFile($event, resourceTask)"
          @continue="continueDownloadTask(resourceTask)"
          :rpc="rpc"
          :prepare="prepareResources"
          :install="installResources"
          @close="resourceSession = undefined"
          @error="error = message($event)"
          @link="openLink"
          @installed="resourcesInstalled"
          @pause="setDownloadPaused(true)"
          @resume="setDownloadPaused(false)"
          @cancel="cancel"
        />
      </GlassDialog>
    </Transition>
  </div>
</template>
