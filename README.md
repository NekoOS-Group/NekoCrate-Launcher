# 猫咪盒子启动器 · NekoCrate Launcher

一个 Rust 核心 + Tauri 2 / Vue 3 桌面界面的 Minecraft Java 版启动器原型。

当前开发分支：`neko_dev`。应用与 CLI 使用同一个核心、数据格式和下载缓存。

上游仓库：[NekoOS-Group/NekoCrate-Launcher](https://github.com/NekoOS-Group/NekoCrate-Launcher)。正式中文名为「猫咪盒子启动器」，英文名为 NekoCrate Launcher。

## 当前功能

| 模块 | 已实现 |
| --- | --- |
| 账户 | Microsoft 设备验证码授权 → Xbox Live → XSTS → Minecraft；权益与档案检查；刷新令牌；多账户切换/移除；未登录时以 Player 本地身份启动 |
| 凭据 | 刷新令牌存入 macOS Keychain / Windows Credential Manager / Linux Secret Service；数据库与前端不接收令牌 |
| 游戏 | 官方版本目录、版本元数据、客户端、库、资源索引、资源对象、原生库、日志配置；按规则构建 JVM 与游戏参数 |
| Java | 检测 JAVA_HOME、PATH、托管运行时；按游戏要求选择 Java；通过 Adoptium 自动下载并验证 SHA-256；实例可指定路径 |
| 下载 | 1–32 并发与自适应、暂停/继续、重试、取消、HTTP Range 续传、SHA-1/SHA-256 和长度校验、共享内容缓存；游戏源自动测速与回退、官方/BMCLAPI 手动选择；独立 Mod 源设置与系统/手动代理 |
| 修复 | 校验游戏文件；损坏文件优先从已校验缓存恢复，再重新下载；安装中断后不标记为可启动 |
| 实例 | 每个实例对应一个 `.minecraft`；已有目录直接关联原路径并在其中读写资源和依赖；同一实例添加、切换多个版本，共用存档/配置；Java/内存设置 |
| 进程 | 直接启动 Java，不经过 shell；工作目录隔离；实时 stdout/stderr、脱敏日志、停止游戏、退出状态 |
| 界面 | 简体中文 / English / 跟随系统；可同步 Minecraft 语言；实例拖拽排序；官方发布更新检查 |

## 支持边界

- 原型支持官方原版 Java Edition 1.13 及更新版本的元数据协议。Apple Silicon 使用带原生 ARM 库的 1.19+。并未逐个版本验证。
- Forge、Fabric、NeoForge、Quilt、外置认证、Bedrock 尚未实现。迁入的模组会保留，但原版不会加载这些模组。
- 下载恢复支持 HTTP Range 字节级续传；未变化的完整文件复用持久化校验记录，主动修复仍完整校验。服务器不支持续传时安全地重新下载对应文件。
- 关联目录移除后仍保留原处，只取消关联；启动器内新建的托管实例移除后移至 `trash/`，不永久删除。
- 自动 Java 下载依赖 Adoptium/GitHub 网络。无法访问时，可在实例设置中指定对应主版本的本地 Java。
- macOS 是本地验证平台；Windows/Linux 配置了 CI 构建检查，未经本机实测。

## 开发运行

需要 Rust 1.93 或更新的 stable、Node.js 22，以及 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
git switch neko_dev
npm --prefix app ci # 首次运行时安装前端依赖
cargo ui
```

在仓库根目录执行 `cargo ui`，会一并启动 Vite 前端开发服务和 Tauri 桌面窗口，并监听代码修改。等价于在 `app/` 下运行 `npm run tauri dev`；按 Ctrl+C 结束开发进程。`cargo ui --help` 查看可传入的 Tauri 开发参数。

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

开发版本发布由 `.github/workflows/release.yml` 管理：推送 `dev*` 标签后，先构建一次共享前端，再在 macOS ARM64、Windows AMD64、Ubuntu 22.04 AMD64 原生 runner 上分别编译并打包。源码测试、格式检查与 Clippy 由普通 CI 执行，发布不重复运行。三个平台全部成功后校验来源提交、架构与 SHA-256，上传 DMG / app ZIP、NSIS EXE、DEB / AppImage，再公开 GitHub 预发布版。构建使用 Rust 1.93.1、Node.js 22 和锁定依赖，`docs/releases/<标签>.md` 保存发布说明。`dev0.1` 对应应用版本 `0.1.0-dev.1`；`dev0.2` 对应 `0.2.0-dev.1`；`dev0.3` 对应 `0.3.0-dev.1`。

macOS 开发包采用 [Tauri ad-hoc 签名](https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing)，未做 Apple 公证；Windows 开发包没有发布者代码签名。Linux 使用英文包名满足包管理器命名要求，桌面入口及窗口仍显示「猫咪盒子启动器」。

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
