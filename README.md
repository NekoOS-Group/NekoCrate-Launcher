# NekoCrate Launcher · 猫匣启动器

一个 Rust 核心 + Tauri 2 / Vue 3 桌面界面的 Minecraft Java 版启动器原型。

当前开发分支：`neko_dev`。应用与 CLI 使用同一个核心、数据格式和下载缓存。

上游仓库：[NekoOS-Group/NekoCrate-Lancher](https://github.com/NekoOS-Group/NekoCrate-Lancher)。仓库 URL 沿用现有拼写，产品显示名统一为 NekoCrate Launcher。

## 当前功能

| 模块 | 已实现 |
| --- | --- |
| 账户 | Microsoft 设备验证码授权 → Xbox Live → XSTS → Minecraft；权益与档案检查；刷新令牌；多账户切换/移除；常见登录错误说明 |
| 凭据 | 刷新令牌存入 macOS Keychain / Windows Credential Manager / Linux Secret Service；数据库与前端不接收令牌 |
| 游戏 | 官方版本目录、版本元数据、客户端、库、资源索引、资源对象、原生库、日志配置；按规则构建 JVM 与游戏参数 |
| Java | 检测 JAVA_HOME、PATH、托管运行时；按游戏要求选择 Java；通过 Adoptium 自动下载并验证 SHA-256；实例可指定路径 |
| 下载 | 1–32 并发、重试、取消、SHA-1/SHA-256 和长度校验、共享内容缓存；官方/BMCLAPI 优先与失败回退 |
| 修复 | 校验游戏文件；损坏文件优先从已校验缓存恢复，再重新下载；安装中断后不标记为可启动 |
| 实例 | UUID 隔离目录、创建、复制、可恢复移除、每实例 Java/内存设置、已有目录复制迁移 |
| 进程 | 直接启动 Java，不经过 shell；工作目录隔离；实时 stdout/stderr、脱敏日志、停止游戏、退出状态 |

## 支持边界

- 原型支持官方原版 Java Edition 1.13 及更新版本的元数据协议。Apple Silicon 使用带原生 ARM 库的 1.19+。并未逐个版本验证。
- Forge、Fabric、NeoForge、Quilt、外置认证、离线账户、Bedrock 尚未实现。迁入的模组会保留，但原版不会加载这些模组。
- 下载恢复以完整文件为单位。已校验文件复用；未完成的单文件重新下载，不提供 HTTP Range 字节级续传。
- 实例移除会移动到 `trash/`，不会永久删除。暂未提供图形恢复入口，可以从其中复制游戏目录重新导入。
- 自动 Java 下载依赖 Adoptium/GitHub 网络。无法访问时，可在实例设置中指定对应主版本的本地 Java。
- macOS 是本地验证平台；Windows/Linux 配置了 CI 构建检查，未经本机实测。
- 本项目不包含游戏文件、微软 Client ID、用户凭据或其他启动器的应用密钥。

## 开发运行

需要 Rust 1.93 或更新的 stable、Node.js 22，以及 [Tauri 系统依赖](https://v2.tauri.app/start/prerequisites/)。

```sh
git switch neko_dev
cd app
npm ci
npm run tauri dev
```

可用 `NEKOCRATE_DATA_DIR` 指定测试数据目录。默认使用操作系统应用数据目录。

```sh
# 在仓库根目录执行；开发数据不会进入 Git
NEKOCRATE_DATA_DIR="$PWD/.runtime/dev" npm --prefix app run tauri dev
```

构建桌面应用：

```sh
cd app
npm run tauri build
# macOS: ../target/release/bundle/macos/NekoCrate Launcher.app
```

项目默认 bundle 类型是 macOS app。其他平台可通过 `--bundles nsis` / `--bundles deb` 选择平台包，或先执行 `npm run tauri build -- --no-bundle` 验证可执行文件。

浏览器仅用于界面预览：`npm run dev`。安装、账户和启动等操作需要桌面运行时，浏览器不会模拟成功结果。

## 首次使用

1. 在「设置」填写自己的 Microsoft Client ID，并保存。应用注册要求见 [登录配置说明](docs/authentication.md)。
2. 在「账户」选择「添加账户」，打开微软官方页面输入验证码，完成权益验证。
3. 在「我的猫匣」新建实例，输入官方版本号（可先在下载页同步版本目录）。
4. 点击「安装游戏」。缺少 Java 时会按设置自动下载。
5. 安装完成后点击「开始游戏」。右上角终端按钮查看日志。

安装与修复不需要登录；完整游戏启动必须通过微软账户及游戏权益验证。

## CLI

```sh
cargo run -p nekocrate-cli -- --help
cargo run -p nekocrate-cli -- --data-dir .runtime/cli versions
cargo run -p nekocrate-cli -- --data-dir .runtime/cli create "我的世界" --version 1.21.1
# 将创建结果中的 id 用于后续命令
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
cd app && npm run build
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

参见 [架构说明](docs/architecture.md) 与 [验证记录](docs/validation.md)。

Minecraft 是 Mojang/Microsoft 的商标，本项目与其没有隶属关系。
