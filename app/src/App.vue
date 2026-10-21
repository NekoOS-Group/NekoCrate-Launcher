<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Box,
  Boxes,
  Plus,
  Download,
  Settings2,
  UsersRound,
  ArrowDownToLine,
  FolderOpen,
  Play,
  Square,
  RefreshCw,
  Search,
  ChevronRight,
  Terminal,
  X,
  Check,
  Copy,
  Trash2,
  ShieldCheck,
  ExternalLink,
  LoaderCircle,
  CircleAlert,
  Cat,
  HardDrive,
  Layers,
  CheckCircle2,
  SlidersHorizontal,
  ChevronDown,
  Cpu,
  MemoryStick,
  Clock3,
} from "lucide-vue-next";
import type {
  Settings,
  Instance,
  Snapshot,
  VersionManifest,
  LauncherEvent,
  Challenge,
  Task,
} from "./types";

const desktop = isTauri();
const page = ref<"library" | "accounts" | "downloads" | "settings">("library");
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
const filter = ref("");
const selectedInstanceId = ref("");
const busy = ref(false);
const current = ref<Task | null>(null);
const tasks = ref<Task[]>([]);
const notice = ref("");
const error = ref("");
const logs = ref<{ time: string; kind: string; message: string }[]>([]);
const showLogs = ref(false);
const modal = ref<"create" | "import" | "edit" | "delete" | null>(null);
const editing = ref<Instance | null>(null);
const form = reactive({
  name: "",
  version: "1.21.1",
  source: "",
  memory_mb: 2048,
  java_path: "",
});
const challenge = ref<Challenge | null>(null);
const javaList = ref<{ path: string; major: number; version: string }[]>([]);
let unlisten: UnlistenFn | undefined;
const instances = computed(() =>
  snapshot.value.database.instances.filter((i) =>
    `${i.name} ${i.version}`.toLowerCase().includes(filter.value.toLowerCase()),
  ),
);
const selectedAccount = computed(() =>
  snapshot.value.database.accounts.find(
    (a) => a.id === snapshot.value.database.selected_account,
  ),
);
const selectedInstance = computed(
  () =>
    snapshot.value.database.instances.find(
      (i) => i.id === selectedInstanceId.value,
    ) || snapshot.value.database.instances[0],
);
const selectedRunning = computed(
  () =>
    !!selectedInstance.value &&
    snapshot.value.running.includes(selectedInstance.value.id),
);
const selectedDirectory = computed(() =>
  selectedInstance.value
    ? `${snapshot.value.root}/instances/${selectedInstance.value.id}/game`
    : "",
);
const downloadSource = computed(
  () =>
    ({ official: "Minecraft 官方", auto: "官方优先", bmclapi: "BMCLAPI 优先" })[
      snapshot.value.database.settings.download_source
    ] || "Minecraft 官方",
);
const nav = [
  { id: "library", label: "游戏实例", icon: Boxes },
  { id: "accounts", label: "账户管理", icon: UsersRound },
  {
    id: "downloads",
    label: "下载与修复",
    icon: ArrowDownToLine,
  },
  { id: "settings", label: "启动器设置", icon: Settings2 },
] as const;
const titles = {
  library: "游戏实例",
  accounts: "账户管理",
  downloads: "下载与修复",
  settings: "启动器设置",
};
function selectInstance(id: string) {
  selectedInstanceId.value = id;
  try {
    localStorage.setItem("nekocrate.selected-instance", id);
  } catch {
    /* Storage may be unavailable in a restricted webview. */
  }
}
function lastPlayed(value: number | null) {
  return value
    ? new Date(value * 1000).toLocaleString("zh-CN", {
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      })
    : "尚未启动";
}
async function copyInstance(instance: Instance) {
  const result = await action<Instance>(
    "复制实例",
    "clone",
    { id: instance.id },
    true,
  );
  if (result) {
    selectInstance(result.id);
    modal.value = null;
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
      cancellable: ["versions", "install", "launch"].includes(op),
      title,
      status: "running",
      message: "正在准备…",
      progress: 0,
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
      entry.message = "已完成";
    }
    await sync();
    notify(`${title}完成`);
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
  error.value = "";
  form.name = "";
  form.version = manifest.value?.latest.release || "1.21.1";
  form.source = "";
  modal.value = importing ? "import" : "create";
}
function edit(i: Instance) {
  error.value = "";
  editing.value = i;
  Object.assign(form, {
    name: i.name,
    version: i.version,
    memory_mb: i.memory_mb,
    java_path: i.java_path || "",
  });
  modal.value = "edit";
}
async function chooseDirectory() {
  try {
    const path = await open({
      directory: true,
      multiple: false,
      title: "选择现有 Minecraft 游戏目录",
    });
    if (typeof path === "string") form.source = path;
  } catch (e) {
    error.value = message(e);
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
  let result: Instance | boolean | undefined;
  if (modal.value === "create")
    result = await action<Instance>("创建实例", "create", {
      name: form.name,
      version: form.version,
    });
  else if (modal.value === "import")
    result = await action<Instance>(
      "迁移实例",
      "import",
      { source: form.source, name: form.name, version: form.version },
      true,
    );
  else if (modal.value === "edit" && editing.value)
    result = await action<boolean>("保存实例", "update", {
      id: editing.value.id,
      name: form.name,
      memory_mb: form.memory_mb,
      java_path: form.java_path || null,
    });
  if (result !== undefined) {
    if (typeof result === "object") selectInstance(result.id);
    modal.value = null;
  }
}
async function install(i: Instance) {
  await action(
    i.installed ? "校验与修复" : "安装游戏",
    "install",
    { id: i.id },
    true,
  );
}
async function launch(i: Instance) {
  if (!selectedAccount.value) {
    page.value = "accounts";
    notify("添加微软账户后即可启动游戏。");
    return;
  }
  await action("启动游戏", "launch", { id: i.id }, true);
}
async function cancel() {
  try {
    await rpc("cancel");
    notify("正在取消，已校验的文件将保留在缓存中。");
  } catch (e) {
    error.value = message(e);
  }
}
async function login() {
  if (!settings.microsoft_client_id) {
    page.value = "settings";
    notify("请先配置 Microsoft Client ID。");
    return;
  }
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    challenge.value = await rpc<Challenge>("begin_login");
    await rpc("wait_login");
    challenge.value = null;
    await sync();
    notify("登录成功，Java 版权益已验证");
  } catch (e) {
    error.value = message(e);
    challenge.value = null;
  } finally {
    busy.value = false;
  }
}
async function saveSettings() {
  const result = await action("保存设置", "save_settings", {
    settings: {
      ...settings,
      microsoft_client_id: settings.microsoft_client_id.trim(),
    },
  });
  if (result) Object.assign(settings, snapshot.value.database.settings);
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
  if (current.value) {
    if (e.kind === "stage") current.value.progress = 0;
    if (e.kind === "progress" && e.total > 0)
      current.value.progress = Math.round((e.completed / e.total) * 100);
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
  try {
    selectedInstanceId.value =
      localStorage.getItem("nekocrate.selected-instance") || "";
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
onUnmounted(() => unlisten?.());
</script>

<template>
  <div class="shell">
    <aside class="sidebar">
      <a
        class="brand"
        href="#"
        aria-label="NekoCrate 主页"
        @click.prevent="page = 'library'"
      >
        <span class="brand-mark"><Cat :size="23" :stroke-width="1.8" /></span>
        <span><strong>NekoCrate</strong><small>Launcher</small></span>
      </a>
      <button class="account-shortcut" @click="page = 'accounts'">
        <span class="avatar"><UsersRound :size="20" /></span>
        <span class="account-label"
          ><strong>{{ selectedAccount?.username || "未登录" }}</strong
          ><small>{{
            selectedAccount ? "Microsoft 账户" : "添加微软账户"
          }}</small></span
        >
        <ChevronRight :size="14" />
      </button>
      <nav aria-label="主导航">
        <button
          v-for="item in nav"
          :key="item.id"
          :class="['nav-item', { active: page === item.id }]"
          :aria-current="page === item.id ? 'page' : undefined"
          @click="page = item.id"
        >
          <component :is="item.icon" :size="18" /><span>{{ item.label }}</span>
          <span v-if="item.id === 'library'" class="nav-count">{{
            snapshot.database.instances.length
          }}</span>
          <span
            v-if="item.id === 'downloads' && current"
            class="activity-dot"
          />
        </button>
      </nav>
      <div class="sidebar-bottom">
        <button
          class="nav-item"
          :class="{ active: showLogs }"
          @click="showLogs = !showLogs"
        >
          <Terminal :size="18" /><span>运行日志</span>
        </button>
        <button class="nav-item" @click="openFolder()">
          <FolderOpen :size="18" /><span>数据目录</span>
        </button>
        <div class="build-label">
          <span>NekoCrate</span><span>0.1.0 · 开发版</span>
        </div>
      </div>
    </aside>

    <div class="workspace">
      <header class="topbar">
        <h1>{{ titles[page] }}</h1>
        <div class="toolbar-actions">
          <template v-if="page === 'library'">
            <button
              class="button secondary"
              :disabled="busy"
              @click="newInstance(true)"
            >
              <Download :size="15" />导入
            </button>
            <button
              class="button primary"
              :disabled="busy"
              @click="newInstance()"
            >
              <Plus :size="16" />新建实例
            </button>
          </template>
          <button
            v-else-if="page === 'accounts'"
            class="button primary"
            :disabled="busy"
            @click="login"
          >
            <Plus :size="16" />添加账户
          </button>
          <button
            v-else-if="page === 'downloads'"
            class="button secondary"
            :disabled="busy"
            @click="loadVersions"
          >
            <RefreshCw :size="15" />同步版本目录
          </button>
          <button
            v-else
            class="button primary"
            :disabled="busy"
            @click="saveSettings"
          >
            <Check :size="16" />保存设置
          </button>
        </div>
      </header>

      <div v-if="!desktop" class="preview-note">
        <CircleAlert :size="15" />浏览器预览；安装和启动请使用桌面版。
      </div>
      <div v-if="error && !modal" class="error-banner" role="alert">
        <CircleAlert :size="17" />
        <p>{{ error }}</p>
        <button
          class="icon-button"
          aria-label="关闭错误提示"
          @click="error = ''"
        >
          <X :size="16" />
        </button>
      </div>

      <main :class="['page-content', { 'library-page': page === 'library' }]">
        <template v-if="page === 'library'">
          <section class="instance-list" aria-label="游戏实例列表">
            <div class="list-toolbar">
              <label class="search"
                ><Search :size="16" /><input
                  v-model="filter"
                  placeholder="搜索实例"
                  aria-label="搜索实例"
              /></label>
            </div>
            <div class="list-heading">
              <span>已添加的实例</span><span>{{ instances.length }}</span>
            </div>
            <div class="instance-items">
              <button
                v-for="i in instances"
                :key="i.id"
                :class="[
                  'instance-item',
                  { selected: selectedInstance?.id === i.id },
                ]"
                :aria-pressed="selectedInstance?.id === i.id"
                @click="selectInstance(i.id)"
              >
                <span class="instance-icon"
                  ><Box :size="23" :stroke-width="1.5"
                /></span>
                <span class="instance-label"
                  ><strong>{{ i.name }}</strong
                  ><small
                    >{{ i.version }}<span class="dot-separator">·</span
                    >{{
                      snapshot.running.includes(i.id)
                        ? "运行中"
                        : i.installed
                          ? "已安装"
                          : "待安装"
                    }}</small
                  ></span
                >
                <span
                  v-if="snapshot.running.includes(i.id)"
                  class="running-indicator"
                /><Check
                  v-else-if="selectedInstance?.id === i.id"
                  class="selection-check"
                  :size="16"
                />
              </button>
              <div v-if="!instances.length" class="list-empty">
                <Box :size="24" />
                <p>{{ filter ? "未找到匹配实例" : "暂无实例" }}</p>
                <button v-if="filter" class="text-button" @click="filter = ''">
                  清除搜索
                </button>
              </div>
            </div>
            <button class="list-add" :disabled="busy" @click="newInstance()">
              <Plus :size="16" />添加游戏版本
            </button>
          </section>

          <section
            v-if="selectedInstance"
            class="instance-detail"
            aria-label="当前实例"
          >
            <div class="detail-heading">
              <span class="detail-icon"
                ><Box :size="30" :stroke-width="1.5"
              /></span>
              <div class="grow">
                <h2>{{ selectedInstance.name }}</h2>
                <p>
                  Minecraft {{ selectedInstance.version
                  }}<span class="dot-separator">·</span>Java Edition
                </p>
              </div>
              <span
                :class="[
                  'status-tag',
                  {
                    ready: selectedInstance.installed,
                    running: selectedRunning,
                  },
                ]"
                ><span />{{
                  selectedRunning
                    ? "运行中"
                    : selectedInstance.installed
                      ? "已安装"
                      : "待安装"
                }}</span
              >
            </div>
            <div class="detail-toolbar">
              <button
                class="button secondary"
                :disabled="busy || selectedRunning"
                @click="edit(selectedInstance)"
              >
                <SlidersHorizontal :size="15" />实例设置
              </button>
              <button
                class="button subtle"
                @click="openFolder(selectedInstance.id)"
              >
                <FolderOpen :size="15" />游戏目录
              </button>
              <button
                class="button subtle"
                :disabled="busy || selectedRunning"
                @click="copyInstance(selectedInstance)"
              >
                <Copy :size="15" />复制
              </button>
              <button
                class="button subtle"
                :disabled="busy || selectedRunning"
                @click="install(selectedInstance)"
              >
                <RefreshCw :size="15" />修复
              </button>
            </div>
            <section class="config-section">
              <h3>运行配置</h3>
              <dl class="config-list">
                <div>
                  <dt><Layers :size="17" />游戏版本</dt>
                  <dd>
                    {{ selectedInstance.version }}
                    <span class="inline-label">原版</span>
                  </dd>
                </div>
                <div>
                  <dt><MemoryStick :size="17" />最大内存</dt>
                  <dd>
                    {{ (selectedInstance.memory_mb / 1024).toFixed(1) }} GiB
                    <small>{{ selectedInstance.memory_mb }} MiB</small>
                  </dd>
                </div>
                <div>
                  <dt><Cpu :size="17" />Java</dt>
                  <dd
                    :title="selectedInstance.java_path || ''"
                    class="path-value"
                  >
                    {{ selectedInstance.java_path || "自动匹配" }}
                  </dd>
                </div>
                <div>
                  <dt><ShieldCheck :size="17" />版本隔离</dt>
                  <dd>独立目录</dd>
                </div>
                <div>
                  <dt><Clock3 :size="17" />上次启动</dt>
                  <dd>{{ lastPlayed(selectedInstance.last_played) }}</dd>
                </div>
              </dl>
            </section>
            <section class="directory-section">
              <h3>游戏目录</h3>
              <div class="directory-path">
                <code>{{ selectedDirectory }}</code
                ><button
                  class="icon-button"
                  aria-label="复制游戏目录路径"
                  title="复制路径"
                  @click="copy(selectedDirectory)"
                >
                  <Copy :size="15" />
                </button>
              </div>
            </section>
          </section>
          <section v-else class="empty-detail">
            <Box :size="36" :stroke-width="1.3" />
            <h2>暂无游戏实例</h2>
            <p>新建实例，或导入已有的 Minecraft 游戏目录。</p>
            <div class="inline-actions">
              <button
                class="button primary"
                :disabled="busy"
                @click="newInstance()"
              >
                <Plus :size="16" />新建实例</button
              ><button
                class="button secondary"
                :disabled="busy"
                @click="newInstance(true)"
              >
                <Download :size="16" />导入实例
              </button>
            </div>
          </section>
        </template>

        <div v-if="page === 'accounts'" class="page-inner">
          <div class="section-heading">
            <h2>
              微软账户 <span>{{ snapshot.database.accounts.length }}</span>
            </h2>
            <button
              v-if="selectedAccount"
              class="button secondary"
              :disabled="busy"
              @click="action('刷新登录', 'refresh_account', {}, true)"
            >
              <RefreshCw :size="15" />刷新权益
            </button>
          </div>
          <div v-if="challenge" class="panel login-panel">
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
              {{ challenge.user_code }}<Copy :size="19" />
            </button>
            <div class="inline-actions">
              <button
                class="button primary"
                @click="openLink(challenge.verification_uri)"
              >
                打开登录页 <ExternalLink :size="15" /></button
              ><button class="button secondary" @click="cancel">
                取消登录
              </button>
            </div>
            <p class="waiting">
              <LoaderCircle :size="14" class="spin" />等待授权…
            </p>
          </div>
          <div
            v-if="!snapshot.database.accounts.length && !challenge"
            class="empty-state"
          >
            <UsersRound :size="30" :stroke-width="1.5" />
            <h3>尚未添加账户</h3>
            <p>登录拥有 Minecraft Java 版的微软账户后即可启动游戏。</p>
            <button class="button primary" :disabled="busy" @click="login">
              <Plus :size="16" />添加账户
            </button>
          </div>
          <div
            v-for="account in snapshot.database.accounts"
            :key="account.id"
            class="panel account-row"
          >
            <span class="avatar"><UsersRound :size="22" /></span>
            <div class="grow">
              <h3>
                {{ account.username }}
                <span
                  v-if="account.id === snapshot.database.selected_account"
                  class="pill"
                  >当前</span
                >
              </h3>
              <p>
                Microsoft<span class="dot-separator">·</span>Java 版权益已验证
              </p>
              <small class="mono">{{ account.id }}</small>
            </div>
            <button
              v-if="account.id !== snapshot.database.selected_account"
              class="button secondary"
              :disabled="busy"
              @click="action('切换账户', 'select_account', { id: account.id })"
            >
              切换</button
            ><button
              class="icon-button danger"
              :disabled="busy"
              :aria-label="`移除账户 ${account.username}`"
              @click="action('移除账户', 'remove_account', { id: account.id })"
            >
              <Trash2 :size="17" />
            </button>
          </div>
          <p class="section-note">
            <ShieldCheck
              :size="15"
            />授权在微软官网完成，刷新凭据保存在系统钥匙串。
          </p>
        </div>

        <div v-if="page === 'downloads'" class="page-inner">
          <div class="section-heading">
            <h2>
              本次会话任务 <span>{{ tasks.length }}</span>
            </h2>
            <button class="text-button" @click="page = 'settings'">
              {{ downloadSource }} ·
              {{ snapshot.database.settings.concurrency }} 并发
              <ChevronRight :size="14" />
            </button>
          </div>
          <div v-if="!tasks.length" class="empty-state compact">
            <ArrowDownToLine :size="27" :stroke-width="1.5" />
            <h3>暂无下载任务</h3>
          </div>
          <div v-for="task in tasks" :key="task.id" class="panel task-row">
            <span :class="['task-icon', task.status]"
              ><LoaderCircle
                v-if="task.status === 'running'"
                class="spin"
                :size="19" /><Check
                v-else-if="task.status === 'done'"
                :size="19" /><CircleAlert v-else :size="19"
            /></span>
            <div class="grow">
              <div class="task-title">
                <strong>{{ task.title }}</strong
                ><time>{{ task.started }}</time>
              </div>
              <p>{{ task.message }}</p>
              <div
                v-if="task.status === 'running'"
                class="progress-track"
                role="progressbar"
                :aria-valuenow="task.progress"
                aria-valuemin="0"
                aria-valuemax="100"
                :aria-label="task.title"
              >
                <i :style="{ width: `${task.progress}%` }" />
              </div>
            </div>
            <button
              v-if="task.status === 'running' && task.cancellable"
              class="button secondary"
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
          <div
            v-if="snapshot.database.instances.length"
            class="section-heading spaced"
          >
            <h2>文件校验与修复</h2>
          </div>
          <div
            v-for="i in snapshot.database.instances"
            :key="i.id"
            class="panel repair-row"
          >
            <span class="instance-icon small"><Box :size="20" /></span>
            <div class="grow">
              <strong>{{ i.name }}</strong>
              <p>Minecraft {{ i.version }}</p>
            </div>
            <button
              class="button secondary"
              :disabled="busy || snapshot.running.includes(i.id)"
              @click="install(i)"
            >
              <RefreshCw :size="15" />校验与修复
            </button>
          </div>
        </div>

        <div v-if="page === 'settings'" class="page-inner settings-page">
          <section class="settings-section">
            <h2><UsersRound :size="18" />微软登录</h2>
            <label class="field"
              >Microsoft Client ID<input
                v-model="settings.microsoft_client_id"
                placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                spellcheck="false"
              /><small
                >填写公共客户端应用 ID，无需 Client Secret。</small
              ></label
            ><button
              class="text-button"
              @click="
                openLink(
                  'https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code',
                )
              "
            >
              应用配置说明 <ExternalLink :size="14" />
            </button>
          </section>
          <section class="settings-section">
            <h2><Download :size="18" />下载</h2>
            <div class="field-grid">
              <label class="field wide"
                >下载来源<select v-model="settings.download_source">
                  <option value="official">Minecraft 官方</option>
                  <option value="auto">官方优先，失败回退 BMCLAPI</option>
                  <option value="bmclapi">BMCLAPI 优先，失败回退官方</option>
                </select></label
              ><label class="field"
                >并发数<input
                  v-model.number="settings.concurrency"
                  type="number"
                  min="1"
                  max="32" /></label
              ><label class="field"
                >重试次数<input
                  v-model.number="settings.retries"
                  type="number"
                  min="0"
                  max="5"
              /></label>
            </div>
            <p class="field-help">
              镜像用于游戏文件，账户授权始终连接微软官方服务。
            </p>
          </section>
          <section class="settings-section">
            <h2><Cpu :size="18" />Java</h2>
            <label class="toggle-row"
              ><span
                ><strong>自动安装缺少的 Java</strong
                ><small>通过 Eclipse Adoptium 下载并校验。</small></span
              ><input
                v-model="settings.auto_java"
                type="checkbox"
                role="switch" /></label
            ><button
              class="button secondary"
              :disabled="busy"
              @click="inspectJava"
            >
              <Search :size="15" />检测本机 Java
            </button>
            <div v-for="java in javaList" :key="java.path" class="java-row">
              <strong>Java {{ java.major }}</strong
              ><code>{{ java.path }}</code>
            </div>
          </section>
          <section class="settings-section">
            <h2><HardDrive :size="18" />存储</h2>
            <div class="directory-path">
              <code>{{ snapshot.root || "使用系统应用数据目录" }}</code
              ><button
                class="icon-button"
                aria-label="打开数据目录"
                @click="openFolder()"
              >
                <FolderOpen :size="17" />
              </button>
            </div>
          </section>
        </div>
      </main>

      <section v-if="showLogs" class="log-panel" aria-label="运行日志">
        <header>
          <span
            ><Terminal :size="15" />运行日志
            <small>{{ logs.length }} 条</small></span
          >
          <div class="inline-actions">
            <button
              class="icon-button"
              aria-label="复制日志"
              @click="
                copy(logs.map((l) => `[${l.time}] ${l.message}`).join('\n'))
              "
            >
              <Copy :size="15" /></button
            ><button
              class="icon-button"
              aria-label="清空日志"
              @click="logs = []"
            >
              <Trash2 :size="15" /></button
            ><button
              class="icon-button"
              aria-label="关闭日志"
              @click="showLogs = false"
            >
              <X :size="17" />
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
            >{{ line.message }}
          </p>
        </div>
      </section>

      <div v-if="current" class="task-dock" role="status">
        <LoaderCircle class="spin" :size="17" />
        <div class="grow">
          <strong>{{ current.title }}</strong
          ><span>{{ current.message }}</span>
        </div>
        <span class="task-percent">{{ current.progress }}%</span
        ><button
          v-if="current.cancellable"
          class="icon-button"
          aria-label="取消任务"
          @click="cancel"
        >
          <X :size="17" /></button
        ><i :style="{ width: `${current.progress}%` }" />
      </div>
      <footer class="launchbar">
        <div class="launch-selection">
          <label for="launch-instance">当前实例</label>
          <div class="select-wrapper">
            <select
              id="launch-instance"
              :value="selectedInstance?.id || ''"
              :disabled="!snapshot.database.instances.length"
              @change="
                selectInstance(($event.target as HTMLSelectElement).value)
              "
            >
              <option v-if="!snapshot.database.instances.length" value="">
                未选择实例
              </option>
              <option
                v-for="i in snapshot.database.instances"
                :key="i.id"
                :value="i.id"
              >
                {{ i.name }} · {{ i.version }}
              </option></select
            ><ChevronDown :size="14" />
          </div>
        </div>
        <div class="launch-account">
          <span>启动账户</span
          ><button @click="page = 'accounts'">
            {{ selectedAccount?.username || "未登录"
            }}<ChevronRight :size="13" />
          </button>
        </div>
        <button
          v-if="selectedRunning && selectedInstance"
          class="launch-button stop"
          :disabled="busy"
          @click="action('停止游戏', 'stop', { id: selectedInstance.id })"
        >
          <Square :size="17" />停止游戏
        </button>
        <button
          v-else-if="selectedInstance?.installed"
          class="launch-button"
          :disabled="busy"
          @click="launch(selectedInstance)"
        >
          <Play :size="19" fill="currentColor" /><span>启动游戏</span>
        </button>
        <button
          v-else-if="selectedInstance"
          class="launch-button"
          :disabled="busy"
          @click="install(selectedInstance)"
        >
          <Download :size="19" />安装游戏
        </button>
        <button
          v-else
          class="launch-button"
          :disabled="busy"
          @click="newInstance()"
        >
          <Plus :size="19" />新建实例
        </button>
      </footer>
    </div>

    <Transition name="toast"
      ><div v-if="notice" class="toast" role="status">
        <CheckCircle2 :size="17" />{{ notice }}
      </div></Transition
    >
    <div
      v-if="modal"
      class="modal-overlay"
      @click.self="!busy && (modal = null)"
      @keydown.esc="!busy && (modal = null)"
    >
      <section
        class="modal"
        role="dialog"
        aria-modal="true"
        :aria-label="
          modal === 'create'
            ? '新建实例'
            : modal === 'import'
              ? '导入实例'
              : modal === 'edit'
                ? '实例设置'
                : '移除实例'
        "
      >
        <div class="modal-header">
          <h2>
            {{
              modal === "create"
                ? "新建实例"
                : modal === "import"
                  ? "导入实例"
                  : modal === "edit"
                    ? "实例设置"
                    : "移除实例"
            }}
          </h2>
          <button
            class="icon-button"
            :disabled="busy"
            aria-label="关闭对话框"
            @click="modal = null"
          >
            <X :size="20" />
          </button>
        </div>
        <div v-if="error" class="error-banner" role="alert">
          <CircleAlert :size="17" />
          <p>{{ error }}</p>
        </div>
        <template v-if="modal === 'delete'"
          ><p class="delete-message">
            「{{ editing?.name }}」将移至回收目录，游戏文件会保留。
          </p>
          <div class="modal-actions">
            <button class="button secondary" @click="modal = 'edit'">
              取消</button
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
            <label class="field"
              >实例名称<input
                v-model="form.name"
                placeholder="例如：生存 1.21.1"
                required
                maxlength="80"
                autofocus /></label
            ><label class="field"
              >Minecraft 版本<input
                v-model="form.version"
                list="versions"
                :readonly="modal === 'edit'"
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
              ><small v-if="modal !== 'edit'"
                >支持原版 Java 1.13+，Apple Silicon 使用 1.19+。</small
              ></label
            >
            <label v-if="modal === 'import'" class="field"
              >来源目录
              <div class="input-action">
                <input
                  v-model="form.source"
                  required
                  placeholder=".minecraft 或实例游戏目录"
                /><button
                  type="button"
                  class="button secondary"
                  @click="chooseDirectory"
                >
                  <FolderOpen :size="16" />浏览
                </button>
              </div>
              <small
                >复制存档与配置，保留源文件；不导入账户凭据。原型不加载模组。</small
              ></label
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
              ><label class="field"
                >Java 可执行文件
                <div class="input-action">
                  <input
                    v-model="form.java_path"
                    placeholder="留空自动匹配"
                  /><button
                    type="button"
                    class="button secondary"
                    @click="chooseJava"
                  >
                    浏览
                  </button>
                </div></label
              >
              <div class="edit-tools">
                <button
                  type="button"
                  class="button secondary"
                  :disabled="busy"
                  @click="editing && copyInstance(editing)"
                >
                  <Copy :size="15" />复制实例</button
                ><button
                  type="button"
                  class="text-button danger"
                  :disabled="busy"
                  @click="modal = 'delete'"
                >
                  <Trash2 :size="15" />移除实例
                </button>
              </div></template
            >
          </div>
          <div class="modal-actions">
            <button
              type="button"
              class="button secondary"
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
                    : "创建实例"
              }}
            </button>
          </div>
        </form>
      </section>
    </div>
  </div>
</template>
