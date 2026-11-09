# 猫咪盒子启动器 · NekoCrate Launcher

基于 Rust、Tauri 2 和 Vue 3 的 Minecraft Java 版启动器，支持多游戏目录、加载器与整合包安装、资源管理和可自定义的桌面布局。

[下载官网](https://bzy.moe/NekoCrate-Launcher/) · [功能与设计](https://bzy.moe/NekoCrate-Launcher/features/) · [使用文档](https://bzy.moe/NekoCrate-Launcher/docs/) · [反馈问题](https://github.com/NekoOS-Group/NekoCrate-Launcher/issues) · [发布说明](docs/releases)

项目仍在开发中，当前开发分支为 `neko_dev`。桌面应用与 CLI 共用 Rust 核心、数据格式和资源索引。

## 下载与运行

从 [GitHub Releases](https://github.com/NekoOS-Group/NekoCrate-Launcher/releases) 选择对应平台的文件：

| 平台 | 安装版 | 便携运行 |
| --- | --- | --- |
| macOS Apple Silicon | `.dmg` | 解压 `.app.tar.gz`，双击 `.app` |
| Windows x64 | `-setup.exe` | 直接运行 `-win-amd64.exe` |
| Linux x64 | `.deb` | 为 `.elf` 添加执行权限后运行 |

安装版与便携版共用同一套程序，通过运行模式配置选择数据位置：

- **安装版**使用系统应用数据目录，游戏目录可以单独关联或创建。
- **便携版**使用程序旁的 `.minecraft` 和 `.nekocrate`，分别保存游戏与启动器数据；首次运行会创建并关联 `.minecraft`。macOS 的这两个目录位于 `.app` 旁。只移动程序会改用新位置的数据，需要一起迁移游戏和设置时，请关闭启动器后移动整个文件夹。
- 账户凭据保存在系统凭据库中，换电脑后需要重新登录。

Windows 需要 WebView2；Linux ELF 需要系统提供 GTK、WebKitGTK 和 D-Bus 运行库。macOS 发布包目前未做 Apple 公证，Windows 发布包尚无发布者代码签名。

macOS `.app`、Windows 安装版与便携版、Linux ELF 支持应用内下载更新、验签、安装替换和重启。Linux DEB 使用手动安装更新；当前发布文件说明见[构建基础设施](docs/build-infrastructure.md#分发格式)。

## 功能与设计

### 功能列表

- **账户与皮肤**

  - 通过浏览器登录微软账户，完成 Xbox / Minecraft 身份验证、游戏权益检查和令牌刷新；支持保存多个账户并切换当前启动身份。
  - 支持游客身份和游客名称设置。刷新令牌保存在 macOS Keychain、Windows Credential Manager 或 Linux Secret Service，认证流程见[账户与认证](docs/authentication.md)。
  - 首页展示可旋转的 3D 角色模型和账户头像；皮肤库支持预览、导入和管理皮肤，为游客选择外观，并管理微软账户皮肤。

- **游戏安装与版本管理**

  - 浏览并安装 Minecraft 原版、Fabric、Forge 和 NeoForge，识别已有游戏版本、版本继承关系及对应的启动参数。Forge 安装支持包含 `install_profile.json` 的安装器；早于 1.6 的传统分发包需另行准备。
  - 一个实例对应一个 `.minecraft` 游戏目录，同一目录可以添加多个版本。支持切换启动版本、设置显示名称、调整排列顺序和管理版本隔离；显示名称与磁盘目录名称独立。
  - 添加版本时可预选 Mods、资源包、光影和存档，按游戏版本与加载器匹配资源，并将资源准备纳入安装流程。
  - 支持检查和修复游戏文件，重新获取缺失或损坏的客户端、依赖库及资源；启动前准备匹配的 Java 和原生库。

- **整合包与资源浏览**

  - 浏览 Modrinth 和 CurseForge 的项目介绍、资源版本与依赖信息，按游戏版本、加载器和资源类型筛选下载内容。
  - 支持在线安装整合包、导入本地整合包，按清单下载依赖并应用覆盖文件。官方构建内置 CurseForge 应用凭据；受作者分发限制的文件需按任务提示手动补齐。
  - 管理 Mods、资源包、光影、数据包和存档；支持从文件管理器拖入资源，将数据包放入指定存档，以及在版本之间拖拽复制 Mods、资源包和光影。

- **Mod 配置与全局资源库**

  - 查看 Mod 元数据、依赖和关联版本，切换启用状态；编辑配置时提供 JSON / TOML / CFG 辅助、保存校验与备份。
  - 全局资源库汇总已关联目录中的资源，支持搜索、分类浏览、收藏本地文件，以及从 Modrinth / CurseForge 下载指定资源版本供后续使用。
  - 资源详情可跳转到对应游戏版本。缓存页展示 Java、皮肤、资源与未完成下载的占用，并提供未完成下载清理入口。

- **目录导入、复制与导出**

  - 自动发现本机游戏目录，支持关联现有 `.minecraft`、指定位置新建目录，以及拖入文件夹扫描游戏；多个扫描结果可预览后选择。
  - 支持跨游戏目录复制版本和资源，导出实例或版本 ZIP。实例导出可携带对应平台的便携启动器，方便整理和分享游戏环境。
  - 支持为实例配置独立的 Java、内存和游戏设置。已有整合包沿用原工作目录，新建加载器版本使用独立目录维护 Mods、配置及存档。
  - 移除关联目录时解除关联并保留原文件；移除托管实例或单个版本时使用系统废纸篓／回收站，操作前检查版本依赖。

- **Java、内存与下载任务**

  - 自动发现系统安装、实例指定和手动登记的 Java，展示版本、发行方、架构及路径；根据游戏要求复用兼容运行时或从 Adoptium / GitHub 下载 Java。Apple Silicon 上缺少 ARM 原生库的游戏版本可能需要 Intel Java 和系统的 Intel 程序运行支持。
  - 提供自动、推荐、最小和自定义内存分配方式，允许按实例调整启动配置。
  - 下载支持 1–256 并发、自适应调度、暂停续传、取消和单文件重试；通过完整性校验和本地资源复用减少重复下载，恢复任务时显示已完成的工作量。
  - 游戏文件与 Mod 资源可分别配置下载来源；网络设置支持代理，下载和安装任务显示当前步骤、文件进度与错误信息。

- **日志与故障诊断**

  - 查看游戏实时输出，按日志内容搜索与筛选；敏感信息在展示和记录前脱敏。可选择启动时显示日志或最小化启动器。
  - 游戏异常退出时展示相关日志，识别常见 Java 版本、内存不足、Mod 依赖和配置错误，提供对应的排查信息。
  - 启动器记录下载、更新、启动及认证操作的失败信息，包含时间、版本、平台和错误链，便于反馈问题。

- **界面布局与首页小组件**

  - 支持简体中文、English 和跟随系统，可将界面语言同步到游戏；提供主题背景和可调整的首页布局。
  - 首页可配置一言、Trends、服务器列表、最近存档、待办便签和像素时钟。服务器组件展示在线人数与延迟，服务器和存档入口可选择关联的游戏版本启动。
  - 资源管理、版本管理、日志和下载任务以工具窗呈现，支持移动、缩放、边缘吸附和最大化；多个工具窗可同时打开，位置与尺寸会保存。
  - 实例、版本和资源支持拖拽操作；界面提供键盘导航，并响应系统的减少动态效果设置。

### UI 设计语言与功能拆分

**首页围绕“选择身份、选择游戏、开始游玩”组织。** 账户、角色模型、实例选择和启动按钮构成主要操作区；服务器、最近存档、待办和订阅动态作为可选小组件加入。布局编辑支持调整位置、方向和尺寸，实例栏提供横排、竖排及滚轮选择器，让常用入口适应个人习惯和窗口空间。

**视觉层次由透明表面、主题色和内容本身共同建立。** 动态几何背景承载主题氛围，角色模型与像素元素呼应 Minecraft；工具窗、菜单和设置气泡通过透明底色、文字明暗与悬停反馈区分层级。列表切换、窗口展开和拖放到达使用连贯的过渡，帮助用户追踪操作前后的对象位置。界面响应系统的减少动态效果设置。

**功能按操作对象和作用范围拆分。** 实例代表游戏目录，版本管理负责其中的启动版本、隔离设置与游戏内容；全局资源管理集中处理跨实例复用的 Java、皮肤和资源收藏；账户管理负责启动身份；全局设置负责语言、网络和启动器偏好。资源详情保留关联版本入口，便于从全局浏览返回具体游戏环境。

**长时间工作放进工具窗，短操作靠近对象呈现。** 资源管理、版本管理、下载任务和日志支持同时打开，工具窗可以移动、缩放、吸附和最大化，方便一边整理资源、一边查看目标版本或任务进度。账户选择、实例设置和全局设置使用紧凑的菜单或气泡；需要确认目标与影响范围的操作提供预览或确认步骤。

#### dev0.9：“拖动万物”

“拖动万物”是 dev0.9 系列贯穿目录、版本、资源和布局的交互设计：把对象拖到目标位置，由目标表达要执行的操作。文件管理器、首页实例栏、版本列表、资源窗口和导出区域之间因此形成连续的操作路径。

| 拖动对象 | 目标位置与操作 |
| --- | --- |
| 游戏目录、实例 ZIP | 拖入窗口后识别游戏内容，预览并选择关联或导入位置 |
| `versions` 或单个版本目录 | 拖入窗口识别所属游戏目录；拖到指定实例时复制到目标目录 |
| Mods、资源包、光影、存档 | 拖到版本或对应资源区域安装；已有资源可拖到其他版本复制，也可加入新版本的资源预选 |
| 数据包 | 拖到指定存档，安装到该存档的数据包目录 |
| Modrinth / CurseForge 项目链接 | 拖入后打开项目详情，选择兼容的资源版本 |
| 配置文件 | 拖入配置编辑器，比较内容后保存 |
| 实例、版本与资源 | 拖出到文件管理器导出；实例和版本可打包为 ZIP，实例包可附带便携启动器 |
| 版本与可删除资源 | 拖到右上角回收区，按确认流程移入系统废纸篓／回收站 |
| 实例条目、版本条目与首页组件 | 拖动排序、调整布局；工具窗拖到边缘可吸附排列 |

拖动开始后，提示随悬停目标更新；折叠的实例区域和版本列表会按需展开，目标区域反馈可执行的操作。资源支持多选和批量拖放，复制保留源文件及 Mod 的启停状态。文件类型通过实际内容识别，提交前检查目标版本、同名冲突和必要依赖；涉及多个游戏目录或数据包存档时，先明确目标再执行。

拖放与添加、导入、复制、导出等按钮入口共用同一套操作流程，用户可以在鼠标拖动、菜单选择和键盘操作之间切换。设计目标是让“拿起什么、放到哪里、会发生什么”在操作过程中清晰可见。具体功能记录见 [dev0.9 系列更新](docs/releases/dev0.9.9.md)。

## 开发

下载官网与使用文档的前端位于 [`site/`](site/README.md)，独立构建并部署到 GitHub Pages。运行 `npm --prefix site ci`、`npm --prefix site run dev` 可本地预览。

需要 Rust 1.93 或更新的 stable、Node.js 22，以及 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。macOS 本地签名还需要 Python 3。

以下命令均在仓库根目录执行。

### 启动桌面界面

```sh
git switch neko_dev
npm --prefix app ci
cargo ui
```

`cargo ui` 会启动 Vite 和 Tauri 窗口，监听代码修改；Ctrl+C 结束开发进程，`cargo ui --help` 查看参数。开发模式默认使用系统应用数据目录，也可单独指定测试目录：

```sh
NEKOCRATE_DATA_DIR="$PWD/.runtime/dev" cargo ui
```

macOS 下 `cargo ui` 使用固定的本地自签身份 `NekoCrate Local Development`。首次设置可能出现系统确认，Rust 重新编译后仍可能要求钥匙串授权；直接运行 `tauri dev` 会绕过本地签名步骤。详见[签名与密钥管理](docs/build-infrastructure.md#签名与密钥管理)。

仅预览界面可执行 `npm --prefix app run dev`。账户、安装和游戏启动等操作需要桌面运行时。

### 构建桌面应用

按当前平台选择一条命令：

```sh
# macOS：默认生成 .app
npm --prefix app run tauri build

# Windows：NSIS 安装包
npm --prefix app run tauri build -- --bundles nsis

# Linux：DEB 安装包
npm --prefix app run tauri build -- --bundles deb
```

打包产物位于 `target/release/bundle/`。只构建可执行文件时使用 `--no-bundle`。macOS 本机打包可复用开发自签证书：

```sh
python3 scripts/macos_sign.py ensure
npm --prefix app run tauri build -- --config src-tauri/tauri.local.conf.json
```

### CLI

```sh
cargo run -p nekocrate-cli -- --help
cargo run -p nekocrate-cli -- --data-dir .runtime/cli versions
cargo run -p nekocrate-cli -- --data-dir .runtime/cli create "我的世界" --version 1.21.1

# 替换为创建结果中的实例 ID
instance_id="实例 ID"
cargo run -p nekocrate-cli -- --data-dir .runtime/cli install "$instance_id"
cargo run -p nekocrate-cli -- --data-dir .runtime/cli login
cargo run -p nekocrate-cli -- --data-dir .runtime/cli launch "$instance_id"
```

还支持 `add-version`、`select-version`、`repair`、`import`、`java` 和账户管理等命令，具体参数见 `--help`。Ctrl+C 可取消下载／登录，或停止当前 CLI 启动的游戏。同一数据目录不允许同时打开多个启动器进程。

### 检查

```sh
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix app test
npm --prefix app run build
```

真实网络安装与修复检查：

```sh
python3 scripts/smoke_install.py --version 1.21.1
```

该脚本使用独立测试目录，下载游戏资源和 Java，再验证文件损坏后的修复；下载量约数百 MB 至数 GB，不读取用户账户或启动游戏。

### CI 与发布

CI 集中执行前端与 Rust 检查，Windows / macOS 补充平台差异测试。发布工作流固定标签提交，复用已检查的前端产物和跨版本 Rust 缓存，三个平台构建成功且完整 CI 通过后发布。

发布说明保存在 [`docs/releases/`](docs/releases)。缓存清理、Linux 构建镜像、发布附件与签名配置见[构建基础设施](docs/build-infrastructure.md)。

## 仓库结构

```text
crates/nekocrate-core/   账户、下载、Java、元数据、实例、启动与日志
crates/nekocrate-cli/    命令行入口
app/src/                Vue / TypeScript 界面
app/src-tauri/          Tauri IPC 与桌面打包
site/                   下载官网、功能介绍与使用文档
xtask/                  cargo ui 开发入口
docs/                   架构、认证、构建与发布说明
scripts/                构建、签名与验证工具
```

## 许可证

Copyright © NekoOS Group。启动器代码采用 [GNU GPL v3.0](LICENSE)（`GPL-3.0-only`）。第三方依赖和 Minecraft 原版素材保留各自的许可与权利声明；方块素材来源见 [provenance.json](app/src/assets/blocks/provenance.json)。
