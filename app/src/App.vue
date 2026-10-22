<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Plus,
  Settings2,
  UsersRound,
  FolderOpen,
  Play,
  Square,
  RefreshCw,
  Search,
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
import { prepareAndLaunch } from "./launch";
import { signInWithMicrosoft } from "./login";
import GlassDialog from "./components/GlassDialog.vue";
import SkinStage from "./components/SkinStage.vue";
import WorldBackdrop from "./components/WorldBackdrop.vue";
import VersionMenu from "./components/VersionMenu.vue";
import AccountAvatar from "./components/AccountAvatar.vue";
import InstanceHeader from "./components/InstanceHeader.vue";
import DownloadSourceSelect from "./components/DownloadSourceSelect.vue";
import type {
  Settings,
  Instance,
  Snapshot,
  VersionManifest,
  LauncherEvent,
  Challenge,
  Task,
  Account,
} from "./types";

const desktop = isTauri();
const utility = ref<"accounts" | "downloads" | "settings" | null>(null);
const snapshot = ref<Snapshot>({
  root: "",
  database: {
    settings: {
      microsoft_client_id: "",
      download_source: "official",
      concurrency: 8,
      retries: 2,
      auto_java: true,
    },
    instances: [],
    accounts: [],
    selected_account: null,
  },
  running: [],
});
const settings = reactive<Settings>({ ...snapshot.value.database.settings });
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
const javaList = ref<{ path: string; major: number; version: string }[]>([]);
const themes = backdropThemes;
const theme = ref("tide");
const hoveredInstance = ref<Instance | null>(null);
const instanceTipPosition = ref({ left: "0px", top: "0px" });
function previewInstance(event: MouseEvent | FocusEvent, instance: Instance) {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  hoveredInstance.value = instance;
  instanceTipPosition.value = {
    left: `${box.left + box.width / 2}px`,
    top: `${box.bottom + 8}px`,
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
    error.value = "";
  }
});
function toggleUtility(panel: "accounts" | "downloads" | "settings") {
  showLogs.value = false;
  modal.value = null;
  if (panel === "accounts") loginSetup.value = false;
  utility.value = utility.value === panel ? null : panel;
}
function instanceMaterial(instance: Instance) {
  if (
    Number.isInteger(instance.icon) &&
    instance.icon! >= 0 &&
    instance.icon! < 4
  )
    return instance.icon!;
  return (
    (Number.parseInt(instance.id.replaceAll("-", "").slice(0, 8), 16) || 0) % 4
  );
}
function selectInstance(id: string) {
  selectedInstanceId.value = id;
  try {
    localStorage.setItem("nekocrate.selected-instance", id);
  } catch {
    /* Storage may be unavailable in a restricted webview. */
  }
}
const stamp = () => new Date().toLocaleTimeString("zh-CN", { hour12: false });
async function rpc<T = unknown>(
  op: string,
  data: Record<string, unknown> = {},
): Promise<T> {
  if (!desktop)
    throw new Error("当前为界面预览。请运行桌面版以管理实例和启动游戏。");
  return invoke<T>("command", { request: { op, ...data } });
}
async function sync() {
  const response = await rpc<{
    state: Snapshot;
    manifest: VersionManifest | null;
    busy: boolean;
  }>("snapshot");
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
      title: "选择 bin/java 或 bin/java.exe",
    });
    if (typeof path === "string") form.java_path = path;
  } catch (e) {
    error.value = message(e);
  }
}
async function submit() {
  if (modal.value === "edit" && instanceHeader.value?.finishRename() === false)
    return;
  let result: Instance | boolean | undefined;
  if (modal.value === "create")
    result = await action<Instance>("创建实例", "create", {
      name: form.name,
    });
  else if (modal.value === "import")
    result = await action<Instance>(
      "迁移实例",
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
      modal.value = null;
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
    modal.value = null;
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
  const result = await action("保存设置", "save_settings", {
    settings: {
      ...settings,
      microsoft_client_id: snapshot.value.database.settings.microsoft_client_id,
    },
  });
  if (result) {
    Object.assign(settings, snapshot.value.database.settings);
    if (utility.value === "settings") utility.value = null;
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
async function inspectJava() {
  const result = await action<typeof javaList.value>("检测 Java", "java");
  if (result) javaList.value = result;
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
        aria-label="账户管理"
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
                ? "已登录"
                : "待刷新"
            }}</span
          >
          <span v-else class="account-status">未登录</span>
        </span>
        <ChevronDown class="account-chevron" :size="14" />
      </button>
      <div class="global-tools">
        <button
          class="icon-button"
          title="全局设置"
          aria-label="全局设置"
          @click="toggleUtility('settings')"
        >
          <Settings2 :size="20" />
        </button>
        <button
          class="icon-button"
          title="运行日志"
          aria-label="运行日志"
          @click="showLogs = true"
        >
          <Terminal :size="20" />
        </button>
      </div>
    </header>

    <main class="workspace" aria-label="玩家皮肤预览">
      <div
        v-if="error && !modal && !utility && !showLogs"
        class="error-banner workspace-error"
        role="alert"
      >
        <CircleAlert :size="16" />
        <p>{{ error }}</p>
        <button
          class="icon-button"
          aria-label="关闭错误提示"
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
        :caption="skinCaption"
        :issue="skinError"
        @avatar="avatar = $event"
        @failed="skinError = $event"
        @retry="loadAccountSkin"
      />
    </main>

    <footer class="bottom-bar">
      <div class="theme-picker" role="group" aria-label="主题配色">
        <button
          v-for="color in themes"
          :key="color.id"
          class="theme-swatch"
          :class="{ active: theme === color.id }"
          :style="{ '--swatch': color.color }"
          :aria-label="`${color.name}主题，${color.pattern}背景`"
          :aria-pressed="theme === color.id"
          :title="`${color.name} · ${color.pattern}`"
          @click="setTheme(color.id)"
        >
          <svg
            viewBox="0 0 12 14"
            width="14"
            height="16"
            fill="none"
            stroke="currentColor"
            aria-hidden="true"
          >
            <path :d="color.mark" />
          </svg>
        </button>
      </div>
      <div class="game-controls">
        <nav
          class="instance-dock"
          :class="{ empty: !instances.length }"
          aria-label="实例"
        >
          <button
            class="icon-button add-instance"
            :disabled="busy"
            aria-label="新建实例"
            title="新建实例"
            @click="newInstance()"
          >
            <Plus :size="20" />
          </button>
          <div class="instance-strip" @scroll="hoveredInstance = null">
            <div
              v-for="instance in instances"
              :key="instance.id"
              class="instance-item"
            >
              <button
                class="instance-tile"
                :class="{ selected: selectedInstance?.id === instance.id }"
                :aria-label="`${instance.name}，${instance.selected_version || '尚无版本'}，双击或按 F2 设置`"
                :aria-pressed="selectedInstance?.id === instance.id"
                @click="selectInstance(instance.id)"
                @dblclick="edit(instance)"
                @contextmenu.prevent="edit(instance)"
                @keydown.f2.prevent="edit(instance)"
                @mouseenter="previewInstance($event, instance)"
                @mouseleave="hoveredInstance = null"
                @focus="previewInstance($event, instance)"
                @blur="hoveredInstance = null"
              >
                <span
                  class="instance-art"
                  :data-material="instanceMaterial(instance)"
                  aria-hidden="true"
                />
                <i
                  v-if="snapshot.running.includes(instance.id)"
                  class="running-dot"
                />
              </button>
              <button
                v-if="
                  current?.instance_id === instance.id &&
                  current.status === 'running'
                "
                class="instance-progress-button"
                :title="current.message"
                aria-label="查看此实例下载进度"
                @click="toggleUtility('downloads')"
              >
                <span
                  class="instance-progress"
                  :class="{ indeterminate: current.indeterminate }"
                  role="progressbar"
                  :aria-valuenow="
                    current.indeterminate ? undefined : current.progress
                  "
                  aria-valuemin="0"
                  aria-valuemax="100"
                  :aria-label="current.title"
                  ><i
                    :style="
                      current.indeterminate
                        ? {}
                        : { width: `${current.progress}%` }
                    "
                /></span>
              </button>
            </div>
          </div>
        </nav>
        <div
          class="launch-control glass-surface"
          :class="{ running: selectedRunning }"
        >
          <button
            class="launch-button"
            :disabled="busy || !selectedVersion"
            :title="
              !selectedInstance
                ? '请先新建实例'
                : !selectedVersion
                  ? '请先在右侧版本菜单中下载版本'
                  : selectedRunning
                    ? '停止游戏'
                    : `启动 ${selectedVersion.id}`
            "
            @click="
              selectedRunning
                ? action('停止游戏', 'stop', { id: selectedInstance?.id })
                : startGame()
            "
          >
            <LoaderCircle
              v-if="busy && current?.instance_id === selectedInstance?.id"
              class="spin"
              :size="18"
            /><Square
              v-else-if="selectedRunning"
              class="stop-icon"
              :size="16"
            /><Play v-else :size="18" />
            <span>{{ selectedRunning ? "停止游戏" : "启动游戏" }}</span>
          </button>
          <VersionMenu
            :key="selectedInstance?.id || 'none'"
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
        </div>
      </div>
    </footer>
    <div
      v-if="hoveredInstance && !modal && !utility && !showLogs"
      class="instance-tooltip"
      role="tooltip"
      :style="instanceTipPosition"
    >
      {{ hoveredInstance.name }}
    </div>

    <GlassDialog
      :notice="notice"
      v-if="utility"
      :label="utilityTitles[utility]"
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
            class="settings-heading dialog-heading"
            tabindex="-1"
            autofocus
          >
            <span class="settings-symbol" aria-hidden="true"
              ><Settings2 :size="28"
            /></span>
            <div class="settings-heading-details">
              <h2>全局设置</h2>
              <button
                class="settings-directory-link"
                :disabled="!snapshot.root"
                :title="snapshot.root"
                aria-label="打开启动器数据目录"
                @click="openFolder()"
              >
                <FolderOpen :size="13" /><span>{{
                  snapshot.root || "使用系统应用数据目录"
                }}</span>
              </button>
            </div>
          </div>
          <h2 v-else class="dialog-heading" tabindex="-1" autofocus>
            {{ utilityTitles[utility] }}
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
              aria-label="刷新账户与皮肤"
              title="刷新账户与皮肤"
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
              <Terminal :size="16" />日志
            </button>
            <button
              class="icon-button"
              aria-label="关闭面板"
              @click="utility = null"
            >
              <X :size="16" />
            </button>
          </div>
        </header>
        <div v-if="error && !modal" class="error-banner" role="alert">
          <CircleAlert :size="16" />
          <p>{{ error }}</p>
          <button
            class="icon-button"
            aria-label="关闭错误提示"
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
                此开发版本还需要配置微软登录应用。Client ID
                是启动器的公开应用编号，由开发者注册提供。
              </p>
              <label class="field"
                >应用 ID（Client ID）
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
                  注册说明 <ExternalLink :size="14" />
                </button>
                <span class="grow" />
                <button
                  type="button"
                  class="button"
                  :disabled="busy"
                  @click="loginSetup = false"
                >
                  取消
                </button>
                <button
                  type="submit"
                  class="button primary"
                  :disabled="busy || !loginClientId.trim()"
                >
                  保存并登录
                </button>
              </div>
            </form>
            <div v-else-if="challenge" class="panel login-panel">
              <h3>完成微软授权</h3>
              <p>
                在微软页面输入验证码，有效期约
                {{ Math.ceil(challenge.expires_in / 60) }} 分钟。
              </p>
              <button
                class="device-code"
                title="复制验证码"
                @click="copy(challenge.user_code)"
              >
                {{ challenge.user_code }}<Copy :size="16" />
              </button>
              <div class="inline-actions">
                <button
                  class="button primary"
                  @click="openLink(challenge.verification_uri)"
                >
                  打开登录页 <ExternalLink :size="16" /></button
                ><button class="button" @click="cancel">取消登录</button>
              </div>
              <button
                class="text-button login-address"
                title="复制登录地址"
                @click="copy(challenge.verification_uri)"
              >
                {{ challenge.verification_uri }} <Copy :size="14" />
              </button>
              <p class="waiting">
                <LoaderCircle :size="16" class="spin" />等待授权…
              </p>
            </div>
            <template v-else>
              <div
                class="account-picker"
                role="group"
                aria-label="选择启动账户"
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
                    :aria-label="`使用 ${account.username} 启动游戏`"
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
                        v-if="account.id === snapshot.database.selected_account"
                        class="account-selected-mark"
                        aria-label="当前账户"
                        ><Check :size="12"
                      /></span>
                    </span>
                    <span class="account-name">{{ account.username }}</span>
                  </button>
                  <button
                    class="icon-button account-remove"
                    :disabled="busy"
                    :aria-label="`移除账户 ${account.username}`"
                    title="移除账户"
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
                  aria-label="添加微软账户"
                  title="添加微软账户"
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
                配置登录应用
              </button>
            </template>
          </div>

          <div v-if="utility === 'downloads'" class="page-inner">
            <div class="section-heading">
              <h2>
                任务 <span>{{ tasks.length }}</span>
              </h2>
              <button class="text-button" @click="utility = 'settings'">
                下载设置 <ChevronRight :size="16" />
              </button>
            </div>
            <div v-if="!tasks.length" class="empty-state compact">
              <p>暂无下载任务</p>
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
                  <strong>{{ task.title }}</strong
                  ><time>{{ task.started }}</time>
                </div>
                <p>{{ task.message }}</p>
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
                  :aria-label="task.title"
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
                取消</button
              ><span v-else class="task-state">{{
                {
                  done: "已完成",
                  error: "失败",
                  cancelled: "已取消",
                  running: "进行中",
                }[task.status]
              }}</span>
            </div>
          </div>

          <form
            v-if="utility === 'settings'"
            id="global-settings-form"
            class="page-inner settings-form"
            @submit.prevent="saveSettings"
          >
            <div class="settings-fields">
              <div class="field settings-source-field">
                <label id="download-source-label" for="download-source-trigger"
                  >下载源</label
                >
                <DownloadSourceSelect
                  v-model="settings.download_source"
                  :disabled="busy"
                />
              </div>
              <label class="field"
                >并发下载
                <input
                  v-model.number="settings.concurrency"
                  type="number"
                  min="1"
                  max="32"
                  step="1"
                  required
                  :disabled="busy"
                />
              </label>
              <label class="field"
                >失败重试
                <input
                  v-model.number="settings.retries"
                  type="number"
                  min="0"
                  max="5"
                  step="1"
                  required
                  :disabled="busy"
                />
              </label>
            </div>
            <label
              class="toggle-row settings-java-toggle"
              title="缺少 Java 时，通过 Eclipse Adoptium 下载并校验"
            >
              <span>自动安装 Java</span>
              <input
                v-model="settings.auto_java"
                type="checkbox"
                role="switch"
                :disabled="busy"
              />
            </label>
            <div
              v-if="javaList.length"
              class="settings-java-list"
              aria-label="检测到的 Java"
            >
              <div v-for="java in javaList" :key="java.path" class="java-row">
                <strong>Java {{ java.major }}</strong
                ><code :title="java.path">{{ java.path }}</code>
              </div>
            </div>
          </form>
        </div>
        <footer v-if="utility === 'settings'" class="modal-actions">
          <div class="settings-maintenance-actions">
            <button
              type="button"
              class="text-button"
              :disabled="busy"
              @click="inspectJava"
            >
              <Search :size="16" />检测 Java
            </button>
            <button
              type="button"
              class="text-button"
              @click="utility = 'downloads'"
            >
              下载记录
            </button>
          </div>
          <button type="button" class="button" @click="utility = null">
            取消
          </button>
          <button
            type="submit"
            form="global-settings-form"
            class="button primary"
            :disabled="busy"
          >
            保存
          </button>
        </footer>
      </section>
    </GlassDialog>
    <GlassDialog
      :notice="notice"
      v-if="showLogs"
      label="运行日志"
      @close="showLogs = false"
    >
      <section class="log-panel glass-surface" aria-label="运行日志">
        <header>
          <h2 class="dialog-heading" tabindex="-1" autofocus>
            <Terminal :size="16" />运行日志 <small>{{ logs.length }} 条</small>
          </h2>
          <div class="inline-actions">
            <button
              class="icon-button"
              aria-label="复制日志"
              @click="
                copy(logs.map((l) => `[${l.time}] ${l.message}`).join('\n'))
              "
            >
              <Copy :size="16" /></button
            ><button
              class="icon-button"
              aria-label="清空日志"
              @click="logs = []"
            >
              <Trash2 :size="16" /></button
            ><button
              class="icon-button"
              aria-label="关闭日志"
              @click="showLogs = false"
            >
              <X :size="16" />
            </button>
          </div>
        </header>
        <div class="log-content">
          <p v-if="!logs.length" class="muted">暂无运行日志。</p>
          <p
            v-for="(line, index) in logs"
            :key="index"
            :class="{ error: line.kind === 'error' }"
          >
            <time>{{ line.time }}</time
            ><span>{{ line.message }}</span>
          </p>
        </div>
      </section>
    </GlassDialog>

    <Transition name="toast"
      ><div v-if="notice" class="toast glass-surface" role="status">
        <CheckCircle2 :size="16" />{{ notice }}
      </div></Transition
    >
    <GlassDialog
      :notice="notice"
      v-if="modal"
      :label="modal === 'edit' ? form.name : modalTitles[modal]"
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
            {{ modalTitles[modal] }}
          </h2>
          <button
            class="icon-button"
            :disabled="busy"
            aria-label="关闭对话框"
            @click="modal = null"
          >
            <X :size="16" />
          </button>
        </div>
        <div v-if="error" class="error-banner" role="alert">
          <CircleAlert :size="16" />
          <p>{{ error }}</p>
        </div>
        <template v-if="modal === 'delete'"
          ><p class="delete-message">
            「{{ editing?.name }}」将移至回收目录，游戏文件会保留。
          </p>
          <div class="modal-actions">
            <button class="button" @click="modal = 'edit'">取消</button
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
              移除实例
            </button>
          </div></template
        >
        <form v-else @submit.prevent="submit">
          <div class="modal-fields">
            <div v-if="modal === 'version'" class="download-destination">
              <span
                >安装到 <strong>{{ editing?.name }}</strong></span
              >
              <button
                type="button"
                class="icon-button"
                :disabled="busy"
                aria-label="更新版本列表"
                title="更新版本列表"
                @click="loadVersions"
              >
                <RefreshCw :size="18" />
              </button>
            </div>
            <label v-if="modal === 'create' || modal === 'import'" class="field"
              >实例名称<input
                v-model="form.name"
                placeholder="例如：日常游戏"
                required
                maxlength="80" /></label
            ><label v-if="modal === 'version'" class="field"
              >Minecraft 版本<input
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
              ><small v-if="modal === 'version'"
                >支持原版 Java 1.13+，Apple Silicon 使用 1.19+。</small
              ></label
            >
            <label v-if="modal === 'import'" class="field"
              >来源目录
              <div class="input-action">
                <input
                  v-model="form.source"
                  required
                  placeholder="~/.minecraft 或游戏目录"
                /><button
                  type="button"
                  class="button"
                  :disabled="directoryPicking || busy"
                  @click="chooseDirectory"
                >
                  <FolderOpen :size="16" />浏览
                </button>
              </div>
              <small>保留源目录，仅复制游戏文件。原型不加载模组。</small></label
            >
            <template v-if="modal === 'edit'"
              ><label class="field"
                >最大内存
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
              ><label class="field instance-java"
                >Java 可执行文件
                <div class="input-action">
                  <input
                    v-model="form.java_path"
                    placeholder="留空自动匹配"
                  /><button
                    type="button"
                    class="icon-button"
                    :disabled="busy"
                    aria-label="选择 Java 可执行文件"
                    title="选择 Java 可执行文件"
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
                <RefreshCw :size="16" />校验修复
              </button>
              <button
                type="button"
                class="text-button danger"
                :disabled="busy"
                @click="modal = 'delete'"
              >
                <Trash2 :size="16" />移除实例
              </button>
            </div>
            <button
              v-if="modal === 'create'"
              type="button"
              class="text-button import-link"
              :disabled="busy"
              @click="newInstance(true)"
            >
              导入已有目录
            </button>
            <button
              type="button"
              class="button"
              :disabled="busy"
              @click="modal = null"
            >
              取消</button
            ><button type="submit" class="button primary" :disabled="busy">
              <LoaderCircle v-if="busy" class="spin" :size="16" />{{
                modal === "edit"
                  ? "保存"
                  : modal === "import"
                    ? "导入"
                    : modal === "version"
                      ? "下载并安装"
                      : "创建实例"
              }}
            </button>
          </div>
        </form>
      </section>
    </GlassDialog>
  </div>
</template>
