# 猫咪盒子启动器 · NekoCrate Launcher

一个 Rust 核心 + Tauri 2 / Vue 3 桌面界面的 Minecraft Java 版启动器原型。

当前开发分支：`neko_dev`。应用与 CLI 使用同一个核心、数据格式和资源索引。

上游仓库：[NekoOS-Group/NekoCrate-Launcher](https://github.com/NekoOS-Group/NekoCrate-Launcher)。正式中文名为「猫咪盒子启动器」，英文名为 NekoCrate Launcher。

## 当前功能

| 模块 | 已实现 |
| --- | --- |
| 账户 | Microsoft 设备验证码授权 → Xbox Live → XSTS → Minecraft；权益与档案检查；刷新令牌；多账户切换/移除；游客身份与名称设置；登录账户修改 Java 版名称与皮肤 |
| 凭据 | 刷新令牌存入 macOS Keychain / Windows Credential Manager / Linux Secret Service；数据库与前端不接收令牌 |
| 游戏 | 原版、Fabric、Forge、NeoForge 安装；版本目录与推荐版本选择；已有加载器版本启动；版本继承、现代/旧式启动参数、依赖与原生库解析 |
| Java | 检测本机及托管运行时；综合已知加载器范围、ASM 与模组声明选择兼容 Java，优先复用本地版本；信息不足时保守匹配；现代加载器自动使用完整 JDK；下载 SHA-256 校验 |
| 内存 | 自动（默认）、推荐、最小、自定义；按游戏版本、Mod 规模及支持的整合包声明估算；启动前重新读取可用内存 |
| 下载 | 1–256 并发与自适应、暂停/继续、重试、取消、HTTP Range 续传、SHA-1/SHA-256 和长度校验、本地资源索引复用；区分传输／复用／校验／安装状态，恢复完成量与单文件重试；游戏源自动测速与回退、官方/BMCLAPI 手动选择；独立 Mod 源设置与系统/手动代理 |
| 资源 | 全局 Java、游戏资源、皮肤与缓存管理；Mods / 资源包 / 光影可拖入其他版本；内置 Steve / Cirno 皮肤；安装成功后自动清理下载暂存 |
| 诊断 | 意外退出自动展示日志和本次崩溃依据，识别常见内存、Java、依赖、配置、Mixin 与图形问题 |
| 修复 | 校验游戏文件；损坏文件优先从已校验缓存恢复，再重新下载；安装中断后不标记为可启动 |
| 实例 | 每个实例对应一个 `.minecraft`；已有目录直接关联原路径并在其中读写资源和依赖；同一实例添加、切换、改名与移除版本；查看 Mods、数据包、资源包和存档；Mod 启用／禁用、关联配置编辑与备份，JSON/TOML 高亮、格式化与语法提示，CFG 高亮、Forge 分组缩进与结构提示；Java/内存设置 |
| 进程 | 直接启动 Java，不经过 shell；工作目录隔离；实时 stdout/stderr、脱敏日志；异常退出自动打开日志，正常退出与主动停止不弹出 |
| 界面 | 简体中文 / English / 跟随系统；可同步 Minecraft 语言；实例与启动版本拖拽排序、拖到右上角移除；应用内下载更新、验签、安装与重启 |

## 支持边界

- 普通游戏下载页可选择原版、Fabric、Forge、NeoForge；加载器版本目录缓存一小时，联网失败时复用已有目录。支持导入 Modrinth / CurseForge 整合包；不自动添加 Fabric API 或未声明的其他模组。CurseForge 使用官方构建内置的应用凭据，无需用户填写密钥；作者限制第三方分发的文件仍需手动补齐。
- 新建加载器版本使用 `versions/<版本>` 隔离 Mods、配置和存档，安装记录保存在其 `.nekocrate/loader.json`；暂停时终止正在运行的 Java 安装步骤，继续时重跑该步骤，已校验完成的步骤保留。支持修复本启动器安装的加载器；不原地升级或替换已有整合包加载器，不覆盖同名的外部版本。
- Forge 安装支持含 `install_profile.json` 的新旧安装器格式，早于 1.6 的传统分发包不在选择列表中；没有逐个历史构建做实机验证。
- 已有关联目录会在启动器启动时重新识别版本。支持 `inheritsFrom` 和 HMCL 合并配置；有隔离目录的整合包保留 `versions/<子实例>` 中的模组、配置和存档，原版及已有普通版本继续沿用原工作目录。
- 支持旧式 Forge 启动参数；Apple Silicon 上缺少 ARM 原生库的旧版本匹配 Intel Java，需要系统可运行 Intel 程序。没有逐个游戏、加载器及整合包版本做实机验证。
- 外置认证、Bedrock、Quilt 安装尚未实现。
- 下载恢复支持 HTTP Range 字节级续传；未变化的完整文件复用持久化校验记录，主动修复仍完整校验。服务器不支持续传时安全地重新下载对应文件。
- 关联目录移除后仍保留原处，只取消关联；启动器内新建的托管实例移除后进入系统废纸篓／回收站，可通过系统恢复或清空。
- 添加游戏目录时自动发现官方启动器目录及当前可执行文件旁的 `.minecraft`，也可选择已有目录或在数据目录／外部位置新建。官方目录默认命名为「官方启动器」。
- 管理版本时，改名只修改显示名称，保留加载器 ID 与磁盘目录；移除版本会检查其他版本的依赖，并将该版本目录移入系统废纸篓／回收站，共享文件保留。四类内容按版本实际工作目录读取，数据包同时列出游戏目录与存档内的内容。
- 将文件夹拖入桌面窗口，会在后台查找它及下层隐藏的 `.minecraft`。单个新目录直接关联，多个目录先预览勾选；默认名称取游戏目录的上层文件夹名。同目录不重复添加，游戏文件保持原位。扫描可取消，关联提示显示名称与路径并在 2 秒后消失。
- 自动 Java 下载依赖 Adoptium/GitHub 网络。无法访问时，可在实例设置中指定对应主版本的本地 Java。
- macOS 是本地验证平台；Windows/Linux 配置了 CI 构建检查，未经本机实测。

## 开发运行

需要 Rust 1.93 或更新的 stable、Node.js 22，以及 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。macOS 的本地签名脚本还需要 Python 3。

```sh
git switch neko_dev
npm --prefix app ci # 首次运行时安装前端依赖
cargo ui
```

在仓库根目录执行 `cargo ui`，会一并启动 Vite 前端开发服务和 Tauri 桌面窗口，并监听代码修改；按 Ctrl+C 结束开发进程。`cargo ui --help` 查看可传入的 Tauri 开发参数。

macOS 下 `cargo ui` 会在首次运行时创建固定的本地自签身份 `NekoCrate Local Development`，之后在每次编译完成、启动程序之前签名，包括 Rust 热重载。私钥不可导出地保存在当前用户钥匙串；首次设置可能出现系统确认，旧的临时签名迁移后可能需要重新授权一次凭据访问。证书缺失时会报错，不自动更换身份。直接执行 `npm run tauri dev` 会绕过此签名步骤。详见[签名与密钥管理](docs/build-infrastructure.md#签名与密钥管理)。

可用 `NEKOCRATE_DATA_DIR` 指定测试数据目录。默认使用操作系统应用数据目录。

```sh
# 在仓库根目录执行；开发数据不会进入 Git
NEKOCRATE_DATA_DIR="$PWD/.runtime/dev" cargo ui
```

构建桌面应用：

```sh
cd app
npm run tauri build
# macOS: ../target/release/bundle/macos/猫咪盒子启动器.app
```

项目默认 bundle 类型是 macOS app。其他平台可通过 `--bundles nsis` / `--bundles deb` 选择平台包，或先执行 `npm run tauri build -- --no-bundle` 验证可执行文件。

本机打包也可复用上述自签证书（在仓库根目录执行）：

```sh
python3 scripts/macos_sign.py ensure
npm --prefix app run tauri build -- --config src-tauri/tauri.local.conf.json
```

普通 CI 在 Linux 上集中执行一次前端测试与构建、Rust 完整测试、格式检查和 Clippy；Windows/macOS 仅补充文件系统差异测试，不再各自重复整套检查或编译桌面测试程序。通过检查的前端产物保留 14 天。

Linux CI 和发布构建使用固定 digest 的 Ubuntu 22.04 预装依赖镜像。镜像升级前在全新的 runner 上比较三次 apt 安装与三次冷拉取；构建缓存自动保留各平台最新代次，定期清理旧标签范围与重复代次。规则、实测数据、手动操作和发布文件说明见 [构建基础设施](docs/build-infrastructure.md)。

开发版本发布由 `.github/workflows/release.yml` 管理：先将提交推送到 `neko_dev` / `main`，再推送 `dev*` 标签；标签任务将构建调度到默认分支，复用跨版本的 Rust 发布缓存（手动发布也选择默认分支），实际源码仍固定为标签提交；发布提前复用相同提交已检查的前端产物，并与剩余 CI 检查并行编译；公开发布仍须等待完整 CI 成功，不再安装前端依赖、重跑检查或重建前端。三个原生 runner 仅安装锁定版本的 Tauri CLI，在编译前检查前端入口与资源，编译后执行实际二进制的 `--verify-frontend`，验证内嵌资源及 IPC 异步对象大小，不打开窗口或用户数据；分别编译并打包 macOS ARM64、Windows AMD64、Ubuntu 22.04 AMD64。全部成功后校验来源提交、架构与 SHA-256，上传 DMG、NSIS EXE、DEB / AppImage，再公开 GitHub 预发布版。若 CI 失败或产物已过期，需要先重跑对应提交的 CI，再重跑发布任务。构建使用 Rust 1.93.1、Node.js 22 和锁定依赖，`docs/releases/<标签>.md` 保存发布说明。`dev0.1` 对应应用版本 `0.1.0-dev.1`；`dev0.2` 对应 `0.2.0-dev.1`；`dev0.3` 对应 `0.3.0-dev.1`；`dev0.3.1` 对应 `0.3.1-dev.1`；`dev0.3.2` 对应 `0.3.2-dev.1`；`dev0.4` 对应 `0.4.0-dev.1`；`dev0.5` 对应 `0.5.0-dev.1`；`dev0.6` 对应 `0.6.0-dev.1`；`dev0.7` 对应 `0.7.0-dev.1`；`dev0.7.1` 对应 `0.7.1-dev.1`；`dev0.8` 对应 `0.8.0-dev.1`；`dev0.9` 对应 `0.9.0-dev.1`；`dev0.9.1` 对应 `0.9.1-dev.1`。

应用内更新使用 Tauri updater：macOS 安装后的 `.app`、Windows NSIS 安装版和 Linux AppImage 支持下载、签名验证、安装替换及重启；开发模式、未安装的 macOS 二进制和 Linux DEB 保留手动安装入口。下载可取消，游戏或下载任务运行时保留准备好的更新，任务结束后点击「安装并重启」。已发布的 dev0.4 及以前版本仍需手动安装一次带更新器的新版本。

发布 workflow 需要仓库 Actions Secret `TAURI_SIGNING_PRIVATE_KEY`，加密私钥另设 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。公钥固定在 `tauri.conf.json`；私钥保存在仓库外并备份，不能提交。发布额外提供 macOS `.app.tar.gz`、`latest.json` 和 `BUILDINFO.json`。构建时生成的签名内嵌到 `latest.json`，不再单独上传 `.sig`；`BUILDINFO.json` 的 `files` 统一记录各安装包和更新清单的大小与 SHA-256，不再提供 `SHA256SUMS` 或 `.app.zip`。更新清单绑定当前发布标签、版本及平台。客户端同时验证 HTTPS、更新签名及签名中的版本；该签名独立于 Apple / Windows 代码签名。

GitHub Release 的 macOS 开发包目前仍采用 [Tauri ad-hoc 签名](https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing)，未做 Apple 公证；本机自签私钥不会上传到 CI。Windows 开发包没有发布者代码签名。Linux 使用英文包名满足包管理器命名要求，桌面入口及窗口仍显示「猫咪盒子启动器」。

浏览器仅用于界面预览：在仓库根目录执行 `npm --prefix app run dev`。安装、账户和启动等操作需要桌面运行时，浏览器不会模拟成功结果。

## CLI

```sh
cargo run -p nekocrate-cli -- --help
cargo run -p nekocrate-cli -- --data-dir .runtime/cli versions
cargo run -p nekocrate-cli -- --data-dir .runtime/cli create "我的世界" --version 1.21.1
# 将创建结果中的 id 用于后续命令；同一目录可添加更多版本
cargo run -p nekocrate-cli -- --data-dir .runtime/cli add-version <instance-id> 1.20.1
cargo run -p nekocrate-cli -- --data-dir .runtime/cli select-version <instance-id> 1.21.1
cargo run -p nekocrate-cli -- --data-dir .runtime/cli install <instance-id>
cargo run -p nekocrate-cli -- --data-dir .runtime/cli configure --client-id <your-client-id>
cargo run -p nekocrate-cli -- --data-dir .runtime/cli login
cargo run -p nekocrate-cli -- --data-dir .runtime/cli launch <instance-id>
```

其他命令：`status`、`repair`、`clone`、`delete`、`import`、`java`、`refresh`、`select-account`、`remove-account`。Ctrl+C 取消 CLI 下载/登录或停止当前 CLI 启动的游戏。同一数据目录不允许同时打开多个启动器进程。

## 检查

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd app && npm test && npm run build
```

真实网络安装检查（下载游戏资源和 Java，约数百 MB 至数 GB）：

```sh
python3 scripts/smoke_install.py --version 1.21.1
```

该脚本创建独立测试目录，安装官方版本，主动损坏一个测试资源文件，再验证修复。不会读取用户账户或启动付费游戏。

## 仓库结构

```text
crates/nekocrate-core/   账户、下载、Java、元数据、实例、启动与日志
crates/nekocrate-cli/    可脚本化的命令行入口
app/src/                Vue / TypeScript 桌面界面
app/src-tauri/          Tauri IPC 与桌面打包
docs/                   认证配置、架构与验证记录
scripts/                可复现的真实网络检查
```

参见 [架构说明](docs/architecture.md)。

## 许可证

Copyright © NekoOS Group。启动器代码采用 [GNU GPL v3.0](LICENSE)（`GPL-3.0-only`）。第三方依赖和 Minecraft 原版素材保留各自的许可与权利声明；原版方块素材来源见 [provenance.json](app/src/assets/blocks/provenance.json)。
