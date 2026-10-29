# 构建缓存与 Linux 环境

## 缓存清理

`Maintain build caches` 在 CI、发布或镜像构建完成后运行，并在每天 UTC 20:17
兜底运行。也可手动触发；手动默认只预览。

- 仅管理本仓库约定的 `v0-rust-*`、`node-cache-*-npm-*` 与 `nekocrate-tools-v1-*` 缓存。
- 清理 `refs/tags/dev*` 和旧 `refs/heads/refs/tags/dev*` 范围内的缓存。
  当前发布在默认分支执行、检出标签对应提交，因此不会再依赖这些旧范围。
- 默认分支按构建任务、系统、架构分组：保留最近两代 Rust 缓存、一代 npm 缓存及每个平台一代构建工具缓存。
- 总量超过 8 GiB 时，按最近访问时间清理第二代缓存；每组最新一代始终保留。
  这是软上限，不能通过删除唯一缓存或未知缓存强行满足。
- 其他工作流正在运行或排队时跳过；逐项删除前再次检查。清理开始后被访问过
  的缓存也跳过。未知命名、其他分支和 PR 范围交由 GitHub 自身的保留策略处理。
- 删除的是 Actions 构建缓存，不是 Release 文件、workflow artifacts 或源码。

策略实现和离线预览：

```sh
python3 scripts/cache_cleanup.py --snapshot caches.json --default-branch neko_dev
```

实时运行需要 `GH_TOKEN` 的 Actions 读权限；传 `--apply` 才会删除，并需要写权限。
工作流使用自身的短期 `GITHUB_TOKEN`。

## Linux 预装依赖镜像

镜像位于 `ghcr.io/nekoos-group/nekocrate-linux-builder`，基于 Ubuntu 22.04，保持
现有 Linux 发布的 glibc 基线。它只预装原生依赖和基础工具；Rust、Node、Python
继续由 workflow 固定版本，避免为了省几秒工具安装而膨胀镜像。

`scripts/linux_build_packages.txt` 是镜像与 apt 基准测试共用的依赖清单。
镜像不包含项目源码、账户或签名密钥。镜像内 `/opt/nekocrate/dpkg-manifest.txt`
记录实际安装的系统包版本。镜像验证会执行 `file`，避免编译和 DEB 打包成功后才
发现 AppImage 工具缺失；容器发布步骤只将当前 `$GITHUB_WORKSPACE` 加入 Git
的 `safe.directory`，使产物清单能读取实际检出提交。

`Build Linux dependency image` 在 Dockerfile、依赖清单或相关脚本变更时运行，
也可手动触发以刷新系统包。候选镜像按 digest 测试，六个全新的 Ubuntu 22.04
runner 分别执行三次 apt 安装和三次镜像冷拉取、启动、依赖检查。镜像已在本地
时测试直接失败，避免热缓存污染数据。

中位数快至少 10%，且容器中的 Actions、Rust/Node/Python 工具准备、WebKit 原生
链接和发布脚本测试通过后，才更新 `jammy-v1` 标签。结果保留在 workflow summary
和 `linux-setup-comparison` artifact。镜像构建与推送的一次性成本不算入每次拉取
耗时；镜像失败或没有收益时保留原有标签。

CI 与发布工作流使用已经验证的 **digest**，不会因为 `jammy-v1` 标签更新就静默更换
构建环境。重建后应先查看验证与对比结果，再同步更新两个 workflow 中的 digest。
Fork PR 在普通 Ubuntu runner 上安装依赖，避免依赖外部贡献者没有权限读取的镜像。

2026-10-25 的实测（workflow run `40117837250`，补齐 AppImage 所需 `file` 命令）：

| 方法 | 三次耗时（秒） | 中位数（秒） |
| --- | --- | --- |
| apt 安装并检查依赖 | 39.740 / 45.054 / 33.408 | 39.740 |
| 镜像冷拉取、启动并检查依赖 | 19.461 / 21.208 / 19.965 | 19.965 |

依赖准备阶段每次节省 19.775 秒，约 49.8%。这不是整条发布流水线的加速比例，
也不包含首次镜像构建、工具链准备、Rust 编译或安装包压缩。

首次清理移除了 24 个缓存，释放 5.211 GiB，仓库缓存由 10.631 GiB 降至 5.421 GiB。

## 发布文件的职责

| 文件 | 用途 | 是否为客户端更新所需 |
| --- | --- | --- |
| macOS `.dmg` | 面向用户的拖拽安装介质 | 手动安装入口 |
| macOS `.app.tar.gz` | Tauri updater 使用的应用归档 | 是 |
| Windows `.exe` / Linux `.AppImage` | 手动安装与自动更新共用 | 是 |
| Linux `.deb` | 发行版包管理器安装 | 当前自动更新不使用 |
| `latest.json` | 版本、各平台包地址及内嵌签名 | 是 |
| `BUILDINFO.json` | 提交、平台，以及统一的文件大小与 SHA-256 校验清单 | 否，供校验与追溯 |

从 dev0.5 的附件精简开始，共公开五个安装/更新包和两个 JSON 文件，不再提供
`.app.zip`、独立 `.sig` 或 `SHA256SUMS`。Tauri 仍在原生 bundle 目录生成 `.sig`，
收集时检查签名对应的公钥及版本，将内容写入内部平台清单；发布时再次检查并
内嵌到 `latest.json`。客户端读取内嵌签名，安装前的数字签名验证不变。
数字签名证明更新包由持有私钥的一方签发；普通 SHA-256 校验和本身不提供发布者
身份保证。Tauri 更新签名也不同于 Apple 公证或 Windows 发布者代码签名。

`BUILDINFO.json` 使用 `schema_version: 2`：顶层 `files` 是唯一的校验清单，记录所有
安装/更新包和 `latest.json` 的 `name`、`size`、`sha256`；`builds` 仅引用文件名并
记录平台与目标架构，避免重复存储哈希。BUILDINFO 不对自身求哈希，避免循环依赖。
`build-<platform>.json` 是流水线内部用于汇总的清单，不是额外公开的 Release 附件。

下载同一版本的全部附件后，在该目录校验：

```sh
jq -r '.files[] | "\(.sha256)  \(.name)"' BUILDINFO.json | shasum -a 256 --check
```

## 签名与密钥管理

CurseForge 应用凭据保存在仓库 Actions Secret `CURSEFORGE_API_KEY`。发布前检查其存在，
仅在三个平台的 Rust 构建步骤注入同名环境变量，由核心通过 `option_env!` 编译进程序。
修改或移除环境变量会使 Cargo 重新编译受影响的代码，不会继续使用旧缓存中的凭据。
设置页、IPC、数据库及玩家钥匙串均不再读取或保存 CurseForge Key。元数据解析和
官方 CDN 下载使用同一凭据；请求头标记为敏感，禁止将它转发给非允许的下载主机。

普通 CI 和外部贡献者的构建无需此 Secret，其余功能照常编译测试；未注入时访问
CurseForge 会提示使用官方发行版。本地开发或自建发行版可在构建进程的环境中提供
`CURSEFORGE_API_KEY`，不要写入源码、Tauri 前端变量或命令行参数。此应用凭据会随
二进制分发，可从客户端提取；它与绝不进入安装包的更新签名私钥用途不同。

`cargo ui` 也支持从仓库根目录 Git 已忽略的 `.env` 读取 `CURSEFORGE_API_KEY`，文件
应仅允许当前用户读写。只读取此项，不执行 shell、不展开 `$`，已有进程环境优先。
配置变更后重新运行 `cargo ui`；直接使用 Cargo / Tauri 构建时由调用方注入环境变量。

更新包签名、操作系统的发布者签名、玩家登录凭据是三套独立机制。

| 用途 | 当前存放位置与状态 | 构建时的处理 |
| --- | --- | --- |
| 三平台自动更新签名 | 固定公钥在 `tauri.conf.json`；私钥为 Actions Secret `TAURI_SIGNING_PRIVATE_KEY`，密码为 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 仅发布构建步骤注入，内嵌签名进入 `latest.json`；私钥不进入安装包、缓存或 artifacts |
| macOS 本机开发签名 | `NekoCrate Local Development` 自签身份，私钥在本机用户钥匙串且不可导出 | `cargo ui` 编译完成后签名再执行；重载继续使用相同身份 |
| macOS 公开发布者签名 | 尚未配置，现有 Release 仍为 ad hoc | 后续使用独立的 Developer ID Application 身份和公证配置，不上传本机开发私钥 |
| Windows 公开发布者签名 | 尚未配置 Authenticode；现有更新包已有上面的更新签名 | 后续通过 Tauri `signCommand` 接入云签名或硬件密钥；CI 获得签名权限，私钥留在服务/HSM 中 |
| Linux 包/仓库签名 | 当前 AppImage 使用上面的更新签名；DEB 用于手动安装 | 若增加 AppImage GPG 或 APT 仓库，使用独立、持久的 GPG 签名密钥；不每次构建重新生成 |

Windows 的发布者证书与更新包签名不同。自签证书可以用于本机测试，但不能让其他
用户的 Windows 自动信任发布者或保证消除 SmartScreen 提示。正式发布可使用
[Tauri 支持的云签名/硬件密钥方式](https://v2.tauri.app/distribute/sign/windows/)。
Linux 没有要求普通应用持有类似 macOS 的本机签名证书；可按分发渠道增加
[AppImage GPG 签名](https://v2.tauri.app/distribute/sign/linux/)，但 AppImage
本身不会自动验证该签名，不能替代启动器现有的更新签名验证。

固定更新私钥应在仓库外保留加密备份。更换该密钥需要处理已安装客户端内置公钥的
迁移，不能简单重生成。签名 Secrets 只供受信任的发布工作流使用，PR 测试不需要，
也不应通过缓存传递；现有 Linux 构建镜像不含任何签名密钥。

### 本机 macOS 自签身份

`scripts/macos_sign.py ensure` 在当前用户默认钥匙串中创建一次 RSA 3072 位、
有效期十年的代码签名证书；仅添加当前用户域的代码签名信任，不改变 HTTPS 信任。
私钥只授权 `/usr/bin/codesign` 使用，不使用允许任意应用访问的 `security import -A`。
创建时的私钥暂存于权限为 0700 的临时目录，导入后删除；钥匙串内私钥不可导出。
`~/Library/Application Support/NekoCrate/signing/` 只保存公证书和指纹，不能据此签名。
若重装系统或删除钥匙串导致私钥丢失，需要明确重新配置身份并重新授权凭据访问。

开发工具通过 Cargo 的 target runner 在构建与执行之间调用签名脚本，保留 Tauri 的
监听、参数传递与退出逻辑。已经满足固定签名要求的文件不重复改写；签名或身份
验证失败时停止执行，不退回临时签名。Windows/Linux 的 `cargo ui` 路径不执行该脚本。
`tauri.local.conf.json` 提供本机 `.app` 打包覆盖配置，不由 Release 工作流加载。

玩家的微软刷新令牌始终留在玩家自己的系统凭据库：macOS Keychain、Windows
[Credential Manager](https://learn.microsoft.com/en-us/windows/win32/secauthn/credentials-management)、
Linux [Secret Service](https://specifications.freedesktop.org/secret-service/latest/)。
它们不是构建签名密钥；Linux 密钥库锁定时可能要求解锁，没有可用服务时明确报错，
不回退到明文保存。

## Cargo 安装

CLI 已有独立二进制 crate，可通过 Git 安装指定版本：

```sh
cargo install --git https://github.com/NekoOS-Group/NekoCrate-Launcher.git \
  --tag dev0.5 --locked nekocrate-cli
```

安装的命令叫 `nekocrate`。Cargo 在用户本机编译源码，需要 Rust 和平台开发依赖，
不是下载 GitHub Release 的安装包。发布到 crates.io 则需先为 core/CLI 补全发布
元数据、给 path 依赖补充版本、发布 core 后再发布 CLI；单纯上传 `.crate` 到 Release
不会启用 `cargo install nekocrate-cli`。

已在 macOS ARM64 用上述 Git 命令安装到独立临时目录，并验证 `--version` 和
`--help`。直接执行 `cargo package -p nekocrate-cli --no-verify --locked --offline`
会明确报告 core path 依赖缺少版本要求，当前尚不是可直接上传 crates.io 的包。

桌面 UI 还需处理前端产物打包：当前 `frontendDist` 指向 crate 外的 `app/dist`，且
Cargo 不会执行 Tauri CLI 的 `beforeBuildCommand`。需要将预构建前端包含进 desktop
crate、配置对应资源路径，并明确系统依赖与安装方式。`cargo install` 安装二进制，
不会自动制作 macOS `.app`、DMG 或系统开始菜单快捷方式。


## dev0.9.1 发布流程优化

前端测试、构建、产物校验和发布脚本测试通过后，CI 立即上传 `ci-frontend`；Rust
全量测试、Clippy 与其他平台检查继续运行。发布准备只复用同一提交、同一仓库、
受信任分支 CI 的产物。三个发布任务可提前进行编译；独立的 `checks` 发布门禁
持续检查选定 CI run，只有其完整成功、且全部平台完成，发布任务才会执行。
失败、取消或提交不匹配均禁止发布，不会把“前端产物存在”当成完整 CI 通过。

`tauri build --no-bundle -- --locked --timings` 只编译一次，随后 `tauri bundle`
打包并签名。两步使用相同目标、配置及默认 features。Cargo 的每个 crate 耗时保留
在 `build-timings-<平台>` Actions artifact 中 14 天，不加入 Release 下载附件。

已安装的 Tauri CLI 和 `target/.tauri` 工具按 OS / 架构 / Node 22 / CLI 版本独立缓存，
不再随整个前端 lockfile 的变化重新下载安装。依赖编译缓存仍由 rust-cache 管理；
发布任务关闭 workspace crate 缓存，因为每次发布的版本变化都会使这些 crate 重新
编译。已有旧缓存可正常恢复，下一次需要保存缓存时才剔除这些项目产物，避免为了
迁移缓存而强迫本次发布冷编译。CI 的测试缓存策略保持不变。

作为基线，dev0.9 的 Rust 编译分别为 macOS 98 秒、Linux 75 秒、Windows 162 秒；
打包约 8 / 31 / 13 秒，Rust 缓存恢复约 16 / 15 / 40 秒。Linux 另有约 50 秒容器初始化。
这些是单次运行数据，包含当时依赖变动和 runner 差异；不能把不同提交之间的全部
耗时变化归因于优化。此次保留现有编译优化等级、链接器、安装包压缩及 Linux 镜像，
优先减少流程串行等待和重复工具准备，再用逐 crate 报告决定进一步调整。
