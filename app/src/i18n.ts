import { computed, ref } from "vue";
import { translateBackend } from "./backendMessages.ts";

export type Locale = "zh-CN" | "en-US";
export const languagePreference = ref("system");
const systemLocale = ref(
  typeof navigator === "undefined" ? "en-US" : navigator.language,
);

export function resolveLanguage(preference: string, system: string): Locale {
  if (preference === "zh-CN" || preference === "en-US") return preference;
  const parts = system.replaceAll("_", "-").toLowerCase().split("-");
  return parts[0] === "zh" &&
    (parts.includes("hans") ||
      (!parts.includes("hant") &&
        !parts.some((part) => ["tw", "hk", "mo"].includes(part))))
    ? "zh-CN"
    : "en-US";
}
export const locale = computed(() =>
  resolveLanguage(languagePreference.value, systemLocale.value),
);
export function setLanguage(preference: string, system: string) {
  languagePreference.value = preference;
  systemLocale.value = system;
}

/** Chinese source keys stay readable in templates; interpolate only after translation. */
export const english: Readonly<Record<string, string>> = {
  正在查找实例: "Looking for instances",
  正在关联实例: "Linking instances",
  未找到实例: "No instances found",
  拖到此处移除实例: "Drop here to remove the instance",
  "运行或任务处理中，暂不可移除实例":
    "Cannot remove while the game or a task is running",
  "已关联实例 {name}: {path}": "Linked instance {name}: {path}",
  查找游戏目录: "Find game directories",
  "正在查找 .minecraft 文件夹": "Looking for .minecraft folders",
  "找到 {count} 个游戏目录": "Found {count} game directories",
  取消全选: "Deselect all",
  全选: "Select all",
  "直接使用原目录，游戏文件保持原位。":
    "Use the original directories. Game files stay in place.",
  已关联: "Linked",
  无法关联: "Unavailable",
  "未找到含 saves、mods、versions 或 options.txt 的游戏目录":
    "No game directory containing saves, mods, versions or options.txt was found",
  "已跳过 {count} 项": "Skipped {count} items",
  请等待当前任务完成后关联:
    "Wait for the current task to finish before linking",
  "关联所选 ({count})": "Link selected ({count})",
  拖拽导入实例: "Drop to import instances",
  默认使用上层文件夹名称: "Defaults to the parent folder name",
  猫咪盒子启动器: "NekoCrate Launcher",
  解析游戏版本: "Resolve game version",
  下载与校验文件: "Download and verify files",
  准备原生库: "Prepare native libraries",
  "准备 Java": "Prepare Java",
  完成安装: "Finish installation",
  检查启动配置: "Check launch configuration",
  准备游戏身份: "Prepare player identity",
  文件下载进度: "File download progress",
  等待中: "Pending",
  下载中: "Downloading",
  校验中: "Verifying",
  未执行: "Not run",
  取消下载: "Cancel download",
  暂停下载: "Pause download",
  继续下载: "Resume download",
  已暂停: "Paused",
  已缓存: "Cached",
  "已校验 {done} / {total} 个文件": "Verified {done} / {total} files",
  通用设置: "General",
  网络设置: "Network",
  关于: "About",
  源代码: "Source code",
  反馈问题: "Report an issue",
  "已下载约 {progress}%": "About {progress}% downloaded",
  跟随系统: "Follow system",
  简体中文: "简体中文",
  账户管理: "Manage accounts",
  已登录: "Signed in",
  待刷新: "Refresh needed",
  未登录: "Not signed in",
  全局设置: "Settings",
  运行日志: "Logs",
  玩家皮肤预览: "Player skin preview",
  关闭错误提示: "Dismiss error",
  实例: "Instances",
  新建实例: "New instance",
  尚无版本: "No versions yet",
  请先新建实例: "Create an instance first",
  停止游戏: "Stop game",
  启动游戏: "Launch game",
  下载游戏: "Download game",
  下载并启动游戏: "Download & launch",
  "下载并启动 {version}": "Download and launch {version}",
  设置分类: "Settings categories",
  刷新账户与皮肤: "Refresh account and skin",
  日志: "Logs",
  关闭面板: "Close panel",
  "此开发版本还需要配置微软登录应用。Client ID 是启动器的公开应用编号，由开发者注册提供。":
    "This development build requires a Microsoft sign-in app. Its public Client ID is registered and provided by the developer.",
  "应用 ID（Client ID）": "Application ID (Client ID)",
  注册说明: "Registration guide",
  取消: "Cancel",
  保存并登录: "Save and sign in",
  完成微软授权: "Sign in with Microsoft",
  "在微软页面输入验证码，有效期约 {minutes} 分钟。":
    "Enter this code on the Microsoft page. It expires in about {minutes} minutes.",
  复制验证码: "Copy code",
  打开登录页: "Open sign-in page",
  取消登录: "Cancel sign-in",
  复制登录地址: "Copy sign-in URL",
  "等待授权…": "Waiting for sign-in…",
  选择启动账户: "Choose an account to play",
  当前账户: "Current account",
  移除账户: "Remove account",
  添加微软账户: "Add Microsoft account",
  配置登录应用: "Configure sign-in app",
  任务: "Tasks",
  下载设置: "Download settings",
  暂无下载任务: "No downloads yet",
  已完成: "Completed",
  失败: "Failed",
  已取消: "Cancelled",
  进行中: "In progress",
  语言: "Language",
  游戏启动时跟随: "Use when launching game",
  启动器版本: "Launcher version",
  正在检查更新: "Checking for updates",
  "下载更新 {progress}%": "Downloading update {progress}%",
  正在下载更新: "Downloading update",
  正在验证更新: "Verifying update",
  正在安装更新: "Installing update",
  正在重启启动器: "Restarting launcher",
  安装并重启: "Install and restart",
  "更新失败，点击重试": "Update failed · Retry",
  "最新版为 {version} · 更新并重启": "{version} available · Update and restart",
  "最新版为 {version} · 下载安装包": "{version} available · Download installer",
  "DEB 安装版请通过软件包管理器更新":
    "Update the DEB installation through your package manager",
  开发模式或未安装的程序请先安装发布版:
    "Install a release build to enable in-app updates",
  "检查失败，点击重试": "Check failed · Retry",
  你已经是最新版: "You're up to date",
  "最新版为 {version} · 点击更新": "{version} available · Update",
  打开启动器数据目录: "Open launcher data folder",
  启动器目录: "Launcher folder",
  使用系统应用数据目录: "Using the system app data folder",
  下载源: "Download source",
  游戏本体下载源: "Game download source",
  "Mod 下载源": "Mod download source",
  自动选择: "Automatic",
  官方来源: "Official servers",
  代理: "Proxy",
  不使用代理: "No proxy",
  手动设置: "Manual",
  代理地址: "Proxy address",
  "填写 HTTP、HTTPS 或 SOCKS5 地址，不含账号密码。":
    "Enter an HTTP, HTTPS or SOCKS5 address without credentials.",
  未知代理模式: "Unknown proxy mode",
  "代理地址须为 HTTP、HTTPS 或 SOCKS5 地址，包含有效端口且不含账号密码、路径或参数":
    "Use an HTTP, HTTPS or SOCKS5 proxy with a valid port and no credentials, path or query.",
  并发下载: "Parallel downloads",
  并发数: "Concurrency",
  自适应: "Adaptive",
  账号: "Username",
  密码: "Password",
  留空不验证: "Leave blank for no authentication",
  无法访问系统凭据存储: "Cannot access the system credential store",
  "无法读取代理密码，请重新填写密码":
    "Cannot read the proxy password. Please enter it again.",
  "无法保存代理密码，请检查系统凭据存储":
    "Cannot save the proxy password. Check the system credential store.",
  "代理密码记录无效，请重新填写密码":
    "Invalid proxy password entry. Please enter it again.",
  "代理账号和密码不能超过 255 字节":
    "Proxy username and password must not exceed 255 bytes",
  "{count} 个并发下载": "{count} concurrent downloads",
  代理类型: "Proxy type",
  失败重试: "Retry attempts",
  设置未能保存: "Could not save settings",
  重试: "Retry",
  保存: "Save",
  "{count} 条": "{count} entries",
  复制日志: "Copy logs",
  清空日志: "Clear logs",
  关闭日志: "Close logs",
  "暂无运行日志。": "No logs yet.",
  "重复 {count} 次": "Repeated {count} times",
  日志来源: "Log source",
  日志级别: "Log level",
  全部实例: "All instances",
  启动器: "Launcher",
  已移除实例: "Removed instance",
  所有级别: "All levels",
  警告和错误: "Warnings & errors",
  仅错误: "Errors only",
  搜索日志: "Search logs",
  搜索正文或来源: "Search text or source",
  复制筛选后的日志: "Copy filtered logs",
  清空此实例日志: "Clear this instance's logs",
  回到最新日志: "Jump to latest",
  "没有符合筛选条件的日志。": "No logs match these filters.",
  "{name}，{version}，双击查看运行日志，按 F2 设置，拖动或按 Alt 加左右方向键排序":
    "{name}, {version}. Double-click for logs, F2 for settings; drag or Alt + arrow keys to reorder",
  关闭对话框: "Close dialog",
  移除实例: "Remove instance",
  安装到: "Install to",
  更新版本列表: "Refresh version list",
  实例名称: "Instance name",
  "例如：日常游戏": "e.g. Survival",
  "Minecraft 版本": "Minecraft version",
  原版: "Vanilla",
  模组加载器: "Mod loader",
  "Fabric 版本": "Fabric version",
  稳定版: "Stable",
  测试版: "Preview",
  刷新版本列表: "Refresh versions",
  "正在获取 Fabric 版本…": "Loading Fabric versions…",
  "此游戏版本暂无可用的 Fabric":
    "Fabric is not available for this game version",
  "添加 Fabric 版本": "Add Fabric version",
  "支持原版 Java 1.13+，Apple Silicon 使用 1.19+。":
    "Supports vanilla Java 1.13+. Use 1.19+ on Apple Silicon.",
  游戏目录: "Game folder",
  "~/.minecraft 或游戏目录": "~/.minecraft or a game folder",
  浏览: "Browse",
  "直接使用原目录，不复制游戏文件。":
    "Uses the existing folder without copying game files.",
  最大内存: "Maximum memory",
  启动内存: "Game memory",
  最小: "Minimum",
  推荐: "Recommended",
  推荐配置: "Recommended",
  最小配置: "Minimum",
  自定义: "Custom",
  自定义内存: "Custom memory",
  启动时按可用内存重新计算: "Recalculated from available memory at launch",
  按游戏与模组估算: "Estimated for this game and its mods",
  "Java 可执行文件": "Java executable",
  自动: "Auto",
  "正在探测 Java…": "Detecting Java…",
  按游戏版本自动匹配: "Match the game version automatically",
  "手动指定的 Java": "Manually selected Java",
  "选择文件…": "Choose file…",
  "探测失败，仍可自动匹配或选择文件。":
    "Detection failed. Auto and file selection are still available.",
  留空自动匹配: "Leave blank to auto-detect",
  "选择 Java 可执行文件": "Choose Java executable",
  校验修复: "Verify & repair",
  导入已有目录: "Import existing folder",
  导入: "Import",
  下载并安装: "Download & install",
  创建实例: "Create instance",
  切换实例图标: "Change instance icon",
  实例图标: "Instance icon",
  修改实例名称: "Rename instance",
  读取中: "Loading",
  "读取目录…": "Loading folder…",
  目录不可用: "Folder unavailable",
  正在准备: "Preparing",
  "正在准备…": "Preparing…",
  玩家皮肤: "Player skin",
  "玩家皮肤，拖拽旋转；方向键左右旋转，Home 复位":
    "Player skin. Drag or use Left/Right arrows to rotate; Home to reset.",
  正在加载皮肤: "Loading skin",
  重新加载皮肤: "Reload skin",
  主题配色: "Theme colors",
  选择游戏版本: "Choose game version",
  选择或下载版本: "Choose or download a version",
  游戏版本: "Game versions",
  暂无游戏版本: "No game versions yet",
  待下载: "Not installed",
  下载新的版本: "Download a new version",
  导入实例: "Import instance",
  实例设置: "Instance settings",
  下载游戏版本: "Download game version",
  账户: "Accounts",
  下载任务: "Downloads",
  同步版本目录: "Sync version list",
  关联实例: "Link instance",
  添加版本: "Add version",
  保存实例: "Save instance",
  切换版本: "Switch version",
  校验与修复: "Verify & repair",
  安装游戏: "Install game",
  切换账户: "Switch account",
  刷新账户: "Refresh account",
  保存设置: "Save settings",
  已复制: "Copied",
  "选择 bin/java 或 bin/java.exe": "Choose bin/java or bin/java.exe",
  "账户暂无皮肤资料，请刷新账户。":
    "No skin data is available. Refresh the account.",
  "皮肤加载失败 · 显示 Steve": "Skin unavailable · Showing Steve",
  皮肤纹理无法读取: "Could not read the skin texture",
  "当前设备无法创建 3D 皮肤预览":
    "3D skin preview is unavailable on this device",
  "当前为界面预览。请运行桌面版以管理实例和启动游戏。":
    "This is a UI preview. Open the desktop app to manage instances and play.",
  "请等待当前任务完成。": "Wait for the current task to finish.",
  "正在取消登录…": "Cancelling sign-in…",
  "正在取消，已校验的文件将保留在缓存中。":
    "Cancelling. Verified files will remain cached.",
  "请在桌面启动器中登录，浏览器预览不支持微软授权。":
    "Sign in from the desktop launcher. Microsoft sign-in is unavailable in the browser preview.",
  "未能自动打开浏览器。请点击「打开登录页」重试，或复制下方登录地址到浏览器。":
    "Could not open your browser. Click “Open sign-in page” to retry, or copy the URL below into your browser.",
  "并发下载请填写 1–32 的整数。":
    "Parallel downloads must be an integer from 1 to 32.",
  "失败重试请填写 0–5 的整数。":
    "Retry attempts must be an integer from 0 to 5.",
  "无法访问剪贴板，请手动选择并复制。":
    "Clipboard unavailable. Select and copy the text manually.",
  "实例名称应为 1–80 个字符": "Instance names must contain 1–80 characters",
  "{name}，{version}，双击或按 F2 设置，拖动或按 Alt 加左右方向键排序":
    "{name}, {version}. Double-click or press F2 for settings. Drag or press Alt+Left/Right to reorder.",
  "{name}，{version}，双击查看下载进度，按 F2 设置，拖动或按 Alt 加左右方向键排序":
    "{name}, {version}. Double-click for download progress. Press F2 for settings. Drag or press Alt+Left/Right to reorder.",
  "为 {name} 下载游戏版本": "Download a game version for {name}",
  "启动 {version}": "Launch {version}",
  "使用 {name} 启动游戏": "Play as {name}",
  "移除账户 {name}": "Remove account {name}",
  "仅从列表移除「{name}」，原目录和游戏文件保留在原处。":
    "Remove “{name}” from the list. Its folder and game files will stay where they are.",
  "「{name}」将移至回收目录，游戏文件会保留。":
    "“{name}” will move to the trash folder. Its game files will be kept.",
  "{name} · 点击修改名称": "{name} · Click to rename",
  "打开游戏目录：{path}": "Open game folder: {path}",
  "{title}，{progress}，查看下载任务": "{title}, {progress}. View downloads",
  "{error}，点击重试": "{error}. Click to retry",
  "{name}主题，展开配色选项": "{name} theme. Show theme options",
  "{name}主题，{pattern}背景": "{name} theme, {pattern} background",
  "当前 {version} · 选择版本": "Current: {version} · Choose version",
  "Minecraft 官方": "Minecraft official",
  仅使用官方来源: "Official servers only",
  海蓝: "Tide",
  蜂巢: "Honeycomb",
  青苔: "Moss",
  方块矩阵: "Block matrix",
  蔷薇: "Rose",
  菱形晶格: "Diamond lattice",
  鸢尾: "Iris",
  三角切面: "Triangle facets",
  石墨: "Graphite",
  切角电路: "Chamfered circuit",
  草方块: "Grass block",
  冰块: "Ice",
  红砖: "Bricks",
  苔石: "Mossy cobblestone",
  橡木木板: "Oak planks",
  樱花木板: "Cherry planks",
  史莱姆方块: "Slime block",
  石砖: "Stone bricks",
  工作台: "Crafting table",
  砂岩: "Sandstone",
  石英块: "Quartz block",
  海晶石砖: "Prismarine bricks",
  切制铜块: "Cut copper",
  氧化切制铜块: "Oxidized cut copper",
  紫水晶块: "Amethyst block",
  干草块: "Hay bale",
};

export function t(
  key: string,
  values: Record<string, string | number> = {},
): string {
  const template =
    locale.value === "en-US" && Object.hasOwn(english, key)
      ? english[key]
      : key;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.hasOwn(values, name) ? String(values[name]) : match,
  );
}

export function translateMessage(message: string): string {
  if (locale.value !== "en-US") return message;
  return Object.hasOwn(english, message)
    ? english[message]
    : translateBackend(message);
}
