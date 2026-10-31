/** Launcher-owned messages only. Game output and user-supplied values are never translated. */
export const backendEnglish: Readonly<Record<string, string>> = {
  "环境变量名称重复：{name}": "Duplicate environment variable: {name}",
  "Java 版玩家名称须为 3–16 位英文字母、数字或下划线":
    "Use 3–16 letters, numbers or underscores for your Java Edition name",
  "改名请求未完成，请刷新账户确认结果后再试":
    "The name change request did not complete; refresh the account to confirm the result before retrying",
  "无法检查玩家名称，请检查网络后重试":
    "Could not check the player name; check your connection and retry",
  "Minecraft 未接受此名称，请检查名称是否可用及账户改名限制":
    "Minecraft rejected this name; check availability and account name change restrictions",
  "此账户暂时无法改名，请检查官方改名冷却期或账户权限":
    "This account cannot change its name right now; check the official cooldown or account permissions",
  "此玩家名称已被使用，请换一个名称":
    "This player name is already in use; choose another name",
  "改名请求过于频繁，请稍后重试":
    "Too many name change requests; try again later",
  "玩家改名服务请求失败（HTTP {status}）":
    "Player name service request failed (HTTP {status})",
  玩家改名服务响应过大: "The player name service response is too large",
  玩家改名服务返回了无法解析的响应:
    "The player name service returned an invalid response",
  "此账户尚未到可改名时间，Java 版每 30 天可改名一次":
    "This account is still in its name change cooldown; Java Edition names can be changed once every 30 days",
  玩家改名服务未返回改名资格:
    "The player name service did not return name change eligibility",
  "Minecraft 不允许使用此玩家名称，请换一个名称":
    "Minecraft does not allow this player name; choose another name",
  "无法确认此玩家名称是否可用，请稍后重试":
    "Could not confirm name availability; try again later",
  玩家改名服务未返回账户信息:
    "The player name service did not return an account",
  玩家改名服务返回的账户不匹配:
    "The player name service returned a different account",
  玩家改名服务未返回新名称:
    "The player name service did not return the new name",
  "玩家改名服务返回的名称不匹配，请刷新账户确认结果":
    "The player name service returned a different name; refresh the account to confirm the result",
  "登录凭据已失效，请重新登录后修改玩家名称":
    "Your session is no longer valid; sign in again to change your player name",
  "正版账户皮肤须为 64×64 或 64×32 的 PNG":
    "Account skins must be 64×64 or 64×32 PNG images",
  "无法上传皮肤，请检查网络后重试":
    "Could not upload the skin; check your connection and retry",
  "Minecraft 未接受此皮肤，请检查 PNG 文件和手臂模型":
    "Minecraft rejected the skin; check the PNG file and arm model",
  "Minecraft 拒绝更换皮肤，请检查账户权限或重新登录":
    "Minecraft denied the skin change; check account permissions or sign in again",
  "皮肤切换过于频繁，请稍后重试":
    "Too many skin changes; please try again later",
  "皮肤上传失败（HTTP {status}），请稍后重试":
    "Skin upload failed (HTTP {status}); please try again later",
  "登录凭据已失效，请重新登录后更换皮肤":
    "Your session is no longer valid; sign in again to change skins",
  皮肤服务响应过大: "The skin service response is too large",
  皮肤服务返回了无法解析的响应: "The skin service returned an invalid response",
  皮肤服务未返回账户信息: "The skin service did not return an account",
  皮肤服务返回的账户不匹配: "The skin service returned a different account",
  皮肤服务未返回已应用的皮肤:
    "The skin service did not return the applied skin",
  "以游客身份 {name} 启动": "Launching as guest {name}",
  "游客名称须为 1–16 位英文字母、数字或下划线":
    "Use 1–16 letters, numbers or underscores for the guest name",
  此资源类型不支持跨版本复制:
    "This resource type cannot be copied between versions",
  "下载连接连续 {seconds} 秒未收到数据":
    "No data received for {seconds} seconds",
  "下载连接连续 {seconds} 秒未收到数据；保留 {bytes} 字节并重新连接：{file}":
    "No data received for {seconds} seconds; reconnecting with {bytes} bytes retained: {file}",
  "未找到对应的 Mod 下载声明，请刷新列表":
    "Mod download declaration not found; refresh the list",
  "此声明对应的文件不是 Mod": "This declaration does not refer to a mod",
  "此 Mod 没有可自动下载的地址，请从作者页面下载后拖入此版本":
    "This mod has no automatic download URL; download it from the project page and drop it into this version",
  "Mod 下载声明缺少有效校验值或文件大小超限":
    "The mod declaration has an invalid hash or exceeds the file size limit",
  "Mod 文件路径缺失": "Mod file path is missing",
  "Mod 下载目标必须是 mods 目录内的 JAR 文件":
    "The mod destination must be a JAR inside the mods directory",
  "同名 Mod 文件已存在且内容不同，未覆盖":
    "A different mod file already has this name; it was not overwritten",
  "无法写入 Mod 文件，已有文件未被覆盖":
    "Unable to publish the mod file; existing files were not overwritten",

  "JSON 配置格式错误": "Invalid JSON configuration",
  "TOML 配置格式错误": "Invalid TOML configuration",
  "配置不是 UTF-8 文本，请使用外部编辑器":
    "Configuration is not UTF-8 text. Use an external editor",
  "文件任务正在运行，请完成后修改 Mod":
    "Wait for the file task to finish before modifying mods",
  "文件任务正在运行，请完成后保存配置":
    "Wait for the file task to finish before saving configuration",
  来源备份不能是符号链接: "The provenance backup must not be a symbolic link",
  "{count} 个文件下载失败，其余文件已保留。{cause}":
    "{count} files failed. Other completed files were retained. {cause}",
  目录尚不存在: "The directory does not exist yet",
  此项目不是整合包: "This project is not a modpack",
  "作者未允许通过第三方启动器下载，请在项目主页下载后导入":
    "The author has not allowed third-party downloads. Download from the project page, then import the file",
  此版本没有唯一的整合包文件:
    "This version does not identify a unique modpack archive",
  "此文件未提供下载地址，请在项目主页下载后导入":
    "No download URL is available. Download from the project page, then import the file",
  整合包下载地址无效: "Invalid modpack download URL",
  整合包文件大小或校验信息无效: "Invalid modpack size or checksum",
  整合包版本不属于所选项目:
    "The modpack version does not belong to the selected project",
  整合包内容与下载平台不符:
    "The archive format does not match the selected platform",
  资源搜索结果格式无效: "Invalid resource search response",

  "请分别导入资源、整合包或游戏目录":
    "Import resources, modpacks and game directories separately",
  "请先选择目标游戏版本，或将资源拖到版本列表中的指定版本":
    "Select a game version first, or drop resources onto a specific version in the list",
  "此构建暂不支持 CurseForge，请使用官方发行版":
    "CurseForge is unavailable in this build. Please use an official release",
  "CurseForge 服务暂不可用，请稍后重试或更新启动器":
    "CurseForge is currently unavailable. Try again later or update the launcher",
  "资源平台请求过于频繁，请稍后重试":
    "Too many requests to the resource platform. Try again later",
  "此版本尚未安装 Mod 加载器":
    "This version does not have a mod loader installed",
  请先选择要安装数据包的存档: "Choose a world for the data pack first",
  目标存档不存在: "The target world no longer exists",
  "资源在安装期间发生变化：{path}":
    "The resource changed during installation: {path}",
  "源文件在预览后发生变化，请重新导入":
    "The source changed after the preview. Import it again",
  "源文件在导入时发生变化，请重新导入":
    "The source changed during import. Import it again",
  "目标已存在且内容不同：{path}":
    "The destination already exists with different content: {path}",
  "文件名大小写冲突：{name}": "File names differ only in letter case: {name}",
  "多个资源使用相同文件名：{name}":
    "Multiple resources use the same file name: {name}",
  "已安装此项目的其他文件：{path}":
    "Another file from this project is already installed: {path}",
  "Mod ID 冲突：{details}": "Duplicate mod ID: {details}",
  "本地 Mod 的游戏版本与前置依赖请确认：{name}":
    "Check the game version and dependencies of this local mod: {name}",
  "Mod 加载器不匹配：{details}": "Incompatible mod loader: {details}",
  "光影需要游戏中已有兼容的 Iris 或 OptiFine":
    "Shaders require a compatible Iris or OptiFine installation",
  "资源与游戏或加载器版本不兼容：{name}":
    "The resource is incompatible with the game or loader version: {name}",
  "依赖要求不同版本：{details}":
    "Dependencies require conflicting versions: {details}",
  "存在声明不兼容的资源：{name}":
    "A declared incompatible resource is present: {name}",
  "必要依赖没有兼容版本：{project}":
    "No compatible version for required dependency: {project}",
  "作者未允许通过第三方启动器下载：{name}":
    "The author has not allowed downloads through third-party launchers: {name}",
  "作者未提供可用下载地址，请从项目页面下载后拖入":
    "No download URL is available. Download from the project page and drop the file here",
  "目标版本已变化，请重新生成资源安装计划":
    "The target profile changed. Generate a new installation preview",
  请先解决安装预览中的冲突并重新生成计划:
    "Resolve the conflicts and generate a new installation preview",
  "无法判断资源类型，请选择资源包或数据包":
    "Choose whether to import this as a resource pack or a data pack",
  "不能导入符号链接，请选择实际文件或目录":
    "Choose the actual file or directory instead of a symbolic link",
  "HTTPS 证书不在有效期内，请检查系统日期、时间和时区并开启自动同步；若时间正确，可能是服务器证书异常":
    "The HTTPS certificate is outside its validity period. Check your system date, time and time zone and enable automatic time sync. If your clock is correct, the server certificate may be at fault",
  "HTTPS 证书验证失败，请检查代理或网络的证书配置":
    "HTTPS certificate verification failed. Check your proxy or network certificate configuration",
  安装加载器: "Installing loader",
  游戏与加载器已安装: "Minecraft and loader installed",
  加载器安装记录与版本目录不匹配:
    "Loader installation record does not match its version directory",
  "所选加载器不支持此游戏版本，请重新选择":
    "The selected loader does not support this Minecraft version. Select another version",
  "同名版本已经存在，请保留原版本或选择其他加载器版本":
    "A version with this name already exists. Keep it or select another loader version",
  "同名版本已经存在，未覆盖原目录":
    "A version with this name already exists; the original directory was preserved",
  "本地版本配置已改变，未覆盖已有加载器":
    "The local profile has changed; the existing loader was preserved",
  安装器生成的加载器与所选版本不一致:
    "The installed loader does not match the selected version",
  "NeoForge 版本目录格式无效": "Invalid NeoForge version catalog",
  "运行加载器安装步骤 {step}": "Running loader installation step {step}",
  "加载器安装步骤失败：{error}": "Loader installation step failed: {error}",
  "加载器生成文件校验失败：{path}":
    "Generated loader file verification failed: {path}",

  "此文件尚未下载，暂无本地元数据":
    "This file has not been downloaded; local metadata is unavailable",
  不支持此加载器安装器格式: "This loader installer format is not supported",
  "暂不支持整合包加载器：{loader}": "Unsupported modpack loader: {loader}",
  "整合包包含不安全的路径：{path}": "Unsafe modpack path: {path}",
  "请选择含有唯一 Modrinth 或 CurseForge 清单的整合包":
    "Select a pack containing exactly one Modrinth or CurseForge manifest",
  整合包已安装: "Modpack installed",
  "版本名称重复：{name}，请修改整合包名称":
    "Version name already exists: {name}. Choose a different modpack name",
  "整合包名称不能用作版本目录：{name}":
    "The modpack name cannot be used as a version folder: {name}",
  "整合包名称与保留文件冲突：{name}":
    "The modpack name conflicts with a reserved file: {name}",
  "整合包名称不能与基础游戏版本相同：{version}":
    "The modpack name must differ from the base Minecraft version: {version}",
  整合包不能覆盖版本配置或原始清单:
    "The modpack cannot overwrite its launch profile or original manifest",
  "无法读取整合包清单：{error}": "Could not read the modpack manifest: {error}",
  "原始整合包缓存不存在，无法迁移":
    "The original modpack archive is missing; migration could not finish",
  整合包缓存路径无效: "Invalid modpack cache path",
  "版本目录中的整合包缓存与原记录不一致，未覆盖文件":
    "The local modpack archive differs from the old record. No files were overwritten",
  "保存迁移结果失败，版本文件保留在：{path}":
    "Could not save the migration. Version files remain at: {path}",
  完成整合包安装: "Finishing modpack installation",
  安装游戏与加载器: "Installing Minecraft and loader",
  写入整合包配置: "Applying modpack overrides",
  下载整合包文件: "Downloading modpack files",
  解析整合包依赖: "Resolving modpack dependencies",
  解析并下载整合包文件: "Resolving and downloading modpack files",
  复用已解析的文件信息: "Reusing resolved file metadata",
  准备获取整合包文件信息: "Preparing modpack file metadata",
  "获取项目资料与下载地址（第 {batch} / {total} 批）":
    "Fetching project details and download URLs (batch {batch} / {total})",
  "获取整合包依赖失败（第 {batch} / {total} 批）":
    "Failed to fetch modpack dependencies (batch {batch} / {total})",
  "已解析：{name}": "Resolved: {name}",
  "等待 CurseForge 响应（第 {batch} / {total} 批，{seconds} 秒）":
    "Waiting for CurseForge (batch {batch} / {total}, {seconds}s)",
  "部分文件需手动导入，其余文件继续下载":
    "Some files need a manual import; other files are still downloading",
  "自动安装已完成，等待手动导入文件":
    "Automatic installation finished; waiting for manual files",
  所选文件与整合包声明的文件校验值不匹配:
    "The selected files do not match the modpack's declared checksums",
  "此整合包有 {count} 个文件需要从作者页面手动下载，请补齐后继续":
    "This modpack requires {count} files to be downloaded from their project pages; add them to continue",
  此文件不在当前版本的内容列表中:
    "This file is not in the selected version\u2019s content list",
  内容路径不能离开所选目录:
    "Content paths must stay inside the selected directory",
  元数据内容过大: "Metadata is too large",
  "Mod 描述缺少 mods 列表": "Mod metadata is missing the mods list",
  "Mod 描述格式无效": "Invalid mod metadata format",
  "未找到 pack.mcmeta": "pack.mcmeta was not found",
  "pack.mcmeta 缺少 pack 配置": "pack.mcmeta is missing its pack section",
  "元数据文件超过 8 MiB": "Metadata file exceeds 8 MiB",
  "元数据文件必须是 JSON 对象": "Metadata file must be a JSON object",
  版本继承存在循环或层数过多: "Version inheritance is circular or too deep",
  版本元数据与所选版本不一致: "Profile does not match the selected version",
  "版本配置超过 8 MiB": "Profile exceeds 8 MiB",
  路径不能离开版本目录: "Path must stay within the version folder",
  "子实例名称应为 1–80 个字符，且不能包含控制字符":
    "Version names must contain 1–80 characters and no control characters",
  "有任务正在运行，请稍后删除版本":
    "A task is running. Try deleting the version after it finishes",
  "无法检查版本依赖：{version}":
    "Could not check version dependencies: {version}",
  "版本 {version} 仍依赖此版本，请先删除依赖它的子实例":
    "Version {version} still depends on this version. Remove the dependent version first",
  "无法移除版本目录，请关闭占用这些文件的程序":
    "Could not move the version directory. Close programs using its files",
  "保存删除结果失败，版本文件保留在：{path}":
    "Could not save the deletion. Version files remain at: {path}",
  "无法移入系统回收站，请检查权限或关闭占用文件的程序":
    "Could not move files to the system Trash / Recycle Bin. Check permissions or close programs using the files",
  "回收失败且无法恢复列表记录，原文件位于：{path}；{error}":
    "Recycling failed and the list record could not be restored. Original files remain at: {path}; {error}",
  无法读取游戏内容目录: "Could not read the game content directory",
  "无法读取 Mod 目录": "Could not read the mods directory",
  请选择外部文件夹: "Choose an external folder",
  "无法访问外部文件夹，请重新选择":
    "Cannot access the external folder; choose it again",
  "该位置已有 .minecraft，请导入已有目录或选择其他位置":
    "This location already contains .minecraft; import the existing folder or choose another location",
  "无法创建游戏目录，请检查文件夹权限":
    "Cannot create the game folder; check folder permissions",
  请等待当前任务完成后关联:
    "Wait for the current task to finish before linking",
  "有任务正在运行，请稍后关联实例":
    "A task is running. Try linking instances when it finishes",
  "正在关联实例，请稍候": "Linking instances; please wait",
  请拖入文件夹: "Drop a folder",
  已跳过启动器数据目录: "Skipped the launcher data directory",
  "无法访问目录：{path}": "Cannot access directory: {path}",
  关联实例任务失败: "Could not complete instance linking",
  "所选 Java {major} 超出 {component} 的字节码支持范围（最高 Java {maximum}）。请切换为 Auto 或选择兼容的 Java。":
    "Selected Java {major} exceeds {component}'s bytecode support (up to Java {maximum}). Select Auto or a compatible Java version.",
  "应用 macOS JNA 兼容修复": "Applying the macOS JNA compatibility fix",
  "应用 macOS OpenAL 兼容修复": "Applying the macOS OpenAL compatibility fix",
  "准备旧版 Minecraft 颜色兼容补丁":
    "Preparing the legacy Minecraft color compatibility patch",
  "无法准备旧版 Minecraft 颜色兼容补丁":
    "Could not prepare the legacy Minecraft color compatibility patch",
  无法检查旧版游戏的窗口兼容性:
    "Could not inspect legacy game window compatibility",
  "已启用旧版 Minecraft 颜色兼容补丁：{patch}":
    "Legacy Minecraft color compatibility patch enabled: {patch}",
  "Java 自动匹配范围：{runtime}": "Automatic Java compatibility: {runtime}",
  "版本配置明确声明了 Java 兼容列表":
    "The version profile explicitly declares compatible Java versions",
  "缺少更新 Java 的兼容依据，保留游戏声明的版本":
    "No evidence for newer Java compatibility; keeping the game's declared version",
  "{kind} {version} 的已核实 Java 支持范围":
    "Verified Java support for {kind} {version}",
  "LaunchWrapper 使用旧类加载器，自动匹配限制为 Java 8":
    "Legacy LaunchWrapper restricts automatic selection to Java 8",
  "ASM {version} 最多识别 Java {major} 字节码":
    "ASM {version} recognizes bytecode up to Java {major}",
  "已应用 {count} 个模组的 Java 版本约束":
    "Applied Java constraints from {count} mods",
  "{count} 项模组兼容信息不完整，暂不自动跨 Java 主版本":
    "{count} mod compatibility entries are incomplete; keeping the declared Java major",
  "模组 Java 要求与已知兼容范围冲突：{path}":
    "Mod Java requirements conflict with the known compatible range: {path}",
  "没有同时满足游戏、加载器和模组约束的 Java：{reason}":
    "No Java version satisfies the game, loader and mod constraints: {reason}",
  "未找到兼容的 {runtime}，请指定 Java 路径或允许自动下载。":
    "No compatible {runtime} found. Select a Java executable or allow automatic downloads.",
  "无法检查模组的 Java 兼容性": "Could not check mod Java compatibility",
  "解析 Fabric 版本": "Resolving Fabric version",
  "Fabric 版本号无效": "Invalid Fabric version",
  "Fabric 配置缺少依赖": "Fabric profile is missing libraries",
  "Fabric 配置缺少基础版本": "Fabric profile is missing its base game version",
  "Fabric 配置与所选游戏或加载器版本不一致":
    "Fabric profile does not match the selected game or loader version",
  "同名版本已经存在且配置不同，请保留原版本或使用其他加载器版本":
    "A different profile already uses this name. Keep it or choose another loader version",
  "未在官方版本目录中找到此版本，请检查游戏版本号或关联已安装的加载器目录":
    "Version not found in the official catalogue. Check the game version or link an installed loader's directory",
  "缺少或无法读取版本配置：{0}": "Missing or unreadable version profile: {0}",
  "版本元数据与所选版本不一致：{0}":
    "Version profile does not match the selected version: {0}",
  "版本继承存在循环或层数过多：{0}":
    "Version inheritance is circular or too deep: {0}",
  "版本配置不完整，请检查基础游戏版本":
    "Incomplete version profile. Check the base game version",
  版本配置缺少启动参数: "Version profile is missing launch arguments",
  "启动所需文件缺失：{0}": "Required game file is missing: {0}",
  "整合包的游戏目录不存在：{0}":
    "The modpack's game directory does not exist: {0}",
  "无效 Maven 依赖：{0}": "Invalid Maven dependency: {0}",
  "Maven 校验文件过大": "Maven checksum file is too large",
  "Maven 校验值无效": "Invalid Maven checksum",
  "无法获取依赖校验值：{error}": "Could not fetch library checksum: {error}",
  旧版启动参数的引号未闭合: "Unclosed quote in legacy launch arguments",
  "暂无适合此系统的 Java，请指定本地 Java 路径":
    "No Java runtime is available for this system. Select a local Java executable",
  "此游戏的原生库需要 {0} Java，所选 Java 是 {1}。请切换 Java 或使用自动匹配。":
    "Game natives require {0} Java, but the selected Java is {1}. Select another Java or use automatic selection",
  游戏已停止: "Game stopped",
  "启动内存：{memory} MiB（自动）": "Game memory: {memory} MiB (Auto)",
  "启动内存：{memory} MiB（推荐配置）":
    "Game memory: {memory} MiB (Recommended)",
  "启动内存：{memory} MiB（最小配置）": "Game memory: {memory} MiB (Minimum)",
  "启动内存：{memory} MiB（自定义）": "Game memory: {memory} MiB (Custom)",
  "内存应为 512–65536 MiB": "Memory must be between 512 and 65536 MiB",
  "所选内存超过 32 位 Java 的可用范围，请选择 64 位 Java 或自动内存":
    "Selected memory exceeds the 32-bit Java range. Choose 64-bit Java or automatic memory",
  当前启动器版本无效: "Invalid launcher version",
  "暂无可识别的公开版本，请稍后重试":
    "No supported public release found. Try again later",
  发布地址无效: "Invalid release URL",
  无法连接更新服务器: "Could not connect to the update server",
  "当前运行方式不支持自动安装，请安装发布版或使用系统软件包管理器":
    "Install a release build or update through your package manager",
  启动器更新正在进行中: "A launcher update is already in progress",
  "更新版本已变化，请重新检查更新":
    "The available version changed; check for updates again",
  该版本未提供应用内更新包:
    "This release does not include an in-app update package",
  更新代理配置无效: "Invalid update proxy configuration",
  更新清单与发布版本不一致:
    "The update manifest does not match the selected release",
  "更新包下载或签名验证失败，请重试":
    "Update download or signature verification failed; please retry",
  请先下载启动器更新: "Download the launcher update first",
  "更新安装失败，请检查安装目录权限后重试":
    "Installation failed; check installation folder permissions and retry",
  "更新安装失败，请重新下载更新":
    "Installation failed; download the update again",
  "正在安装启动器更新，请稍候": "Installing the launcher update; please wait",
  "请先退出正在运行的游戏，再安装启动器更新":
    "Close running games before installing the launcher update",
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
  "使用本地 Java：{version} · {path}": "Using local Java: {version} · {path}",
  "需要 {requirement}；未检测到可用的本地 Java":
    "Requires {requirement}; no working local Java was detected",
  "需要 {requirement}；本地 Java 均不匹配：{available}":
    "Requires {requirement}; local Java installations do not match: {available}",
  "正在安装 {runtime}": "Installing {runtime}",
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
  "微软登录应用 ID 配置无效，请联系启动器维护者。":
    "The Microsoft sign-in app ID is invalid. Please contact the launcher maintainer.",
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
  此账户没有自定义皮肤: "This account has no custom skin",
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
  "并发应为 1–256，重试次数应为 0–5":
    "Parallel downloads must be 1–256; retry attempts must be 0–5",
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
  "游戏目录不能与启动器数据目录重叠，请选择其他位置":
    "The game folder must not overlap the launcher data folder. Choose another location",
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
  准备登录会话: "Preparing sign-in session",
  "Minecraft 返回的令牌有效期无效":
    "Minecraft returned an invalid token lifetime",
  系统钥匙串返回了无效凭据引用:
    "The system keychain returned an invalid credential reference",
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
