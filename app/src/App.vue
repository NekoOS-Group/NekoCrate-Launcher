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
  ArrowUpRight,
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
  Sparkles,
  HardDrive,
  Layers,
  CheckCircle2,
  CloudDownload,
  SlidersHorizontal,
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
const readyCount = computed(
  () => snapshot.value.database.instances.filter((i) => i.installed).length,
);
const nav = [
  { id: "library", label: "我的猫匣", en: "Library", icon: Boxes },
  { id: "accounts", label: "账户", en: "Accounts", icon: UsersRound },
  {
    id: "downloads",
    label: "下载与修复",
    en: "Downloads",
    icon: ArrowDownToLine,
  },
  { id: "settings", label: "设置", en: "Settings", icon: Settings2 },
] as const;
const titles = {
  library: "我的猫匣",
  accounts: "游戏账户",
  downloads: "下载与修复",
  settings: "启动器设置",
};
const subtitles = {
  library: "每一个世界，都有自己的位置。",
  accounts: "连接你的微软账户，准备下一段冒险。",
  downloads: "下载进度、文件校验与修复，一目了然。",
  settings: "让猫匣适应你的游玩方式。",
};
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
  let result: unknown;
  if (modal.value === "create")
    result = await action("创建实例", "create", {
      name: form.name,
      version: form.version,
    });
  else if (modal.value === "import")
    result = await action(
      "迁移实例",
      "import",
      { source: form.source, name: form.name, version: form.version },
      true,
    );
  else if (modal.value === "edit" && editing.value)
    result = await action("保存实例", "update", {
      id: editing.value.id,
      name: form.name,
      memory_mb: form.memory_mb,
      java_path: form.java_path || null,
    });
  if (result !== undefined) modal.value = null;
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
        @click.prevent="page = 'library'"
        aria-label="NekoCrate 主页"
        ><span class="brand-mark"><Cat :size="25" :stroke-width="1.9" /></span
        ><span>NekoCrate<small>LAUNCHER</small></span></a
      >
      <div class="sidebar-caption">你的冒险空间</div>
      <nav>
        <button
          v-for="item in nav"
          :key="item.id"
          :class="['nav-item', { active: page === item.id }]"
          @click="page = item.id"
        >
          <component :is="item.icon" :size="19" /><span>{{ item.label }}</span
          ><span v-if="item.id === 'library'" class="nav-count">{{
            snapshot.database.instances.length
          }}</span
          ><span v-if="item.id === 'downloads' && busy" class="activity-dot" />
        </button>
      </nav>
      <div class="sidebar-note">
        <span class="note-icon"><Sparkles :size="17" /></span
        ><strong>小小猫匣，无限世界</strong>
        <p>版本、存档和设置独立保存，<br />放心开始新的冒险。</p>
      </div>
      <div class="sidebar-bottom">
        <button class="account-shortcut" @click="page = 'accounts'">
          <span class="avatar"><Cat :size="20" /></span
          ><span
            ><strong>{{ selectedAccount?.username || "尚未登录" }}</strong
            ><small>{{
              selectedAccount ? "Microsoft 账户" : "连接你的游戏账户"
            }}</small></span
          ><ChevronRight :size="16" />
        </button>
        <div class="build-label">
          <span class="tiny-dot" /> 开发预览版 <span>0.1.0</span>
        </div>
      </div>
    </aside>
    <div class="workspace">
      <header class="topbar">
        <div class="breadcrumb">
          工作空间 <ChevronRight :size="13" /><strong>{{
            titles[page]
          }}</strong>
        </div>
        <div class="topbar-right">
          <span class="branch-badge">neko_dev</span
          ><button
            class="icon-button"
            :class="{ selected: showLogs }"
            title="查看运行日志"
            aria-label="查看运行日志"
            @click="showLogs = !showLogs"
          >
            <Terminal :size="18" />
          </button>
        </div>
      </header>
      <main>
        <div v-if="!desktop" class="preview-note">
          <CircleAlert :size="17" /> 浏览器界面预览 ·
          运行桌面版后可使用真实安装、账户和启动功能。
        </div>
        <div class="page-heading">
          <div>
            <p class="eyebrow">
              {{ nav.find((n) => n.id === page)?.en.toUpperCase() }}
            </p>
            <h1>{{ titles[page] }}</h1>
            <p class="subtitle">{{ subtitles[page] }}</p>
          </div>
          <div v-if="page === 'library'" class="heading-actions">
            <button
              class="button secondary"
              :disabled="busy"
              @click="newInstance(true)"
            >
              <Download :size="16" />迁移导入</button
            ><button
              class="button primary"
              :disabled="busy"
              @click="newInstance()"
            >
              <Plus :size="17" />新建实例
            </button>
          </div>
        </div>
        <div v-if="error" class="error-banner" role="alert">
          <CircleAlert :size="19" />
          <div>
            <strong>这一步没有完成</strong>
            <p>{{ error }}</p>
          </div>
          <button
            class="icon-button"
            aria-label="关闭错误提示"
            @click="error = ''"
          >
            <X :size="16" />
          </button>
        </div>
        <template v-if="page === 'library'">
          <section class="hero">
            <div class="hero-content">
              <span class="hero-tag"><span /> YOUR NEXT ADVENTURE</span>
              <h2>把下一段冒险，<br />装进猫匣。</h2>
              <p>
                一个实例，一个独立的世界。<br />从喜欢的 Minecraft 版本开始。
              </p>
              <button
                class="text-button"
                :disabled="busy"
                @click="newInstance()"
              >
                创建你的世界 <ArrowUpRight :size="18" />
              </button>
            </div>
            <div class="voxel-scene" aria-hidden="true">
              <div class="sun" />
              <div class="cloud cloud-one" />
              <div class="cloud cloud-two" />
              <div class="hill hill-back" />
              <div class="hill hill-front" />
              <div class="voxel-island">
                <div class="grass-top" />
                <div class="dirt-front" />
                <div class="dirt-side" />
                <div class="tree trunk-one"><i /><b /></div>
                <div class="tree trunk-two"><i /><b /></div>
                <div class="pixel-cat">
                  <i class="ear left" /><i class="ear right" /><i
                    class="eye left"
                  /><i class="eye right" /><i class="tail" />
                </div>
                <span class="flower flower-one" /><span
                  class="flower flower-two"
                />
              </div>
              <div class="floating-block" />
            </div>
          </section>
          <div class="stats-row">
            <div>
              <span class="stat-icon"><Boxes :size="20" /></span
              ><strong>{{ snapshot.database.instances.length }}</strong
              ><span>游戏实例</span>
            </div>
            <div>
              <span class="stat-icon"><CheckCircle2 :size="20" /></span
              ><strong>{{ readyCount }}</strong
              ><span>准备就绪</span>
            </div>
            <div>
              <span class="stat-icon"><ShieldCheck :size="20" /></span
              ><span><b>独立保存</b><small>存档与配置互不干扰</small></span>
            </div>
          </div>
          <div class="section-heading">
            <h3>
              全部实例 <span>{{ snapshot.database.instances.length }}</span>
            </h3>
            <label class="search"
              ><Search :size="16" /><input
                v-model="filter"
                placeholder="搜索名称或版本"
                aria-label="搜索实例"
            /></label>
          </div>
          <div v-if="!instances.length" class="empty-library">
            <span class="empty-icon"
              ><Box :size="32" :stroke-width="1.3"
            /></span>
            <h3>{{ filter ? "没有匹配的实例" : "你的猫匣，等一个新世界" }}</h3>
            <p>
              {{
                filter
                  ? "换个名称或版本号试试。"
                  : "新建一个实例，或者把已有的游戏目录搬进来。"
              }}
            </p>
            <button
              v-if="!filter"
              class="button secondary"
              :disabled="busy"
              @click="newInstance()"
            >
              <Plus :size="16" />创建第一个实例
            </button>
          </div>
          <div v-else class="instance-grid">
            <article
              v-for="(i, index) in instances"
              :key="i.id"
              class="instance-card"
            >
              <div :class="['instance-art', `art-${index % 3}`]">
                <span class="version-tag">JAVA EDITION</span>
                <div class="mini-cubes"><i /><i /><i /></div>
                <span
                  :class="[
                    'status-tag',
                    {
                      ready: i.installed,
                      running: snapshot.running.includes(i.id),
                    },
                  ]"
                  ><span />{{
                    snapshot.running.includes(i.id)
                      ? "运行中"
                      : i.installed
                        ? "准备就绪"
                        : "待安装"
                  }}</span
                >
              </div>
              <div class="instance-body">
                <div class="instance-title">
                  <h3>{{ i.name }}</h3>
                  <button
                    class="icon-button"
                    :disabled="busy || snapshot.running.includes(i.id)"
                    :aria-label="`设置 ${i.name}`"
                    @click="edit(i)"
                  >
                    <SlidersHorizontal :size="16" />
                  </button>
                </div>
                <p class="instance-meta">
                  Minecraft {{ i.version }} <span>·</span>
                  {{ (i.memory_mb / 1024).toFixed(1) }} GiB
                </p>
                <div class="instance-actions">
                  <button
                    v-if="snapshot.running.includes(i.id)"
                    class="button stop"
                    @click="action('停止游戏', 'stop', { id: i.id })"
                  >
                    <Square :size="14" />停止游戏</button
                  ><button
                    v-else-if="i.installed"
                    class="button play"
                    :disabled="busy"
                    @click="launch(i)"
                  >
                    <Play :size="15" fill="currentColor" />开始游戏</button
                  ><button
                    v-else
                    class="button play"
                    :disabled="busy"
                    @click="install(i)"
                  >
                    <Download :size="16" />安装游戏</button
                  ><button
                    class="icon-button"
                    title="打开游戏目录"
                    aria-label="打开游戏目录"
                    @click="openFolder(i.id)"
                  >
                    <FolderOpen :size="18" />
                  </button>
                </div>
              </div>
            </article>
          </div>
        </template>
        <template v-if="page === 'accounts'">
          <div class="panel account-intro">
            <span class="large-icon"><ShieldCheck :size="32" /></span>
            <div>
              <h3>用微软账户开启游戏</h3>
              <p>
                在微软官方页面授权，猫匣不会收集你的密码。<br />刷新凭据保存在系统钥匙串，启动前自动检查游戏权益。
              </p>
            </div>
            <button class="button primary" :disabled="busy" @click="login">
              <Plus :size="16" />添加账户
            </button>
          </div>
          <div v-if="challenge" class="panel login-panel">
            <span class="eyebrow">MICROSOFT SIGN IN</span>
            <h3>在浏览器中完成授权</h3>
            <p>
              打开微软登录页面，输入下方验证码。有效期约
              {{ Math.ceil(challenge.expires_in / 60) }} 分钟。
            </p>
            <button class="device-code" @click="copy(challenge.user_code)">
              {{ challenge.user_code }}<Copy :size="20" />
            </button>
            <div class="inline-actions">
              <button
                class="button primary"
                @click="openLink(challenge.verification_uri)"
              >
                打开微软登录页 <ExternalLink :size="16" /></button
              ><button class="button secondary" @click="cancel">
                取消登录
              </button>
            </div>
            <p class="waiting">
              <LoaderCircle :size="15" class="spin" />等待你完成授权…
            </p>
          </div>
          <div class="section-heading">
            <h3>
              已连接账户 <span>{{ snapshot.database.accounts.length }}</span>
            </h3>
            <button
              v-if="selectedAccount"
              class="button secondary"
              :disabled="busy"
              @click="action('刷新登录', 'refresh_account', {}, true)"
            >
              <RefreshCw :size="15" />刷新权益
            </button>
          </div>
          <div
            v-if="!snapshot.database.accounts.length && !challenge"
            class="empty-library"
          >
            <UsersRound :size="32" />
            <h3>还没有游戏账户</h3>
            <p>你可以先创建和安装实例，登录后开始游戏。</p>
          </div>
          <div
            v-for="account in snapshot.database.accounts"
            :key="account.id"
            class="panel account-row"
          >
            <span class="avatar large"><Cat :size="26" /></span>
            <div class="grow">
              <h3>
                {{ account.username }}
                <span
                  v-if="account.id === snapshot.database.selected_account"
                  class="pill"
                  >当前账户</span
                >
              </h3>
              <p>Microsoft <span>·</span> Java 版权益已验证</p>
              <small class="mono">{{ account.id }}</small>
            </div>
            <button
              v-if="account.id !== snapshot.database.selected_account"
              class="button secondary"
              :disabled="busy"
              @click="action('切换账户', 'select_account', { id: account.id })"
            >
              使用此账户</button
            ><button
              class="icon-button danger"
              :disabled="busy"
              aria-label="移除账户"
              @click="action('移除账户', 'remove_account', { id: account.id })"
            >
              <Trash2 :size="18" />
            </button>
          </div>
        </template>
        <template v-if="page === 'downloads'">
          <div class="panel download-intro">
            <CloudDownload :size="36" />
            <div>
              <h3>下载有校验，重试有缓存</h3>
              <p>校验通过的文件会被复用。中断后再次安装，将跳过完整文件。</p>
            </div>
            <button
              class="button secondary"
              :disabled="busy"
              @click="loadVersions"
            >
              <RefreshCw :size="16" />同步版本目录
            </button>
          </div>
          <div class="section-heading">
            <h3>
              本次会话任务 <span>{{ tasks.length }}</span>
            </h3>
            <span class="muted"
              >{{
                settings.download_source === "official"
                  ? "官方源"
                  : settings.download_source === "auto"
                    ? "官方优先 · 自动回退"
                    : "BMCLAPI 优先"
              }}
              · {{ settings.concurrency }} 并发</span
            >
          </div>
          <div v-if="!tasks.length" class="empty-library">
            <CheckCircle2 :size="32" />
            <h3>没有等待中的下载</h3>
            <p>在「我的猫匣」中安装游戏，或者选择下方实例进行修复。</p>
          </div>
          <div v-for="task in tasks" :key="task.id" class="panel task-row">
            <span :class="['task-icon', task.status]"
              ><LoaderCircle
                v-if="task.status === 'running'"
                class="spin"
                :size="20" /><Check
                v-else-if="task.status === 'done'"
                :size="20" /><CircleAlert v-else :size="20"
            /></span>
            <div class="grow">
              <div class="task-title">
                <strong>{{ task.title }}</strong
                ><span>{{ task.started }}</span>
              </div>
              <p>{{ task.message }}</p>
              <div v-if="task.status === 'running'" class="progress-track">
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
            class="section-heading"
          >
            <h3>实例文件修复</h3>
          </div>
          <div
            v-for="i in snapshot.database.instances"
            :key="i.id"
            class="panel repair-row"
          >
            <Box :size="22" />
            <div class="grow">
              <strong>{{ i.name }}</strong>
              <p>{{ i.version }} · 检查并补齐资源、依赖、原生库和 Java</p>
            </div>
            <button
              class="button secondary"
              :disabled="busy || snapshot.running.includes(i.id)"
              @click="install(i)"
            >
              <RefreshCw :size="15" />校验与修复
            </button>
          </div>
        </template>
        <template v-if="page === 'settings'">
          <section class="panel settings-panel">
            <div class="panel-title">
              <UsersRound :size="20" />
              <div>
                <h3>微软应用配置</h3>
                <p>使用你注册的公共客户端应用，通过设备验证码登录。</p>
              </div>
            </div>
            <label class="field"
              >Microsoft Client ID<input
                v-model="settings.microsoft_client_id"
                placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                spellcheck="false"
              /><small
                >原型不内置其他启动器的应用标识，无需填写 Client Secret。</small
              ></label
            ><button
              class="text-button"
              @click="
                openLink(
                  'https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code',
                )
              "
            >
              查看微软设备授权说明 <ArrowUpRight :size="15" />
            </button>
          </section>
          <section class="panel settings-panel">
            <div class="panel-title">
              <CloudDownload :size="20" />
              <div>
                <h3>下载与网络</h3>
                <p>切换来源不会关闭文件完整性校验。</p>
              </div>
            </div>
            <div class="field-grid">
              <label class="field"
                >下载来源<select v-model="settings.download_source">
                  <option value="official">Minecraft 官方</option>
                  <option value="auto">官方优先，失败回退 BMCLAPI</option>
                  <option value="bmclapi">BMCLAPI 优先，失败回退官方</option>
                </select></label
              ><label class="field"
                >并发下载数<input
                  v-model.number="settings.concurrency"
                  type="number"
                  min="1"
                  max="32" /></label
              ><label class="field"
                >每个来源重试次数<input
                  v-model.number="settings.retries"
                  type="number"
                  min="0"
                  max="5"
              /></label>
            </div>
            <p class="field-help">
              镜像仅用于游戏文件与元数据；账户授权始终连接微软官方服务。
            </p>
          </section>
          <section class="panel settings-panel">
            <div class="panel-title">
              <Layers :size="20" />
              <div>
                <h3>Java 运行环境</h3>
                <p>根据官方版本要求匹配 Java，也可在每个实例中指定路径。</p>
              </div>
            </div>
            <label class="toggle-row"
              ><span
                ><strong>自动安装缺少的 Java</strong
                ><small
                  >通过 Eclipse Adoptium 获取，下载后验证 SHA-256。</small
                ></span
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
          <section class="panel settings-panel">
            <div class="panel-title">
              <HardDrive :size="20" />
              <div>
                <h3>数据目录</h3>
                <p>共享资源集中缓存，每个实例使用独立游戏目录。</p>
              </div>
            </div>
            <code class="data-path">{{
              snapshot.root || "桌面版将自动选择系统应用数据目录"
            }}</code
            ><button class="text-button" @click="openFolder()">
              打开数据目录 <ArrowUpRight :size="15" />
            </button>
          </section>
          <div class="settings-footer">
            <span class="muted">设置保存在本机。</span
            ><button
              class="button primary"
              :disabled="busy"
              @click="saveSettings"
            >
              <Check :size="17" />保存设置
            </button>
          </div>
        </template>
        <footer class="page-footer">
          <span>NekoCrate · 为你的每个世界留一个位置</span
          ><span>Java Edition · Prototype</span>
        </footer>
      </main>
      <div v-if="current" class="task-dock">
        <LoaderCircle class="spin" :size="19" />
        <div>
          <strong>{{ current.title }}</strong
          ><span>{{ current.message }}</span>
        </div>
        <span>{{ current.progress }}%</span
        ><button
          v-if="current.cancellable"
          class="icon-button"
          title="取消任务"
          aria-label="取消任务"
          @click="cancel"
        >
          <X :size="17" /></button
        ><i :style="{ width: `${current.progress}%` }" />
      </div>
      <section v-if="showLogs" class="log-panel">
        <header>
          <span
            ><Terminal :size="16" />运行日志
            <small>最近 {{ logs.length }} 条</small></span
          >
          <div>
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
              <X :size="16" />
            </button>
          </div>
        </header>
        <div class="log-content">
          <p v-if="!logs.length" class="muted">
            启动游戏或执行任务后，日志会显示在这里。
          </p>
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
    >
      <section
        class="modal"
        role="dialog"
        aria-modal="true"
        :aria-label="
          modal === 'create'
            ? '新建实例'
            : modal === 'import'
              ? '迁移导入'
              : modal === 'edit'
                ? '实例设置'
                : '移除实例'
        "
      >
        <div class="modal-header">
          <span class="modal-icon"><Box :size="23" /></span
          ><button
            class="icon-button"
            :disabled="busy"
            aria-label="关闭对话框"
            @click="modal = null"
          >
            <X :size="20" />
          </button>
        </div>
        <div v-if="error" class="error-banner" role="alert">
          <CircleAlert :size="18" />
          <p>{{ error }}</p>
        </div>
        <template v-if="modal === 'delete'"
          ><h2>把这个实例移出猫匣？</h2>
          <p class="subtitle">
            {{ editing?.name }} 将移入数据目录的 trash
            文件夹，游戏文件不会被永久删除。
          </p>
          <div class="modal-actions">
            <button class="button secondary" @click="modal = 'edit'">
              返回</button
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
              移入回收目录
            </button>
          </div></template
        >
        <form v-else @submit.prevent="submit">
          <h2>
            {{
              modal === "create"
                ? "装进一个新世界"
                : modal === "import"
                  ? "把熟悉的世界搬进来"
                  : "让这个世界更合心意"
            }}
          </h2>
          <p class="subtitle">
            {{
              modal === "edit"
                ? "只影响当前实例的设置。"
                : modal === "import"
                  ? "复制已有游戏目录，保留来源文件。"
                  : "为不同版本、存档和配置创建独立空间。"
            }}
          </p>
          <label class="field"
            >实例名称<input
              v-model="form.name"
              placeholder="例如：我的生存世界"
              required
              maxlength="80"
              autofocus /></label
          ><label class="field"
            >Minecraft 版本<input
              v-model="form.version"
              list="versions"
              :readonly="modal === 'edit'"
              placeholder="例如：1.21.1"
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
              >官方 Java 版 1.13+；Apple Silicon 使用
              1.19+。可在下载页同步完整版本目录。</small
            ></label
          ><label v-if="modal === 'import'" class="field"
            >来源游戏目录
            <div class="input-action">
              <input
                v-model="form.source"
                required
                placeholder="选择 .minecraft 或实例游戏目录"
              /><button
                type="button"
                class="button secondary"
                @click="chooseDirectory"
              >
                <FolderOpen :size="17" />浏览
              </button>
            </div>
            <small
              >迁移存档、资源和配置，不导入其他启动器的账户凭据。原型启动官方原版，不加载迁入的模组。</small
            ></label
          ><template v-if="modal === 'edit'"
            ><label class="field"
              >最大内存 <strong>{{ form.memory_mb }} MiB</strong
              ><input
                v-model.number="form.memory_mb"
                type="range"
                min="512"
                max="16384"
                step="256" /></label
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
                @click="
                  action('复制实例', 'clone', { id: editing?.id }, true).then(
                    (result) => {
                      if (result) modal = null;
                    },
                  )
                "
              >
                <Copy :size="15" />复制实例</button
              ><button
                type="button"
                class="text-button danger"
                @click="modal = 'delete'"
              >
                <Trash2 :size="15" />移除实例
              </button>
            </div></template
          >
          <div class="modal-actions">
            <button
              type="button"
              class="button secondary"
              :disabled="busy"
              @click="modal = null"
            >
              取消</button
            ><button type="submit" class="button primary" :disabled="busy">
              <LoaderCircle v-if="busy" class="spin" :size="16" /><Check
                v-else-if="modal === 'edit'"
                :size="16"
              /><Plus v-else :size="16" />{{
                modal === "edit"
                  ? "保存设置"
                  : modal === "import"
                    ? "复制并导入"
                    : "创建实例"
              }}
            </button>
          </div>
        </form>
      </section>
    </div>
  </div>
</template>
