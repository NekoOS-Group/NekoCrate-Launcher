/** Launcher-owned messages only. Game output and user-supplied values are never translated. */
export const backendEnglish: Readonly<Record<string, string>> = {
  当前启动器版本无效: "Invalid launcher version",
  "暂无可识别的公开版本，请稍后重试":
    "No supported public release found. Try again later",
  发布地址无效: "Invalid release URL",
  无法连接更新服务器: "Could not connect to the update server",
  "更新检查次数受限，请稍后重试":
    "Update checks are rate-limited. Try again later",
  更新服务器返回错误: "The update server returned an error",
  无法读取更新信息: "Could not read update information",
  操作已取消: "Operation cancelled",
  当前没有可暂停的下载任务: "There is no download to pause",
  下载任务已结束: "The download has finished",
  校验本地文件: "Verifying local files",
  下载缺失文件: "Downloading missing files",
  复用缓存并下载缺失文件: "Reusing cached files and downloading missing files",
  "保存文件校验记录失败：{error}":
    "Could not save file verification records: {error}",
  "下载源未支持续传，重新下载此文件：{0}":
    "The source does not support resuming; restarting this file: {0}",
  "下载源拒绝续传范围，重新下载此文件：{0}":
    "The source rejected the resume range; restarting this file: {0}",
  "无法获取元数据，请检查网络或切换下载源：{last}":
    "Could not fetch metadata. Check your connection or change the download source: {last}",
  资源校验值无效: "Invalid resource checksum",
  文件大小超过元数据声明: "File exceeds the size declared in metadata",
  下载文件校验失败: "Downloaded file failed verification",
  下载源返回的续传范围无效:
    "The download source returned an invalid resume range",
  "保存下载进度失败：{error}": "Could not save download progress: {error}",
  "重试 {0}（{1}/{2}）": "Retrying {0} ({1}/{2})",
  "下载失败 {0}：{error}": "Download failed for {0}: {error}",
  无效下载目标: "Invalid download target",
  "Java 检测超时": "Java detection timed out",
  "无法执行 Java": "Could not execute Java",
  "Java 无法运行：{0}": "Java could not run: {0}",
  "无法识别 Java 版本，请选择标准 Java 可执行文件":
    "Unrecognized Java version. Choose a standard Java executable",
  "Java 版本不完整": "Incomplete Java version",
  "此游戏需要 Java {major}，所选路径是 Java {0}。请切换 Java 或清空路径启用自动匹配。":
    "This game needs Java {major}; the selected path uses Java {0}. Change Java or clear the path to auto-detect.",
  "未找到 Java {major}，请指定 Java 路径或在设置中允许自动下载。":
    "Java {major} was not found. Specify a Java path or enable automatic downloads.",
  "正在安装 Java {major}": "Installing Java {major}",
  "此架构暂不支持自动安装 Java，请手动配置":
    "Automatic Java installation is unavailable for this architecture. Configure Java manually",
  "暂无适合此系统的 Java JRE，请指定本地 Java 路径":
    "No suitable Java runtime found for this system. Specify a local Java path",
  "Java 下载信息缺失": "Missing Java download information",
  "Java 校验值缺失": "Missing Java checksum",
  "解压后未找到 Java 可执行文件": "Java executable not found after extraction",
  "安装后的 Java 版本与要求不匹配":
    "Installed Java version does not match the required version",
  "Java 压缩包含越界路径":
    "Java archive contains a path outside its destination",
  "Java ZIP 中含不支持的符号链接":
    "Java ZIP contains unsupported symbolic links",
  "Java 压缩包包含越界路径":
    "Java archive contains a path outside its destination",
  "此微软账户尚未创建 Xbox 档案。请先登录 xbox.com 完成设置，再重试。":
    "This Microsoft account has no Xbox profile. Sign in at xbox.com to finish setup, then try again.",
  "该地区的 Xbox Live 不可用。请检查账户地区设置。":
    "Xbox Live is unavailable in this region. Check your account region.",
  "Xbox 要求完成年龄或身份验证。请在官方账户页面完成验证。":
    "Xbox requires age or identity verification. Complete it on the official account page.",
  "此账户需要监护人在 Microsoft 家庭设置中允许 Xbox 游戏服务。":
    "A parent or guardian must allow Xbox gaming in Microsoft Family settings.",
  "登录授权已被拒绝，请重新开始并在微软页面确认授权。":
    "Sign-in was denied. Start again and approve access on the Microsoft page.",
  "登录验证码已过期，请重新获取验证码。":
    "The sign-in code has expired. Request a new code.",
  "登录凭据已过期或被撤销，请移除账户并重新登录。":
    "Sign-in credentials have expired or been revoked. Remove the account and sign in again.",
  "Client ID 无效或应用未启用公共客户端流程，请检查微软应用注册配置。":
    "Invalid Client ID or public client flow disabled. Check the Microsoft app registration.",
  "微软应用未接受 XboxLive.signin 权限，请检查应用配置。":
    "The Microsoft app did not accept XboxLive.signin permission. Check the app configuration.",
  "身份服务未能完成请求，请检查网络、应用权限和 Xbox 账户状态后重试。":
    "Sign-in could not complete. Check your connection, app permissions and Xbox account status, then retry.",
  身份服务返回了无法解析的响应:
    "The identity service returned an unreadable response",
  "Minecraft 登录": "Minecraft sign-in",
  "Minecraft 拒绝此应用登录。请确认 Client ID 已获 Minecraft API 访问许可，或稍后重试。（HTTP 403）":
    "Minecraft rejected this app. Check that its Client ID has Minecraft API access, or try again later. (HTTP 403)",
  "Minecraft 档案": "Minecraft profile",
  "未找到 Java 版游戏档案，请确认账户拥有 Java 版并在 minecraft.net 创建玩家名称。":
    "No Java Edition profile found. Make sure the account owns Java Edition and has a player name at minecraft.net.",
  "{stage}失败（HTTP {status}）：{detail}":
    "{stage} failed (HTTP {status}): {detail}",
  "请先在设置中填写你自己的 Microsoft 应用 Client ID（UUID），无需 Client Secret":
    "Configure your Microsoft app Client ID (UUID) first. A Client Secret is not required",
  无法连接微软身份服务: "Could not connect to Microsoft sign-in",
  获取验证码: "Request sign-in code",
  微软返回了无效登录地址: "Microsoft returned an invalid sign-in URL",
  登录已取消: "Sign-in cancelled",
  微软登录轮询失败: "Microsoft sign-in polling failed",
  "无法刷新微软登录，请检查网络":
    "Could not refresh Microsoft sign-in. Check your connection",
  刷新登录: "Refresh sign-in",
  "Xbox 登录": "Xbox sign-in",
  "Xbox 未返回登录令牌": "Xbox did not return a sign-in token",
  "Xbox 游戏授权": "Xbox game authorization",
  "Xbox 未返回游戏令牌": "Xbox did not return a game token",
  "Xbox 未返回用户标识": "Xbox did not return a user ID",
  "Minecraft 未返回访问令牌": "Minecraft did not return an access token",
  游戏权益检查: "Game ownership check",
  "此微软账户没有可用的 Minecraft Java 版权益，请检查购买账户或 Game Pass 订阅状态。":
    "This Microsoft account has no active Minecraft Java Edition entitlement. Check the purchasing account or Game Pass subscription.",
  "档案缺少 UUID": "Profile is missing its UUID",
  "Minecraft 档案 UUID 无效": "Invalid Minecraft profile UUID",
  档案缺少玩家名称: "Profile is missing its player name",
  "微软未返回刷新令牌，请确认 offline_access 权限":
    "Microsoft did not return a refresh token. Check offline_access permission",
  系统凭据存储不可用: "System credential storage is unavailable",
  "无法写入系统钥匙串；登录信息未保存。请解锁钥匙串或启用 Secret Service":
    "Could not save sign-in credentials. Unlock the system keychain or enable Secret Service",
  "系统钥匙串中没有可用凭据，请重新登录":
    "No credentials found in the system keychain. Sign in again",
  无法移除系统钥匙串凭据:
    "Could not remove credentials from the system keychain",
  皮肤地址无效: "Invalid skin URL",
  "皮肤地址不是 Minecraft 纹理服务":
    "Skin URL is not hosted by the Minecraft texture service",
  皮肤纹理路径无效: "Invalid skin texture path",
  皮肤纹理标识无效: "Invalid skin texture ID",
  "无法升级皮肤地址到 HTTPS": "Could not upgrade the skin URL to HTTPS",
  "皮肤文件不是有效的 PNG": "Skin file is not a valid PNG",
  皮肤尺寸不受支持: "Unsupported skin dimensions",
  "皮肤服务返回重定向，未加载纹理":
    "The skin service returned a redirect. Texture not loaded",
  皮肤文件过大: "Skin file is too large",
  不支持的游戏语言: "Unsupported game language",
  无法读取游戏语言设置: "Could not read game language settings",
  无法保存游戏语言设置: "Could not save game language settings",
  "不支持的数据格式版本 {schema}": "Unsupported data format version {schema}",
  实例数据无效: "Invalid instance data",
  旧实例缺少版本: "Legacy instance is missing its version",
  无法读取实例数据: "Could not read instance data",
  "实例 {0} 的关联目录必须是绝对路径":
    "The linked directory for instance {0} must be an absolute path",
  "实例 {0} 的所选版本不存在": "Selected version is missing from instance {0}",
  "实例 {0} 同时存在 game 与 .minecraft，未覆盖任何游戏文件":
    "Instance {0} contains both game and .minecraft folders. No game files were overwritten",
  "无法读取 {0}": "Could not read {0}",
  "JSON 损坏：{0}，请保留原文件后恢复备份":
    "Corrupt JSON: {0}. Keep the original file before restoring a backup",
  路径没有父目录: "Path has no parent directory",
  "非法相对路径：{value}": "Invalid relative path: {value}",
  "路径不能离开实例目录：{value}":
    "Path must stay within the instance directory: {value}",
  游戏版本不合法: "Invalid game version",
  "实例 ID 无效": "Invalid instance ID",
  无法确定用户主目录: "Could not determine the user home folder",
  迁移目录不存在: "Migration directory does not exist",
  目标目录不能位于来源目录内:
    "Destination must not be inside the source folder",
  "迁移目录含符号链接，请先将链接替换为实际文件：{0}":
    "The source contains symbolic links. Replace them with actual files first: {0}",
  "网络不可用，使用已缓存版本目录":
    "Network unavailable. Using the cached version list",
  "未找到该官方游戏版本；原型暂不支持自定义加载器版本":
    "Official game version not found. Custom loaders are not supported yet",
  "未在官方版本目录中找到此版本；自定义加载器版本暂不支持":
    "This version is not in the official version list. Custom loaders are not supported yet",
  "该本地版本使用自定义加载器，启动器暂不支持安装或修复":
    "This local version uses a custom loader. Installing or repairing it is not supported yet",
  无效游戏版本: "Invalid game version",
  "当前原型仅支持独立的官方 Java 版元数据":
    "This prototype supports standalone official Java Edition metadata only",
  "当前原型支持 Minecraft 1.13 及更新的官方版本；旧版本启动协议将在后续加入。":
    "This prototype supports official Minecraft 1.13+ releases. Older launch protocols are not supported yet.",
  "此版本未提供 Apple Silicon 原生库；原型在 Apple Silicon 上请使用 1.19 或更新版本。":
    "This version has no Apple Silicon native libraries. Use Minecraft 1.19+ on Apple Silicon.",
  库文件缺少路径: "Library file is missing its path",
  元数据缺少客户端文件: "Metadata is missing the game client",
  资源索引校验值无效: "Invalid asset index checksum",
  "日志配置 ID 无效": "Invalid log configuration ID",
  "日志配置 URL 无效": "Invalid log configuration URL",
  日志配置校验值无效: "Invalid log configuration checksum",
  "校验并下载 {0} 个游戏文件": "Verifying and downloading {0} game files",
  原生库含越界路径: "Native library contains a path outside its destination",
  原生库包含不支持的符号链接:
    "Native library contains unsupported symbolic links",
  不支持旧版本启动参数: "Legacy launch arguments are not supported",
  启动参数不是字符串: "Launch argument is not a string",
  无法识别的启动参数格式: "Unrecognized launch argument format",
  版本使用了尚未支持的启动参数占位符:
    "This version uses an unsupported launch argument placeholder",
  "有任务正在运行，请等待或取消当前任务":
    "A task is running. Wait for it to finish or cancel it",
  请先获取登录验证码: "Request a sign-in code first",
  无法确定数据目录: "Could not determine the data folder",
  "该数据目录已被另一个 NekoCrate 进程占用，请先关闭它":
    "Another NekoCrate process is using this data folder. Close it first",
  "游戏运行中，请先停止该实例再修改或修复":
    "Stop this instance before changing or repairing it",
  实例不存在: "Instance not found",
  "目标实例不存在，请刷新后重试":
    "Target instance not found. Refresh and retry",
  "关联的游戏目录不可用，请恢复原路径或重新添加目录：{0}":
    "Linked game folder unavailable. Restore the original path or add it again: {0}",
  "有任务正在运行，请稍后添加版本":
    "A task is running. Add the version when it finishes",
  "有任务正在运行，请稍后切换版本":
    "A task is running. Switch versions when it finishes",
  此实例未添加该版本: "This version has not been added to the instance",
  实例中不存在该版本: "Version not found in this instance",
  "有任务正在运行，请完成后修改设置":
    "A task is running. Change settings when it finishes",
  "并发应为 1–32，重试次数应为 0–5":
    "Parallel downloads must be 1–32; retry attempts must be 0–5",
  未知下载源: "Unknown download source",
  不支持的语言选项: "Unsupported language preference",
  "Client ID 应为 UUID": "Client ID must be a UUID",
  "有任务正在运行，请稍后创建实例":
    "A task is running. Create the instance when it finishes",
  "名称应为 1–80 个字符，内存应为 512–65536 MiB":
    "Name must contain 1–80 characters; memory must be 512–65536 MiB",
  "有任务正在运行，请稍后修改实例":
    "A task is running. Edit the instance when it finishes",
  请选择有效的实例图标: "Choose a valid instance icon",
  "无法访问导入目录：{0}": "Could not access the import folder: {0}",
  "导入来源必须是一个目录：{0}": "Import source must be a folder: {0}",
  "请选择含 saves、mods、versions 或 options.txt 的游戏目录":
    "Choose a game folder containing saves, mods, versions or options.txt",
  "游戏目录不能与启动器数据目录重叠，请选择原有的独立游戏目录":
    "The game folder must not overlap the launcher data folder. Choose a separate existing game folder",
  "此目录已由实例「{0}」使用，不能重复添加或嵌套关联":
    "This folder is already used by instance “{0}”. Duplicate or nested links are not allowed",
  "有任务正在运行，请稍后移除实例":
    "A task is running. Remove the instance when it finishes",
  请先为此实例添加并选择一个版本:
    "Add and select a version for this instance first",
  获取官方版本元数据: "Fetching official version metadata",
  下载与校验游戏文件: "Downloading and verifying game files",
  保存安装信息: "Saving installation details",
  "游戏文件与 Java 已准备好": "Game files and Java are ready",
  账户不存在: "Account not found",
  "有任务正在使用账户，请稍后移除":
    "A task is using this account. Remove it when the task finishes",
  请先添加并选择微软账户: "Add and select a Microsoft account first",
  所选账户不存在: "Selected account not found",
  "刷新后账户发生变化，请重新登录":
    "Account changed after refreshing. Sign in again",
  "以未登录身份 Player 启动": "Launching as Player without signing in",
  "刷新登录并验证 Java 版权益":
    "Refreshing sign-in and verifying Java Edition ownership",
  请先选择游戏版本: "Choose a game version first",
  请先安装或修复所选版本: "Install or repair the selected version first",
  "版本元数据与所选版本不一致，请修复该版本":
    "Metadata does not match the selected version. Repair this version",
  启动已取消: "Launch cancelled",
  "无法启动 Java 进程，请检查 Java 路径和系统权限":
    "Could not start Java. Check the Java path and system permissions",
  "无法读取游戏进程 ID": "Could not read the game process ID",
  "游戏已启动 · PID {pid}": "Game started · PID {pid}",
  游戏已正常退出: "Game exited normally",
  "游戏已退出（状态 {s}）。如非主动停止，请查看日志，检查 Java、内存和游戏配置。":
    "Game exited (status {s}). If this was unexpected, check the logs, Java, memory and game settings.",
  "读取游戏进程状态失败：{e}": "Could not read game process status: {e}",
  此实例未运行: "This instance is not running",
  目录选择窗口未返回结果: "The folder picker did not return a result",
  目录选择窗口必须在主线程打开:
    "The folder picker must open on the main thread",
  无法读取所选目录路径: "Could not read the selected folder path",
};

const escapeRegex = (value: string) =>
  value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const patterns = Object.entries(backendEnglish).map(([source, target]) => {
  const names: string[] = [];
  const pattern = source
    .split(/(\{\w+\})/)
    .map((part) => {
      if (/^\{\w+\}$/.test(part)) {
        names.push(part.slice(1, -1));
        return "(.*?)";
      }
      return escapeRegex(part);
    })
    .join("");
  return { regex: new RegExp(`^${pattern}(?:: (.*))?$`, "s"), names, target };
});

export function translateBackend(message: string, depth = 0): string {
  if (depth > 6 || !/\p{Script=Han}/u.test(message)) return message;
  if (Object.hasOwn(backendEnglish, message)) return backendEnglish[message];
  for (const { regex, names, target } of patterns) {
    const match = regex.exec(message);
    if (!match) continue;
    const values = new Map(
      names.map((name, index) => [name, match[index + 1]]),
    );
    const translated = target.replace(/\{(\w+)\}/g, (_, name: string) => {
      const value = values.get(name) ?? "";
      return ["stage", "last", "error", "e"].includes(name)
        ? translateBackend(value, depth + 1)
        : value;
    });
    const cause = match[names.length + 1];
    return (
      translated + (cause ? `: ${translateBackend(cause, depth + 1)}` : "")
    );
  }
  return message;
}
