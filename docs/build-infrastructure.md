# 构建与发布基础设施

本文描述当前构建和分发方式。dev 版本的发布文件、数据格式与接口可以调整，跨版本兼容性不作保证。

## 缓存清理

`Maintain build caches` 在 CI、发布或镜像构建完成后运行，每天 UTC 20:17 兜底运行；
手动触发默认预览。管理范围为 `v0-rust-*`、`node-cache-*-npm-*` 和 `nekocrate-tools-v1-*`。
默认分支按任务、系统、架构保留最近两代 Rust 缓存、一代 npm 缓存及每平台一代工具缓存。
总量超过 8 GiB 时按最近访问时间清理第二代缓存，每组最新一代始终保留。
其他工作流正在运行或排队时跳过，逐项删除前再次检查；近期被访问的缓存也跳过。
未知缓存、其他分支和 PR 范围交由 GitHub 保留策略处理。

离线预览：`python3 scripts/cache_cleanup.py --snapshot caches.json --default-branch neko_dev`。
实时执行需要 `GH_TOKEN`；`--apply` 才会删除，并需要 Actions 写权限。

## Linux 构建环境

镜像为 `ghcr.io/nekoos-group/nekocrate-linux-builder`，基于 Ubuntu 22.04。
`scripts/linux_build_packages.txt` 是依赖清单；镜像预装原生依赖和基础工具，
Rust、Node、Python 由 workflow 固定版本。
`/opt/nekocrate/dpkg-manifest.txt` 记录实际系统包版本。

候选镜像通过原生链接、发布脚本与冷启动对比后才更新标签；依赖准备中位数需至少快 10%。
CI 与发布通过已验证的 digest 固定构建环境。更新镜像后需同步两个 workflow 的 digest。
Fork PR 在普通 Ubuntu runner 上安装依赖。

## 发布流程与程序体积

CI 上传同提交的前端产物，发布准备复用同仓库、受信任分支的产物。
平台编译可提前进行，但完整 CI 与全部平台检查成功后才允许发布。
每个平台只编译一次：`tauri build --no-bundle -- --locked --timings`，随后由 `tauri bundle` 打包。
编译耗时 artifact 保留 14 天；工具缓存按系统、架构、Node 和 CLI 版本隔离。

Release 使用 FatLTO，保留速度优化、并行代码生成与 panic 展开。
`scripts/release_symbols.py` 在打包和更新签名前保存诊断符号：macOS 保存原始 Mach-O 后
执行 `strip -x`；Linux 保存独立符号、剥离 ELF 并添加 `.gnu_debuglink`；Windows 保存 PDB。
诊断符号及 `symbols.json` 作为 Actions artifact 保留 90 天。
符号在临时副本上处理，避免修改 Cargo 共用缓存。最终包校验信息由 BUILDINFO 记录。

Tauri 根据构建时的静态 capabilities 裁剪 IPC 命令。
系统 WebKit、GTK、D-Bus 等使用平台动态库；Rust 运行时依赖保持静态链接。
后台 IPC 任务与 JSON 文件操作共用实现；数据结构及错误传递规则保持一致。
macOS/Linux 默认窗口图标在构建时无损压缩、运行时还原，Windows 使用 EXE 内 ICO。
发布检查验证前端资源、权限、签名及安装和便携模式。

### 分发格式

| 文件 | 用途 | 是否为客户端更新所需 |
| --- | --- | --- |
| macOS `.dmg` | 面向用户的拖拽安装介质 | 手动安装入口 |
| macOS `.app.tar.gz` | 唯一的通用应用归档，便携运行与两种模式更新共用 | 是 |
| Windows `-setup.exe` | 安装版下载与自动更新共用 | 是 |
| Windows `-win-amd64.exe` / Linux `.elf` | 单个便携程序，下载与更新共用 | 是 |
| Linux `.deb` | 发行版包管理器安装 | 当前自动更新不使用 |
| `latest.json` | 版本、各平台包地址及内嵌签名 | 是 |
| `BUILDINFO.json` | 提交、平台，以及统一的文件大小与 SHA-256 校验清单 | 否，供校验与追溯 |

公开附件包括六个安装/更新文件和两个 JSON 文件。安装包与便携包共用每个平台编译出的桌面程序。发布版默认使用便携目录，
安装包部署 `nekocrate.runtime.json`（`{"schema":1,"mode":"installed"}`）：Windows
放在 EXE 旁，macOS 放在 `.app/Contents/Resources`，Linux DEB 放在
`/usr/lib/nekocrate-launcher`。Linux 根据当前程序的相对安装目录查找配置；配置损坏时终止启动并报错。

打包使用 `--no-binary-patching` 保持共享程序的原始内容。更新入口按运行时模式选择目标。
Windows/Linux 的独立文件直接复制同一可执行文件；macOS 通用 `.app` 去掉安装模式
配置后重新进行 ad hoc 签名，因此与 DMG 内 App 的程序代码相同，但签名数据和完整文件
哈希可能不同。macOS 的 Tauri bundling 关闭自动生成更新归档，再单独生成、签名一次通用包。
发布检查用同一个二进制验证配置选定安装模式、去掉配置转为便携模式、改名和迁移。

macOS 两种模式共用 `darwin-aarch64-unified` 更新目标。安装版在替换前将当前 App 的
规范路径保存到系统数据目录 `runtime-modes/<路径 SHA-256>.json`；更新后根据该记录
继续使用系统目录。记录存放在签名包外，按 App 路径隔离。保存失败时停止安装；
记录损坏时终止启动并报错。显式的运行模式配置优先。

Windows 单文件更新目标保留 `windows-x86_64-portable-exe`，附件名为 `-win-amd64.exe`；
安装包继续使用 `windows-x86_64` 和 `-setup.exe`。Linux ELF 保留 `linux-x86_64-elf`，
DEB 仍交由包管理器维护。内部目标名与公开附件名是不同概念。

构建在产物旁生成 `.sig`，
收集时检查签名对应的公钥及版本，发布时内嵌到 `latest.json`。客户端安装前验证
数字签名及版本绑定；Linux ELF 复用现有更新器的 ELF 验签与文件替换实现。
更新包的数字签名用于验证签发者，SHA-256 用于核对文件内容。操作系统的发布者认证
由 Apple 公证或 Windows 代码签名负责。

`BUILDINFO.json` 使用 `schema_version: 2`：顶层 `files` 是唯一的校验清单，记录所有
安装/更新包和 `latest.json` 的 `name`、`size`、`sha256`；`builds` 引用文件名并
记录平台与目标架构。校验范围为安装/更新包和 `latest.json`。
`build-<platform>.json` 供流水线内部汇总使用。

下载同一版本的全部附件后，在该目录校验：

```sh
jq -r '.files[] | "\(.sha256)  \(.name)"' BUILDINFO.json | shasum -a 256 --check
```

## 签名与密钥管理

CurseForge 应用凭据保存在仓库 Actions Secret `CURSEFORGE_API_KEY`。发布前检查其存在，
在三个平台的 Rust 构建步骤注入同名环境变量，由核心通过 `option_env!` 编译进程序。
修改或移除环境变量会使 Cargo 重新编译受影响的代码。元数据解析和
官方 CDN 下载使用同一凭据；请求头标记为敏感，禁止将它转发给非允许的下载主机。

普通 CI 和外部贡献者的构建无需此 Secret，其余功能照常编译测试；未注入时访问
CurseForge 会提示使用官方发行版。本地开发或自建发行版可在构建进程的环境中提供
`CURSEFORGE_API_KEY`，不要写入源码、Tauri 前端变量或命令行参数。此应用凭据会随
二进制分发，可从客户端提取。

`cargo ui` 也支持从仓库根目录 Git 已忽略的 `.env` 读取 `CURSEFORGE_API_KEY`，文件
应设置为当前用户读写。读取器提取此项的字面值，已有进程环境优先。
配置变更后重新运行 `cargo ui`；直接使用 Cargo / Tauri 构建时由调用方注入环境变量。

更新包签名、操作系统的发布者签名、玩家登录凭据是三套独立机制。

| 用途 | 当前存放位置与状态 | 构建时的处理 |
| --- | --- | --- |
| 三平台自动更新签名 | 固定公钥在 `tauri.conf.json`；私钥为 Actions Secret `TAURI_SIGNING_PRIVATE_KEY`，密码为 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 发布构建步骤注入私钥，生成的签名内嵌到 `latest.json`；私钥存储范围为 Actions Secret 和签名进程 |
| macOS 本机开发签名 | `NekoCrate Local Development` 自签身份，私钥在本机用户钥匙串且不可导出 | `cargo ui` 编译完成后签名再执行；重载继续使用相同身份 |
| macOS 公开发布者签名 | 尚未配置，现有 Release 仍为 ad hoc | 正式发布需要独立的 Developer ID Application 身份和公证配置 |
| Windows 公开发布者签名 | 尚未配置 Authenticode；现有更新包已有上面的更新签名 | 后续通过 Tauri `signCommand` 接入云签名或硬件密钥；CI 获得签名权限，私钥留在服务/HSM 中 |
| Linux 包/仓库签名 | 当前 ELF 使用上面的更新签名；DEB 用于手动安装 | APT 仓库需要独立、持久的 GPG 签名密钥 |

Windows 自签证书适用于本机测试。公开发行的发布者信任和 SmartScreen 信誉需要正式签名体系，
可使用
[Tauri 支持的云签名/硬件密钥方式](https://v2.tauri.app/distribute/sign/windows/)。
Linux ELF 使用更新包签名验证；APT 仓库使用 GPG 签名。

固定更新私钥应在仓库外保留加密备份。更换该密钥需要处理已安装客户端内置公钥的
迁移。签名 Secrets 的访问权限限定于受信任的发布工作流。

### 本机 macOS 自签身份

`scripts/macos_sign.py ensure` 在当前用户默认钥匙串中创建一次 RSA 3072 位、
有效期十年的代码签名证书，信任范围为当前用户域的代码签名。
私钥使用权限限定于 `/usr/bin/codesign`。
创建时的私钥暂存于权限为 0700 的临时目录，导入后删除；钥匙串内私钥不可导出。
`~/Library/Application Support/NekoCrate/signing/` 保存公证书和指纹。
若重装系统或删除钥匙串导致私钥丢失，需要明确重新配置身份并重新授权凭据访问。

开发工具通过 Cargo 的 target runner 在构建与执行之间调用签名脚本，保留 Tauri 的
监听、参数传递与退出逻辑。已满足固定签名要求的文件直接复用；签名或身份
验证失败时停止执行。该脚本在 macOS 上运行。
`tauri.local.conf.json` 用于本机 `.app` 打包。

自签应用的钥匙串分区身份为随编译变化的 `cdhash`，因此「始终允许」后也可能在
下一次 Rust 重编译后再次询问密码。
跨构建的稳定分区身份需要 Apple 开发者签名的 Team ID，详见[账户凭据说明](authentication.md)。

玩家的微软刷新令牌始终留在玩家自己的系统凭据库：macOS Keychain、Windows
[Credential Manager](https://learn.microsoft.com/en-us/windows/win32/secauthn/credentials-management)、
Linux [Secret Service](https://specifications.freedesktop.org/secret-service/latest/)。
Linux 密钥库锁定时可能要求解锁；没有可用凭据服务时终止凭据操作并报错。

## Cargo 安装

CLI crate 为 `nekocrate-cli`，可通过 Git 指定发布标签安装，命令名为 `nekocrate`。
Cargo 在本机编译源码，需要 Rust 和平台开发依赖。
当前尚未提供可直接发布到 crates.io 的 core/CLI 包配置。
桌面 UI 的前端资源需要单独构建，应用包和系统快捷方式由桌面打包流程生成。
