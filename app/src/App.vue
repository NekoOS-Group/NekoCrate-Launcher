<script setup lang="ts">
import { t, locale, setLanguage, translateMessage } from "./i18n";
import {
  computed,
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
  Settings2,
  UsersRound,
  FolderOpen,
  Play,
  Download,
  Square,
  RefreshCw,
  ChevronRight,
  ChevronDown,
  Terminal,
  X,
  Check,
  Copy,
  Trash2,
  ExternalLink,
  LoaderCircle,
  CircleAlert,
  CheckCircle2,
} from "lucide-vue-next";
import { backdropThemes } from "./backdropThemes";
import { resolveInstanceIcon } from "./instanceIcons";
import { moveInstanceBefore } from "./instanceOrder";
import { useInstanceSorting } from "./useInstanceSorting";
import { prepareDialogLeave, finishDialogLeave } from "./dialogMotion";
import { version as launcherVersion } from "../package.json";
import { prepareAndLaunch } from "./launch";
import { signInWithMicrosoft } from "./login";
import GlassDialog from "./components/GlassDialog.vue";
import SkinStage from "./components/SkinStage.vue";
import WorldBackdrop from "./components/WorldBackdrop.vue";
import VersionMenu from "./components/VersionMenu.vue";
import AccountAvatar from "./components/AccountAvatar.vue";
import InstanceHeader from "./components/InstanceHeader.vue";
import DownloadSourceSelect from "./components/DownloadSourceSelect.vue";
import GlassSelect from "./components/GlassSelect.vue";
import ThemePicker from "./components/ThemePicker.vue";
import InstanceIcon from "./components/InstanceIcon.vue";
import InstanceProgress from "./components/InstanceProgress.vue";
import type {
  Settings,
  Instance,
  Snapshot,
  VersionManifest,
  LauncherEvent,
  Challenge,
  Task,
  Account,
  UpdateStatus,
} from "./types";

const desktop = isTauri();
const utility = ref<"accounts" | "downloads" | "settings" | null>(null);
const snapshot = ref<Snapshot>({
  root: "",
  system_locale: navigator.language,
  database: {
    settings: {
      microsoft_client_id: "",
      download_source: "official",
      concurrency: 8,
      retries: 2,
      auto_java: true,
      language: "system",
      apply_language_to_game: false,
    },
    instances: [],
    accounts: [],
    selected_account: null,
  },
  running: [],
});
const settings = reactive<Settings>({ ...snapshot.value.database.settings });
const settingsSaving = ref(false);
let utilitySession = 0;
let modalSession = 0;
watch(utility, () => utilitySession++, { flush: "sync" });
const settingsTabs = [
  { id: "general", label: "通用设置" },
  { id: "network", label: "网络设置" },
] as const;
const settingsTab = ref<(typeof settingsTabs)[number]["id"]>("general");
let settingsInvalidFocus: string | null = null;
function focusSettingsField() {
  if (!settingsInvalidFocus) return;
  const input = document.getElementById(settingsInvalidFocus);
  if (input) {
    input.focus();
    settingsInvalidFocus = null;
  }
}
const languages = [
  { value: "system", label: "跟随系统" },
  { value: "zh-CN", label: "简体中文" },
  { value: "en-US", label: "English" },
];
watchEffect(() => {
  setLanguage(
    utility.value === "settings"
      ? settings.language
      : snapshot.value.database.settings.language,
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
async function checkLauncherUpdate() {
  if (updateChecking.value) return;
  updateChecking.value = true;
  updateError.value = "";
  updateStatus.value = null;
  try {
    updateStatus.value = await rpc<UpdateStatus>("check_update");
  } catch (e) {
    updateError.value = message(e);
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
const selectedInstanceId = ref("");
const busy = ref(false);
const directoryPicking = ref(false);
const windowHidden = ref(document.hidden);
function updateVisibility() {
  windowHidden.value = document.hidden;
}
const current = ref<Task | null>(null);
const tasks = ref<Task[]>([]);
const notice = ref("");
const error = ref("");
const logs = ref<{ time: string; kind: string; message: string }[]>([]);
const showLogs = ref(false);
const modal = ref<"create" | "import" | "edit" | "delete" | "version" | null>(
  null,
);
watch(modal, () => modalSession++, { flush: "sync" });
const modalTitles = {
  create: "新建实例",
  import: "导入实例",
  edit: "实例设置",
  delete: "移除实例",
  version: "下载游戏版本",
};
const editing = ref<Instance | null>(null);
const instanceHeader = ref<InstanceType<typeof InstanceHeader>>();
const editingDirectory = ref("");
const directoryLoading = ref(false);
const form = reactive({
  name: "",
  version: "1.21.1",
  source: "",
  memory_mb: 2048,
  java_path: "",
  icon: 0,
});
const challenge = ref<Challenge | null>(null);
const loginSetup = ref(false);
const loginClientId = ref("");
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
const instances = computed(() => snapshot.value.database.instances);
const instanceOrderSaving = ref(false);
const {
  strip: instanceStrip,
  ghost: instanceGhost,
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
);
const selectedAccount = computed(() =>
  snapshot.value.database.accounts.find(
    (a) => a.id === snapshot.value.database.selected_account,
  ),
);
async function fetchAccountSkin(id: string): Promise<string | null> {
  const bytes = await rpc<number[] | null>("account_skin", { id });
  if (!bytes) return null;
  let binary = "";
  for (let i = 0; i < bytes.length; i += 8192)
    binary += String.fromCharCode(...bytes.slice(i, i + 8192));
  return `data:image/png;base64,${btoa(binary)}`;
}
async function loadAccountSkin() {
  const generation = ++skinGeneration;
  const account = selectedAccount.value;
  if (skinTexture.value) avatar.value = "";
  skinTexture.value = null;
  skinError.value = "";
  skinLoading.value = false;
  if (!account?.skin) return;
  skinLoading.value = true;
  try {
    const texture = await fetchAccountSkin(account.id);
    if (generation !== skinGeneration) return;
    if (!texture) throw new Error("账户暂无皮肤资料，请刷新账户。");
    skinTexture.value = texture;
  } catch (e) {
    if (generation === skinGeneration) skinError.value = message(e);
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
      selectedAccount.value?.expires_at,
    ].join("|"),
  loadAccountSkin,
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
watch(utility, (panel) => {
  if (panel === "settings") {
    Object.assign(settings, snapshot.value.database.settings);
    settingsTab.value = "general";
    settingsInvalidFocus = null;
    error.value = "";
    void checkLauncherUpdate();
  }
});
function toggleUtility(panel: "accounts" | "downloads" | "settings") {
  showLogs.value = false;
  modal.value = null;
  if (panel === "accounts") loginSetup.value = false;
  utility.value = utility.value === panel ? null : panel;
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
    logs.value.push({ time: stamp(), kind: "error", message: error.value });
  } finally {
    instanceOrderSaving.value = false;
  }
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
let snapshotRequest = 0;
async function sync() {
  const request = ++snapshotRequest;
  const response = await rpc<{
    state: Snapshot;
    manifest: VersionManifest | null;
    busy: boolean;
  }>("snapshot");
  if (request !== snapshotRequest) return;
  snapshot.value = response.state;
  manifest.value = response.manifest;
}
function message(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}
function notify(text: string) {
  notice.value = text;
  setTimeout(() => {
    if (notice.value === text) notice.value = "";
  }, 4500);
}
async function action<T>(
  title: string,
  op: string,
  data: Record<string, unknown> = {},
  task = false,
): Promise<T | undefined> {
  if (busy.value) {
    notify("请等待当前任务完成。");
    return;
  }
  busy.value = true;
  error.value = "";
  let entry: Task | undefined;
  if (task) {
    entry = reactive<Task>({
      id: Date.now(),
      instance_id: typeof data.id === "string" ? data.id : null,
      cancellable: ["versions", "install", "launch"].includes(op),
      title,
      status: "running",
      message: "正在准备…",
      progress: 0,
      indeterminate: true,
      started: stamp(),
    });
    tasks.value.unshift(entry);
    current.value = entry;
  }
  try {
    const result = await rpc<T>(op, data);
    if (entry) {
      entry.status = "done";
      entry.progress = 100;
      entry.indeterminate = false;
      entry.message = "已完成";
    }
    await sync();
    return result;
  } catch (e) {
    const text = message(e);
    if (entry) {
      entry.status = text.includes("取消") ? "cancelled" : "error";
      entry.message = text;
    }
    error.value = text;
    logs.value.push({ time: stamp(), kind: "error", message: text });
  } finally {
    busy.value = false;
    if (entry) current.value = null;
  }
}
async function loadVersions() {
  const result = await action<VersionManifest>(
    "同步版本目录",
    "versions",
    {},
    true,
  );
  if (result) manifest.value = result;
}
function newInstance(importing = false) {
  utility.value = null;
  error.value = "";
  form.name = "";
  form.version = manifest.value?.latest.release || "1.21.1";
  form.source = "";
  modal.value = importing ? "import" : "create";
}
async function edit(i: Instance) {
  utility.value = null;
  error.value = "";
  editing.value = i;
  Object.assign(form, {
    name: i.name,
    version: i.selected_version || "",
    memory_mb: i.memory_mb,
    java_path: i.java_path || "",
    icon: instanceMaterial(i),
  });
  editingDirectory.value = "";
  directoryLoading.value = true;
  modal.value = "edit";
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
async function chooseDirectory() {
  if (directoryPicking.value || busy.value) return;
  directoryPicking.value = true;
  try {
    const path = await rpc<string | null>("pick_import_directory", {
      path: form.source,
    });
    if (typeof path === "string") form.source = path;
  } catch (e) {
    error.value = message(e);
  } finally {
    directoryPicking.value = false;
  }
}
async function chooseJava() {
  try {
    const path = await open({
      multiple: false,
      title: t("选择 bin/java 或 bin/java.exe"),
    });
    if (typeof path === "string") form.java_path = path;
  } catch (e) {
    error.value = message(e);
  }
}
async function submit() {
  const session = modalSession;
  if (modal.value === "edit" && instanceHeader.value?.finishRename() === false)
    return;
  let result: Instance | boolean | undefined;
  if (modal.value === "create")
    result = await action<Instance>("创建实例", "create", {
      name: form.name,
    });
  else if (modal.value === "import")
    result = await action<Instance>(
      "关联实例",
      "import",
      { source: form.source, name: form.name },
      true,
    );
  else if (modal.value === "version" && editing.value) {
    const instance = await action<Instance>("添加版本", "add_version", {
      id: editing.value.id,
      version: form.version.trim(),
    });
    if (instance) {
      selectInstance(instance.id);
      if (session === modalSession) modal.value = null;
      await install(instance);
    }
    return;
  } else if (modal.value === "edit" && editing.value)
    result = await action<boolean>("保存实例", "update", {
      id: editing.value.id,
      name: form.name,
      memory_mb: form.memory_mb,
      java_path: form.java_path || null,
      icon: form.icon,
    });
  if (result !== undefined) {
    if (typeof result === "object") selectInstance(result.id);
    if (session === modalSession) modal.value = null;
  }
}
function newVersion(instance: Instance) {
  error.value = "";
  utility.value = null;
  editing.value = instance;
  form.version = manifest.value?.latest.release || "1.21.1";
  modal.value = "version";
  if (!manifest.value && desktop) void loadVersions();
}
async function selectVersion(instance: Instance, version: string) {
  if (instance.selected_version === version) return;
  await action("切换版本", "select_version", {
    id: instance.id,
    version,
  });
}
async function install(i: Instance) {
  return action<boolean>(
    i.versions.find((v) => v.id === i.selected_version)?.installed
      ? "校验与修复"
      : "安装游戏",
    "install",
    { id: i.id },
    true,
  );
}
async function startGame() {
  if (busy.value || selectedRunning.value || !selectedInstance.value) return;
  await prepareAndLaunch(selectedInstance.value, { install, launch });
}
async function launch(i: Instance) {
  await action("启动游戏", "launch", { id: i.id }, true);
}
async function cancel() {
  try {
    await rpc("cancel");
    notify(
      challenge.value
        ? "正在取消登录…"
        : "正在取消，已校验的文件将保留在缓存中。",
    );
  } catch (e) {
    error.value = message(e);
  }
}
async function login() {
  if (busy.value) return;
  if (!desktop) {
    error.value = "请在桌面启动器中登录，浏览器预览不支持微软授权。";
    return;
  }
  if (!snapshot.value.database.settings.microsoft_client_id.trim()) {
    configureLogin();
    return;
  }
  busy.value = true;
  error.value = "";
  try {
    await signInWithMicrosoft({
      begin: () => rpc<Challenge>("begin_login"),
      showChallenge: (value) => {
        challenge.value = value;
      },
      openBrowser: (url) => openUrl(url),
      browserFailed: () => {
        error.value =
          "未能自动打开浏览器。请点击「打开登录页」重试，或复制下方登录地址到浏览器。";
      },
      waitForLogin: () => rpc("wait_login"),
    });
    challenge.value = null;
    error.value = "";
    await sync();
  } catch (e) {
    error.value = message(e);
    challenge.value = null;
  } finally {
    busy.value = false;
  }
}
function configureLogin() {
  loginClientId.value = snapshot.value.database.settings.microsoft_client_id;
  loginSetup.value = true;
  error.value = "";
}
async function saveLoginConfiguration() {
  const saved = await action<boolean>("配置登录应用", "save_settings", {
    settings: {
      ...snapshot.value.database.settings,
      microsoft_client_id: loginClientId.value.trim(),
    },
  });
  if (!saved) return;
  settings.microsoft_client_id =
    snapshot.value.database.settings.microsoft_client_id;
  loginSetup.value = false;
  await login();
}
async function chooseAccount(account: Account) {
  if (busy.value) return;
  if (
    account.id === snapshot.value.database.selected_account ||
    (await action<boolean>("切换账户", "select_account", { id: account.id }))
  ) {
    utility.value = null;
  }
}
async function saveSettings() {
  if (settingsSaving.value) return;
  const invalidField =
    !Number.isInteger(settings.concurrency) ||
    settings.concurrency < 1 ||
    settings.concurrency > 32
      ? { id: "settings-concurrency", message: "并发下载请填写 1–32 的整数。" }
      : !Number.isInteger(settings.retries) ||
          settings.retries < 0 ||
          settings.retries > 5
        ? { id: "settings-retries", message: "失败重试请填写 0–5 的整数。" }
        : null;
  if (invalidField) {
    settingsTab.value = "network";
    settingsInvalidFocus = invalidField.id;
    error.value = invalidField.message;
    await nextTick();
    focusSettingsField();
    return;
  }
  const session = utilitySession;
  const submitted = {
    ...settings,
    microsoft_client_id: snapshot.value.database.settings.microsoft_client_id,
    auto_java: snapshot.value.database.settings.auto_java,
  };
  settingsSaving.value = true;
  error.value = "";
  try {
    await rpc("save_settings", { settings: submitted });
    // Discard snapshots requested before this save; they can arrive after it.
    snapshotRequest++;
    snapshot.value.database.settings = { ...submitted };
    await sync();
    if (session === utilitySession && utility.value === "settings") {
      Object.assign(settings, submitted);
      utility.value = null;
    }
  } catch (e) {
    const text = message(e);
    if (session === utilitySession) error.value = text;
    logs.value.push({ time: stamp(), kind: "error", message: text });
  } finally {
    settingsSaving.value = false;
  }
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
async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    notify("已复制");
  } catch {
    error.value = "无法访问剪贴板，请手动选择并复制。";
  }
}
function onEvent(e: LauncherEvent) {
  if (
    current.value &&
    (!e.instance_id || e.instance_id === current.value.instance_id)
  ) {
    if (e.kind === "stage") {
      current.value.progress = 0;
      current.value.indeterminate = true;
    }
    if (e.kind === "progress" && e.total > 0) {
      current.value.progress = Math.min(
        100,
        Math.max(0, Math.round((e.completed / e.total) * 100)),
      );
      current.value.indeterminate = false;
    }
    if (["stage", "progress", "retry", "warning", "complete"].includes(e.kind))
      current.value.message = e.message;
  }
  if (!["transfer", "progress"].includes(e.kind)) {
    logs.value.push({ time: stamp(), kind: e.kind, message: e.message });
    if (logs.value.length > 800) logs.value.splice(0, logs.value.length - 800);
  }
  if (e.kind === "game_started" || e.kind === "game_exit") void sync();
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
  try {
    unlisten = await listen<LauncherEvent>("launcher-event", (e) =>
      onEvent(e.payload),
    );
    await sync();
    if (utility.value !== "settings")
      Object.assign(settings, snapshot.value.database.settings);
  } catch (e) {
    error.value = message(e);
  }
});
onUnmounted(() => {
  skinGeneration++;
  unlisten?.();
  document.removeEventListener("visibilitychange", updateVisibility);
});
</script>

<template>
  <div class="shell" :data-theme="theme">
    <WorldBackdrop :key="theme" :theme="theme" :paused="windowHidden" />
    <header class="topbar">
      <button
        class="account-card glass-surface"
        :aria-label="t('账户管理')"
        @click="toggleUtility('accounts')"
      >
        <span class="account-avatar"
          ><img v-if="avatar" :src="avatar" alt="" /><UsersRound
            v-else
            :size="25"
        /></span>
        <span class="account-identity">
          <strong>{{ selectedAccount?.username || "Player" }}</strong>
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
          <span v-else class="account-status"> {{ t("未登录") }} </span>
        </span>
        <ChevronDown class="account-chevron" :size="14" />
      </button>
      <div class="global-tools">
        <button
          class="icon-button"
          :title="t('全局设置')"
          :aria-label="t('全局设置')"
          @click="toggleUtility('settings')"
        >
          <Settings2 :size="20" />
        </button>
        <button
          class="icon-button"
          :title="t('运行日志')"
          :aria-label="t('运行日志')"
          @click="showLogs = true"
        >
          <Terminal :size="20" />
        </button>
      </div>
    </header>

    <main class="workspace" :aria-label="t('玩家皮肤预览')">
      <div
        v-if="error && !modal && !utility && !showLogs"
        class="error-banner workspace-error"
        role="alert"
      >
        <CircleAlert :size="16" />
        <p>{{ translateMessage(error) }}</p>
        <button
          class="icon-button"
          :aria-label="t('关闭错误提示')"
          @click="error = ''"
        >
          <X :size="16" />
        </button>
      </div>
      <SkinStage
        :texture="skinTexture"
        :slim="!!skinTexture && !!selectedAccount?.skin?.slim"
        :loading="skinLoading"
        :paused="!!modal || !!utility || showLogs"
        :caption="t(skinCaption)"
        :issue="translateMessage(skinError)"
        @avatar="avatar = $event"
        @failed="skinError = $event"
        @retry="loadAccountSkin"
      />
    </main>

    <footer class="bottom-bar">
      <ThemePicker :model-value="theme" @update:model-value="setTheme" />
      <div class="game-controls">
        <nav
          class="instance-dock"
          :class="{ empty: !instances.length, sorting: !!draggedInstanceId }"
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
            class="icon-button add-instance"
            :disabled="busy"
            :aria-label="t('新建实例')"
            :title="t('新建实例')"
            @click="newInstance()"
          >
            <Plus :size="20" />
          </button>
          <div
            ref="instanceStrip"
            v-show="instances.length"
            class="instance-strip"
            @scroll="hoveredInstance = null"
          >
            <TransitionGroup
              tag="div"
              name="instance-sort"
              class="instance-track"
            >
              <div
                v-for="instance in dockInstances"
                :key="instance.id"
                class="instance-item"
                :data-instance-id="instance.id"
                :class="{
                  dragging: draggedInstanceId === instance.id,
                }"
              >
                <button
                  class="instance-tile"
                  :class="{ selected: selectedInstance?.id === instance.id }"
                  :aria-label="
                    t(
                      '{name}，{version}，双击或按 F2 设置，拖动或按 Alt 加左右方向键排序',
                      {
                        name: instance.name,
                        version: instance.selected_version || t('尚无版本'),
                      },
                    )
                  "
                  :aria-pressed="selectedInstance?.id === instance.id"
                  aria-keyshortcuts="Alt+ArrowLeft Alt+ArrowRight F2"
                  @pointerdown="instanceSort.start($event, instance.id)"
                  @keydown="instanceSort.keyboard($event, instance.id)"
                  @click="selectInstance(instance.id)"
                  @dblclick="edit(instance)"
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
                    current.status === 'running'
                  "
                  :title="current.title"
                  :message="current.message"
                  :progress="current.progress"
                  :indeterminate="current.indeterminate"
                  @open="toggleUtility('downloads')"
                />
              </div>
            </TransitionGroup>
          </div>
        </nav>
        <div
          class="launch-control glass-surface"
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
                    : t('启动 {version}', { version: selectedVersion.id })
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
                        ? 'play'
                        : 'download'
                  "
                  class="launch-content"
                  :class="{ 'has-version': !!selectedVersion }"
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
                    /><Download v-else-if="!selectedVersion" :size="18" /><Play
                      v-else
                      :size="18"
                    />
                    <span class="launch-label">{{
                      selectedRunning
                        ? t("停止游戏")
                        : selectedVersion
                          ? t("启动游戏")
                          : t("下载游戏")
                    }}</span>
                  </span>
                  <Transition name="launch-version">
                    <small
                      v-if="selectedVersion"
                      :key="selectedVersion.id"
                      class="launch-version"
                      >{{ selectedVersion.id }}</small
                    >
                  </Transition>
                </span>
              </Transition>
            </span>
          </button>
          <Transition name="version-control">
            <VersionMenu
              v-if="selectedVersion"
              :instance="selectedInstance"
              :disabled="
                busy ||
                !selectedInstance ||
                selectedRunning ||
                !!modal ||
                !!utility ||
                showLogs
              "
              @select="
                selectedInstance && selectVersion(selectedInstance, $event)
              "
              @download="selectedInstance && newVersion(selectedInstance)"
            />
          </Transition>
        </div>
      </div>
    </footer>
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
        :label="t(utilityTitles[utility])"
        @close="utility = null"
      >
        <section
          class="utility-panel glass-surface"
          :class="{
            'account-panel': utility === 'accounts',
            'global-settings': utility === 'settings',
          }"
        >
          <header class="utility-panel-header">
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
            <h2 v-else class="dialog-heading" tabindex="-1" autofocus>
              {{ t(utilityTitles[utility]) }}
            </h2>
            <div class="inline-actions">
              <button
                v-if="
                  utility === 'accounts' &&
                  selectedAccount &&
                  !loginSetup &&
                  !challenge
                "
                class="icon-button"
                :disabled="busy"
                :aria-label="t('刷新账户与皮肤')"
                :title="t('刷新账户与皮肤')"
                @click="action('刷新账户', 'refresh_account')"
              >
                <RefreshCw :size="18" />
              </button>
              <button
                v-if="utility === 'downloads'"
                class="button"
                @click="
                  showLogs = true;
                  utility = null;
                "
              >
                <Terminal :size="16" /> {{ t("日志") }}
              </button>
              <button
                class="icon-button"
                :aria-label="t('关闭面板')"
                @click="utility = null"
              >
                <X :size="16" />
              </button>
            </div>
          </header>
          <div v-if="error && !modal" class="error-banner" role="alert">
            <CircleAlert :size="16" />
            <p>{{ translateMessage(error) }}</p>
            <button
              class="icon-button"
              :aria-label="t('关闭错误提示')"
              @click="error = ''"
            >
              <X :size="16" />
            </button>
          </div>
          <div class="utility-panel-content">
            <div v-if="utility === 'accounts'" class="page-inner">
              <form
                v-if="loginSetup"
                class="login-setup"
                @submit.prevent="saveLoginConfiguration"
              >
                <p class="login-explanation">
                  {{
                    t(
                      "此开发版本还需要配置微软登录应用。Client ID 是启动器的公开应用编号，由开发者注册提供。",
                    )
                  }}
                </p>
                <label class="field">
                  {{ t("应用 ID（Client ID）") }}
                  <input
                    v-model="loginClientId"
                    required
                    :disabled="busy"
                    placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                    spellcheck="false"
                    autocomplete="off"
                  />
                </label>
                <div class="login-setup-actions">
                  <button
                    type="button"
                    class="text-button"
                    @click="
                      openLink(
                        'https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code',
                      )
                    "
                  >
                    {{ t("注册说明") }} <ExternalLink :size="14" />
                  </button>
                  <span class="grow" />
                  <button
                    type="button"
                    class="button"
                    :disabled="busy"
                    @click="loginSetup = false"
                  >
                    {{ t("取消") }}
                  </button>
                  <button
                    type="submit"
                    class="button primary"
                    :disabled="busy || !loginClientId.trim()"
                  >
                    {{ t("保存并登录") }}
                  </button>
                </div>
              </form>
              <div v-else-if="challenge" class="panel login-panel">
                <h3>{{ t("完成微软授权") }}</h3>
                <p>
                  {{
                    t("在微软页面输入验证码，有效期约 {minutes} 分钟。", {
                      minutes: Math.ceil(challenge.expires_in / 60),
                    })
                  }}
                </p>
                <button
                  class="device-code"
                  :title="t('复制验证码')"
                  @click="copy(challenge.user_code)"
                >
                  {{ challenge.user_code }}<Copy :size="16" />
                </button>
                <div class="inline-actions">
                  <button
                    class="button primary"
                    @click="openLink(challenge.verification_uri)"
                  >
                    {{ t("打开登录页") }} <ExternalLink :size="16" /></button
                  ><button class="button" @click="cancel">
                    {{ t("取消登录") }}
                  </button>
                </div>
                <button
                  class="text-button login-address"
                  :title="t('复制登录地址')"
                  @click="copy(challenge.verification_uri)"
                >
                  {{ challenge.verification_uri }} <Copy :size="14" />
                </button>
                <p class="waiting">
                  <LoaderCircle :size="16" class="spin" /> {{ t("等待授权…") }}
                </p>
              </div>
              <template v-else>
                <div
                  class="account-picker"
                  role="group"
                  :aria-label="t('选择启动账户')"
                >
                  <div
                    v-for="account in snapshot.database.accounts"
                    :key="account.id"
                    class="account-choice"
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
                      :title="account.username"
                      @click="chooseAccount(account)"
                    >
                      <span class="account-avatar-frame">
                        <AccountAvatar
                          :account="account"
                          :preview="
                            account.id === selectedAccount?.id
                              ? avatar
                              : undefined
                          "
                          :load-texture="fetchAccountSkin"
                        />
                        <span
                          v-if="
                            account.id === snapshot.database.selected_account
                          "
                          class="account-selected-mark"
                          :aria-label="t('当前账户')"
                          ><Check :size="12"
                        /></span>
                      </span>
                      <span class="account-name">{{ account.username }}</span>
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
                    class="account-add"
                    :disabled="busy"
                    :aria-label="t('添加微软账户')"
                    :title="t('添加微软账户')"
                    @click="login"
                  >
                    <span class="account-add-icon"><Plus :size="24" /></span>
                  </button>
                </div>
                <button
                  v-if="error"
                  class="text-button"
                  :disabled="busy"
                  @click="configureLogin"
                >
                  {{ t("配置登录应用") }}
                </button>
              </template>
            </div>

            <div v-if="utility === 'downloads'" class="page-inner">
              <div class="section-heading">
                <h2>
                  {{ t("任务") }} <span>{{ tasks.length }}</span>
                </h2>
                <button class="text-button" @click="utility = 'settings'">
                  {{ t("下载设置") }} <ChevronRight :size="16" />
                </button>
              </div>
              <div v-if="!tasks.length" class="empty-state compact">
                <p>{{ t("暂无下载任务") }}</p>
              </div>
              <div v-for="task in tasks" :key="task.id" class="panel task-row">
                <span :class="['task-icon', task.status]"
                  ><LoaderCircle
                    v-if="task.status === 'running'"
                    class="spin"
                    :size="16" /><Check
                    v-else-if="task.status === 'done'"
                    :size="16" /><CircleAlert v-else :size="16"
                /></span>
                <div class="grow">
                  <div class="task-title">
                    <strong>{{ t(task.title) }}</strong
                    ><time>{{ task.started }}</time>
                  </div>
                  <p>{{ translateMessage(task.message) }}</p>
                  <div
                    v-if="task.status === 'running'"
                    :class="[
                      'progress-track',
                      { indeterminate: task.indeterminate },
                    ]"
                    role="progressbar"
                    :aria-valuenow="
                      task.indeterminate ? undefined : task.progress
                    "
                    aria-valuemin="0"
                    aria-valuemax="100"
                    :aria-label="t(task.title)"
                  >
                    <i
                      :style="
                        task.indeterminate ? {} : { width: `${task.progress}%` }
                      "
                    />
                  </div>
                </div>
                <button
                  v-if="task.status === 'running' && task.cancellable"
                  class="button"
                  @click="cancel"
                >
                  {{ t("取消") }}</button
                ><span v-else class="task-state">{{
                  {
                    done: t("已完成"),
                    error: t("失败"),
                    cancelled: t("已取消"),
                    running: t("进行中"),
                  }[task.status]
                }}</span>
              </div>
            </div>

            <form
              v-if="utility === 'settings'"
              id="global-settings-form"
              class="page-inner settings-form"
              novalidate
              @submit.prevent="saveSettings"
            >
              <div class="settings-pages">
                <Transition
                  name="settings-page"
                  mode="out-in"
                  @after-enter="focusSettingsField"
                >
                  <div
                    v-if="settingsTab === 'general'"
                    key="general"
                    id="settings-panel-general"
                    class="settings-page"
                    role="tabpanel"
                    aria-labelledby="settings-tab-general"
                  >
                    <div class="settings-language">
                      <div class="field">
                        <label id="language-label" for="language-trigger">{{
                          t("语言")
                        }}</label>
                        <GlassSelect
                          id="language"
                          v-model="settings.language"
                          :options="languages"
                          :disabled="settingsSaving"
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
                          :disabled="settingsSaving"
                        />
                      </div>
                    </div>
                    <div class="settings-preference settings-update">
                      <div class="field settings-version">
                        <span>{{ t("启动器版本") }}</span>
                        <strong>v{{ launcherVersion }}</strong>
                      </div>
                      <span
                        v-if="updateChecking"
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
                        @click="openLink(updateStatus.download_url)"
                      >
                        {{
                          t("最新版为 {version} · 点击更新", {
                            version: updateStatus.latest_version,
                          })
                        }}
                      </button>
                    </div>
                    <button
                      type="button"
                      class="settings-directory"
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
                    v-else
                    key="network"
                    id="settings-panel-network"
                    class="settings-page settings-fields"
                    role="tabpanel"
                    aria-labelledby="settings-tab-network"
                  >
                    <div class="field settings-source-field">
                      <label
                        id="download-source-label"
                        for="download-source-trigger"
                      >
                        {{ t("下载源") }}
                      </label>
                      <DownloadSourceSelect
                        v-model="settings.download_source"
                        :disabled="settingsSaving"
                      />
                    </div>
                    <label class="field">
                      {{ t("并发下载") }}
                      <input
                        id="settings-concurrency"
                        v-model.number="settings.concurrency"
                        type="number"
                        min="1"
                        max="32"
                        step="1"
                        required
                        :disabled="settingsSaving"
                      />
                    </label>
                    <label class="field">
                      {{ t("失败重试") }}
                      <input
                        id="settings-retries"
                        v-model.number="settings.retries"
                        type="number"
                        min="0"
                        max="5"
                        step="1"
                        required
                        :disabled="settingsSaving"
                      />
                    </label>
                  </div>
                </Transition>
              </div>
            </form>
          </div>
          <footer v-if="utility === 'settings'" class="modal-actions">
            <button type="button" class="button" @click="utility = null">
              {{ t("取消") }}
            </button>
            <button
              type="submit"
              form="global-settings-form"
              class="button primary"
              :disabled="settingsSaving"
            >
              {{ t("保存") }}
            </button>
          </footer>
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
        <section class="log-panel glass-surface" :aria-label="t('运行日志')">
          <header>
            <h2 class="dialog-heading" tabindex="-1" autofocus>
              <Terminal :size="16" /> {{ t("运行日志") }}
              <small>{{ t("{count} 条", { count: logs.length }) }} </small>
            </h2>
            <div class="inline-actions">
              <button
                class="icon-button"
                :aria-label="t('复制日志')"
                @click="
                  copy(logs.map((l) => `[${l.time}] ${l.message}`).join('\n'))
                "
              >
                <Copy :size="16" /></button
              ><button
                class="icon-button"
                :aria-label="t('清空日志')"
                @click="logs = []"
              >
                <Trash2 :size="16" /></button
              ><button
                class="icon-button"
                :aria-label="t('关闭日志')"
                @click="showLogs = false"
              >
                <X :size="16" />
              </button>
            </div>
          </header>
          <div class="log-content">
            <p v-if="!logs.length" class="muted">{{ t("暂无运行日志。") }}</p>
            <p
              v-for="(line, index) in logs"
              :key="index"
              :class="{ error: line.kind === 'error' }"
            >
              <time>{{ line.time }}</time
              ><span>{{
                line.kind === "log"
                  ? line.message
                  : translateMessage(line.message)
              }}</span>
            </p>
          </div>
        </section>
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
        :locked="busy"
        @close="modal = null"
      >
        <section
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
              :disabled="busy"
              @open-directory="editing && openFolder(editing.id)"
            />
            <h2 v-else class="dialog-heading" tabindex="-1" autofocus>
              {{ t(modalTitles[modal]) }}
            </h2>
            <button
              class="icon-button"
              :disabled="busy"
              :aria-label="t('关闭对话框')"
              @click="modal = null"
            >
              <X :size="16" />
            </button>
          </div>
          <div v-if="error" class="error-banner" role="alert">
            <CircleAlert :size="16" />
            <p>{{ translateMessage(error) }}</p>
          </div>
          <template v-if="modal === 'delete'"
            ><p class="delete-message">
              {{
                editing?.game_directory
                  ? t("仅从列表移除「{name}」，原目录和游戏文件保留在原处。", {
                      name: editing.name,
                    })
                  : t("「{name}」将移至回收目录，游戏文件会保留。", {
                      name: editing?.name || "",
                    })
              }}
            </p>
            <div class="modal-actions">
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
              <div v-if="modal === 'version'" class="download-destination">
                <span>
                  {{ t("安装到") }} <strong>{{ editing?.name }}</strong></span
                >
                <button
                  type="button"
                  class="icon-button"
                  :disabled="busy"
                  :aria-label="t('更新版本列表')"
                  :title="t('更新版本列表')"
                  @click="loadVersions"
                >
                  <RefreshCw :size="18" />
                </button>
              </div>
              <label
                v-if="modal === 'create' || modal === 'import'"
                class="field"
              >
                {{ t("实例名称") }}
                <input
                  v-model="form.name"
                  :placeholder="t('例如：日常游戏')"
                  required
                  maxlength="80" /></label
              ><label v-if="modal === 'version'" class="field">
                {{ t("Minecraft 版本") }}
                <input
                  v-model="form.version"
                  list="versions"
                  placeholder="1.21.1"
                  required
                /><datalist id="versions">
                  <option
                    v-for="version in manifest?.versions"
                    :key="version.id"
                    :value="version.id"
                  >
                    {{ version.type }}
                  </option></datalist
                ><small v-if="modal === 'version'">
                  {{ t("支持原版 Java 1.13+，Apple Silicon 使用 1.19+。") }}
                </small></label
              >
              <label v-if="modal === 'import'" class="field">
                {{ t("游戏目录") }}
                <div class="input-action">
                  <input
                    v-model="form.source"
                    required
                    :placeholder="t('~/.minecraft 或游戏目录')"
                  /><button
                    type="button"
                    class="button"
                    :disabled="directoryPicking || busy"
                    @click="chooseDirectory"
                  >
                    <FolderOpen :size="16" /> {{ t("浏览") }}
                  </button>
                </div>
                <small>
                  {{ t("直接使用原目录，不复制游戏文件。") }}
                </small></label
              >
              <template v-if="modal === 'edit'"
                ><label class="field">
                  {{ t("最大内存") }}
                  <div class="memory-input">
                    <input
                      v-model.number="form.memory_mb"
                      type="number"
                      min="512"
                      max="65536"
                      step="256"
                      required
                    /><span>MiB</span>
                  </div></label
                ><label class="field instance-java">
                  {{ t("Java 可执行文件") }}
                  <div class="input-action">
                    <input
                      v-model="form.java_path"
                      :placeholder="t('留空自动匹配')"
                    /><button
                      type="button"
                      class="icon-button"
                      :disabled="busy"
                      :aria-label="t('选择 Java 可执行文件')"
                      :title="t('选择 Java 可执行文件')"
                      @click="chooseJava"
                    >
                      <FolderOpen :size="18" />
                    </button></div
                ></label>
              </template>
            </div>
            <div class="modal-actions">
              <div v-if="modal === 'edit'" class="instance-maintenance-actions">
                <button
                  type="button"
                  class="text-button"
                  :disabled="busy || !editing?.selected_version"
                  @click="editing && install(editing)"
                >
                  <RefreshCw :size="16" /> {{ t("校验修复") }}
                </button>
                <button
                  type="button"
                  class="text-button danger"
                  :disabled="busy"
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
                @click="newInstance(true)"
              >
                {{ t("导入已有目录") }}
              </button>
              <button
                type="button"
                class="button"
                :disabled="busy"
                @click="modal = null"
              >
                {{ t("取消") }}</button
              ><button type="submit" class="button primary" :disabled="busy">
                <LoaderCircle v-if="busy" class="spin" :size="16" />{{
                  modal === "edit"
                    ? t("保存")
                    : modal === "import"
                      ? t("导入")
                      : modal === "version"
                        ? t("下载并安装")
                        : t("创建实例")
                }}
              </button>
            </div>
          </form>
        </section>
      </GlassDialog>
    </Transition>
  </div>
</template>
