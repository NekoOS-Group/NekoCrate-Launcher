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
| 下载 | 1–32 并发、重试、取消、SHA-1/SHA-256 和长度校验、共享内容缓存；官方/BMCLAPI 优先与失败回退 |
| 修复 | 校验游戏文件；损坏文件优先从已校验缓存恢复，再重新下载；安装中断后不标记为可启动 |
| 实例 | 每个实例对应一个 `.minecraft`；同一实例添加、切换多个版本，共用存档/配置；目录复制、可恢复移除、Java/内存设置 |
| 进程 | 直接启动 Java，不经过 shell；工作目录隔离；实时 stdout/stderr、脱敏日志、停止游戏、退出状态 |

## 支持边界

- 原型支持官方原版 Java Edition 1.13 及更新版本的元数据协议。Apple Silicon 使用带原生 ARM 库的 1.19+。并未逐个版本验证。
- Forge、Fabric、NeoForge、Quilt、外置认证、Bedrock 尚未实现。迁入的模组会保留，但原版不会加载这些模组。
- 下载恢复以完整文件为单位。已校验文件复用；未完成的单文件重新下载，不提供 HTTP Range 字节级续传。
- 实例移除会移动到 `trash/`，不会永久删除。暂未提供图形恢复入口，可以从其中复制游戏目录重新导入。
- 自动 Java 下载依赖 Adoptium/GitHub 网络。无法访问时，可在实例设置中指定对应主版本的本地 Java。
- macOS 是本地验证平台；Windows/Linux 配置了 CI 构建检查，未经本机实测。
- 本项目不包含游戏客户端、微软 Client ID、用户凭据或其他启动器的应用密钥；内置 Steve 预览纹理的来源和归属记录在素材目录。

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

开发版本发布由 `.github/workflows/release.yml` 管理：推送 `dev*` 标签后，在 macOS ARM64、Windows AMD64、Ubuntu 22.04 AMD64 原生 runner 上分别测试并构建。三个平台全部成功后校验来源提交、架构与 SHA-256，上传 DMG / app ZIP、NSIS EXE、DEB / AppImage，再公开 GitHub 预发布版。构建使用 Rust 1.93.1、Node.js 22 和锁定依赖，`docs/releases/<标签>.md` 保存发布说明。`dev0.1` 对应应用版本 `0.1.0-dev.1`。

macOS 开发包采用 [Tauri ad-hoc 签名](https://v2.tauri.app/distribute/sign/macos/#ad-hoc-signing)，未做 Apple 公证；Windows 开发包没有发布者代码签名。Linux 使用英文包名满足包管理器命名要求，桌面入口及窗口仍显示「猫咪盒子启动器」。

浏览器仅用于界面预览：在仓库根目录执行 `npm --prefix app run dev`。安装、账户和启动等操作需要桌面运行时，浏览器不会模拟成功结果。

## 首次使用

1. 登录为可选步骤，未登录可直接进行第 3 步，以 Player 本地身份游玩。需要登录时，点击左上角账户卡片，弹窗内的头像代表已添加账户，点击头像切换本次启动的账户，点击「+」添加账户。开发构建尚未配置登录应用时，会在此流程中请求项目自己的 Microsoft Client ID；它是启动器的公开应用编号，由开发者注册提供，详见 [登录配置说明](docs/authentication.md)。全局设置不再包含微软登录配置。
2. 添加账户会自动在系统浏览器中打开微软授权页；输入弹窗内的验证码并完成权益验证。若浏览器未打开，可点击「打开登录页」重试或复制登录地址。
3. 悬停底部实例图标区域，点击最左侧浮现的「+」创建实例（一个独立 `.minecraft` 目录）；没有实例时「+」始终可见。也可在新建弹窗中导入已有目录。macOS 的导入选择窗口默认显示隐藏文件夹；也可直接填写 `~/game/minecraft/.minecraft` 这类路径。导入会复制游戏文件并保留源目录。
4. 选择实例图标，点击启动按钮旁的小箭头，向上展开当前实例的版本列表。点击版本可切换，列表末项「下载新的版本」打开下载界面；列表较长时可滚动，下载入口始终保留。支持方向键、Home／End 导航，Esc 或点击外部收起。下载进度显示在对应实例图标下方，点击进度条可查看任务并取消下载。
5. 点击右下角「启动游戏」。已添加但未下载的版本会先安装，成功后继续启动；运行时整组启动按钮填充主题色并显示「停止游戏」，不弹出启动成功提示。双击／右键实例图标（键盘 F2），进入实例设置：点击标题图标可选择四种方块图标，点击名称可直接改名，保存后生效；名称下方的小号目录路径可打开对应 `.minecraft` 文件夹。下方可调整内存与 Java，底部提供校验修复和移除实例。

中央显示可拖拽旋转的 3D 皮肤，方向键可左右旋转、Home 复位。未登录时显示内置的原版 Steve，登录后读取账户的活动皮肤，支持普通与纤细手臂；旧账户可点击刷新加载。皮肤下载失败时回退到 Steve 并提供重试入口。皮肤通过 Rust 从 Minecraft 纹理服务下载并缓存，不经过第三方头像网站。Steve 纹理来源及校验值见 [素材记录](app/src/assets/steve.provenance.json)。左下角的五个几何图标切换配色和背景，并记住选择；选中态仅使用高亮与发光。账户、设置、日志、下载和实例管理使用居中弹窗，启动版本使用就近展开的菜单。

默认窗口尺寸与最小尺寸均为 900 × 600，可按需放大。弹窗打开时不默认聚焦操作按钮，仍可用 Tab 导航和 Esc 关闭。

全局设置的标题下方可点击打开启动器数据目录。下载源使用与主题一致的展开菜单，包含官方、官方优先和 BMCLAPI 优先三种策略，并标明回退行为；支持方向键、Home／End、Enter 选择，Esc 收起。并发数与重试次数并排显示，底部提供 Java 检测、下载记录、取消和保存；取消后重新打开恢复已保存值。

界面参考 [HUD / Sci-Fi 示例](https://josh.hu/uitest/styles/51-hud-scifi.html)，使用深色透明平面、直角、细边框与光晕；按钮和面板不使用渐变或背景模糊。五种背景分别为：海蓝蜂巢、青苔方块矩阵、蔷薇菱形晶格、鸢尾三角切面、石墨切角电路网。主题按钮自身的轮廓与背景几何对应。

背景连续平移，并共用容量受限的马尔可夫传递：信号数量约为共享边数量的 75%，每 0.3 秒尝试移动到邻边，每边最多容纳 4 份信号，目标边已满时留在原边。线色只区分激活与未激活，占用量决定线宽，显示层逐帧缓动。部分封闭区域会逐渐染色；激活边阻挡颜色，开口后颜色流向邻区并混合、淡出，平铺接缝不形成额外边界。人物画布保持透明，控件保持固定。弹窗和版本菜单打开时背景继续动画，页面隐藏或开启“减少动态效果”时暂停。

安装、修复和未登录启动均不要求添加微软账户。未选择账户时使用固定的 Player 名称与本地 UUID，不写入账户列表，也不访问登录服务或钥匙串；已选择微软账户则刷新登录并校验权益，刷新失败会显示错误。需要微软身份验证的在线服务仍须登录。

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

参见 [架构说明](docs/architecture.md) 与 [验证记录](docs/validation.md)。

Minecraft 是 Mojang/Microsoft 的商标，本项目与其没有隶属关系。
