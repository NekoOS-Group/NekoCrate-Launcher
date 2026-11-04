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
记录实际安装的系统包版本。镜像验证会检查 WebKit 原生链接及 ELF 产物；
容器发布步骤只将当前 `$GITHUB_WORKSPACE` 加入 Git
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

### 可执行文件体积与诊断符号

Release 使用 `lto = "fat"`，在整个 Rust 依赖图之间优化；开发和测试配置不变。
保留原有速度优化等级、并行代码生成及 panic 展开行为。完整 LTO 增加最终链接工作，
不为便携包再次编译。Cargo 不直接剥离符号，以便先保存可追溯的诊断资料。

`scripts/release_symbols.py` 在编译之后、任何打包与更新签名之前执行：

- macOS：保存含符号的原始 Mach-O，再用 `strip -x` 去掉分发程序的局部符号，
  恢复 ad hoc 签名以执行启动检查。原文件可按地址解析函数名；当前无完整源码行调试信息。
- Linux：用 `objcopy --only-keep-debug` 保存符号文件，`strip --strip-unneeded`
  精简 ELF，并添加 `.gnu_debuglink`。保留动态链接、栈展开和 build ID 信息。
- Windows：编译时启用 `CARGO_PROFILE_RELEASE_DEBUG=1` 生成独立 PDB，保存 PDB，
  EXE 不再额外 strip；不删除异常处理表或资源段。

诊断文件与包含提交、目标架构、处理前后大小和 SHA-256 的 `symbols.json` 一起保存为
`diagnostic-symbols-<tag>-<platform>` Actions artifact，保留 90 天，不加入公开 Release
附件。需要长期排查旧版本时应在到期前归档。这里的程序哈希对应打包签名前的共享
二进制；最终安装包哈希仍由 BUILDINFO 记录。符号处理在临时副本上完成后替换程序，
避免修改 Cargo 通过硬链接共用的缓存文件。

直接返回 JSON 的后台 IPC 操作通过统一的 blocking task 入口执行，进入 Tokio
之前擦除闭包和结果的类型，避免每种命令都生成一套任务调度、join 和析构代码。
序列化在同一个后台任务中完成，保持原有 JSON 结构与错误链；不改变下载状态机。
JSON 读写的文件操作也由非泛型公共函数执行，保留各类型的直接反序列化。

2026-11-04 的 macOS ARM64 同一工作区、同一前端产物对比（原始可执行文件，
不是 DMG 或压缩包）：

| 构建 | 字节数 | MiB |
| --- | ---: | ---: |
| 原配置基线 | 28,139,968 | 26.84 |
| FatLTO | 23,706,528 | 22.61 |
| 加上后台命令与 JSON 文件操作去重 | 23,464,304 | 22.38 |
| 再剥离局部符号、恢复 ad hoc 签名 | 19,674,576 | 18.76 |

总计减少 30.08%；只比较同样剥离且重新签名的程序，代码去重额外节省
165,888 字节。Mach-O 符号分析中 Tokio `raw::poll` 的不同地址由 324 个降到 301 个。
二进制单独 gzip-6 压缩由 11,953,721 降到 9,939,477 字节；这不是整个应用包的大小。
Windows/Linux 的 FatLTO 收益需要原生发布构建测量，不能套用 macOS 百分比。

对已发布 dev0.9.7 文件的分析副本，仅做符号精简（不包含本轮 FatLTO 和代码去重）：

| 平台 | 原始文件 | 精简后 | 降幅 |
| --- | ---: | ---: | ---: |
| Linux ELF | 37,964,720 字节 / 36.21 MiB | 29,388,120 字节 / 28.03 MiB | 22.6% |
| Windows EXE | 28,101,120 字节 / 26.80 MiB | 28,101,120 字节 / 26.80 MiB | 0% |

这些副本未分发，也不代表当前源码重新构建的最终大小。Windows 的符号已主要位于
独立 PDB，不能靠同样的 strip 获得 Linux 的收益；最终收益应比较新构建的 EXE。

### 运行时代码与链接审计

2026-11-04 在上述 FatLTO 基础上继续检查实际 Mach-O 符号、各平台动态库表及
排除构建依赖/过程宏后的 Cargo 依赖树。最大的可移除部分是未使用的 Tauri IPC
命令，而不是另一套浏览器或 HTTP 客户端。

`build.removeUnusedCommands = true` 让 Tauri 按静态 capabilities 裁掉未授权的命令。
将 `core:default` 改为实际需要的事件订阅/取消订阅、窗口标题与最小化/恢复/聚焦，
保留文件选择与链接打开。应用不使用运行时新增 capability，因此关闭 `dynamic-acl`，
其余原有桌面默认功能保留。Rust 直接调用的菜单、文件定位、更新安装仍保留；裁掉的
是未使用的前端入口。机制与限制见 [Tauri 官方说明](https://v2.tauri.app/concept/size/#remove-unused-commands)。

同一前端、图标、FatLTO、剥离规则及 gzip-6 的最终实测：

| 文件 | 本轮前 | 本轮后 | 减少 |
| --- | ---: | ---: | ---: |
| macOS ARM64 可执行文件 | 19,674,544 字节 | 17,159,680 字节 | 12.78% |
| 通用 `.app.tar.gz` | 10,903,891 字节 | 10,124,618 字节 | 7.15% |

本阶段的包为 9.66 MiB，但仍高于十进制 10 MB 约 125 KB。未修改任何图片、字体、
前端代码或压缩级别。按不同符号地址统计，Tokio `raw::poll` 从 301 个降到 93 个；
Tauri 异步响应相关函数的地址区间合计约 526 KiB 降到 83 KiB。这些是含内联影响的
符号归属估计，不是移除整个 crate 可直接节省的大小。

依赖和动态链接的结论：

| 项目 | 实际情况与处理 |
| --- | --- |
| Linux `libdbus` | `keyring/vendored` 原先把 C 库静态编入程序；现移除此 feature，通过 pkg-config 链接系统 `libdbus-1.so.3`。构建镜像已有 `libdbus-1-dev`，DEB 显式增加 `libdbus-1-3` 运行依赖，原生发布检查验证 ELF 的动态依赖。 |
| macOS WebKit/AppKit/Security 等 | 实际 Mach-O 已链接系统 framework，不在包内静态携带。 |
| Linux GTK/WebKitGTK、Windows 系统 DLL | 已发布文件的动态库表证实在使用系统运行库；继续保留这些链接。 |
| Rust 标准库、Tokio、Reqwest/Rustls | 静态链接保留。系统没有可直接复用的匹配 Rust 动态库；拆分后仍须分发库和管理 ABI/加载路径，不等于减少总包体积。 |
| `flate2`/`miniz_oxide` | 可研究系统 zlib，但最终符号中压缩/解压核心仅约 29 KiB；暂不为这点体积切换后端及其性能特征。 |
| `base64` 多版本 | updater 使用 0.22，HTTP/应用使用 0.23；最终可见相关代码约 11 KiB。没有为统一版本而维护上游分叉。 |
| `getrandom`、CoreFoundation/CoreGraphics 多版本 | 不同上游要求的适配层；依赖树中的多个版本不代表复制了系统库。最终 CoreFoundation 具名函数约 1 KiB，CoreGraphics 大部分调用已内联或消除。 |
| 图片、TOML、SHA-2 等依赖 | 必须区分编译期和运行期。没有新的 `image`、`sysinfo` 或第二套 Reqwest；PNG 仍参与原生菜单图标，TOML 用于 Mod 元数据/配置，Linux SHA-2 用于 Secret Service。不能按锁文件名称直接删库。 |

移除 vendored feature 同时清除了锁文件中的 OpenSSL/openssl-src 等可选依赖。
原先这些项没有出现在审计到的 macOS/Linux 实际运行时 OpenSSL 链接路径中，因此
不将锁文件减少行数计为可执行文件节省。

Linux 旧 dev0.9.7 的静态 D-Bus 有 601 个 C 函数符号、合计 107,632 字节机器码，
还不含常量等数据。此数值只说明旧文件中确实有静态副本；新的 Linux/Windows
原生构建与最终大小尚未验证。D-Bus 的系统链接要求见 [dbus-rs 文档](https://docs.rs/crate/dbus/0.9.12)，
Rust 库链接约束见 [Rust Reference](https://doc.rust-lang.org/reference/linkage.html)。

验证包括工作区 Rust 测试、原生 JS SDK 的文件选择/链接/窗口/拖入监听调用测试、
Clippy，以及最终 macOS 包的签名、嵌入资源、编译后权限、两种运行模式和迁移检查。
`--verify-frontend` 直接验证生成后的 ACL，避免只看源 JSON 而漏掉发布构建差异。

### IPC 泛型代码去重

在上述命令裁剪后继续测量，保留两项有实际收益的改动：

- 桌面 `Request` 和资源管理 `Action` 继续接受原来的 `op` / `kind` JSON 字段。
  `tagged_json` 将同一 JSON 对象交给 Serde 的 `EnumAccess`，通过 remote derive
  复用字段类型、默认值和必填校验，避免每个内部标签变体生成一套 `ContentDeserializer`。
  此适配器只用于 JSON IPC；磁盘配置与网络响应的读取路径不变。
- 后台 IPC 操作通过 `blocking` 共用 Tokio 任务入口。直接回传前端的结果共用
  `Value`；路径和复制计划等中间结果仍保留 Rust 类型，按返回类型共享任务实现，
  不经过 JSON 往返或 `Any` 转换。增加的闭包装箱只发生在这些界面操作中，未修改
  每文件下载、校验或渲染的热路径。

同一前端、资源、FatLTO、符号处理和 gzip-6 下，2026-11-04 macOS ARM64 本地实测：

| 阶段 / 文件 | 字节 |
| --- | ---: |
| 本轮前，可执行文件 | 17,159,680 |
| 仅 IPC JSON 解析去重后 | 16,894,928 |
| 再合并直接返回 JSON 的后台调用 | 16,795,568 |
| 再合并有类型中间结果，最终可执行文件 | 16,745,936 |
| 本轮前，完整通用 `.app.tar.gz` | 10,124,618 |
| 最终完整通用 `.app.tar.gz` | 9,984,058 |

可执行文件减少 413,744 字节（2.41%），完整包减少 140,560 字节（1.39%）。
本地完整归档为 9.984 MB / 9.52 MiB，低于十进制 10 MB；未压缩的程序仍为
16.746 MB。这不代表 Windows/Linux 构建大小，也不保证后续增加功能后仍低于此值。

按 Mach-O 不同函数地址估算，`ContentDeserializer` 相关实现从 114 份降至 5 份，
其地址区间合计由 385,968 降至 7,972 字节；新的 JSON 适配与派生解析自身仍有约
199 KB 代码，因此不能将旧解析代码的全部体积计为净节省。Tokio `raw::poll` 从
93 份降至 80 份。最终净收益以上表的实际文件为准。

验证：384 项工作区 Rust 测试通过、8 项原有测试忽略；Clippy、格式和原生 JS SDK
权限测试通过。新增回归覆盖借用/拥有的 JSON、缺省值、嵌套命令、中文路径、完整
u64 范围及非法输入。正式二进制的 IPC Future 仍为 424 字节，继续使用堆上分派，
避免重现 Windows 启动栈溢出。最终归档解压后通过签名、前端资源、ACL 和便携模式
验证；同一程序的安装模式、改名和迁移检查也通过。未发布本轮产物。

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

合并 macOS 更新包后，共公开六个安装/更新文件和两个 JSON 文件；文件名不含
`portable`，不再构建或发布 AppImage、portable ZIP 或第二份 macOS 更新归档。每个平台只编译一次桌面程序，安装包与便携包共用该程序；
不再启用 `portable` feature 或进行第二次编译与链接。发布版默认使用便携目录，
安装包部署 `nekocrate.runtime.json`（`{"schema":1,"mode":"installed"}`）：Windows
放在 EXE 旁，macOS 放在 `.app/Contents/Resources`，Linux DEB 放在
`/usr/lib/nekocrate-launcher`。Linux 只查找当前程序对应的相对安装目录，避免本机
另一个已安装版本影响便携副本；配置损坏时明确报错，不静默切换数据目录。

打包使用 `--no-binary-patching`，防止 Tauri 将安装包类型写入共享程序，尤其避免
便携 ELF 被标记为 DEB 后调用包管理器更新。Windows 更新入口仍按运行时模式选择目标。
Windows/Linux 的独立文件直接复制同一可执行文件；macOS 通用 `.app` 去掉安装模式
配置后重新进行 ad hoc 签名，因此与 DMG 内 App 的程序代码相同，但签名数据和完整文件
哈希可能不同。macOS 的 Tauri bundling 关闭自动生成更新归档，再单独生成、签名一次通用包。
发布检查用同一个二进制验证配置选定安装模式、去掉配置转为便携模式、改名和迁移。

macOS 两种模式共用 `darwin-aarch64-unified` 更新目标。安装版在替换前将当前 App 的
规范路径保存到系统数据目录 `runtime-modes/<路径 SHA-256>.json`；更新后根据该记录
继续使用系统目录。记录位于签名包外，不修改下载包、不破坏签名，不影响其他位置的
副本。保存失败时停止安装；损坏的记录明确报错，不静默切换成新的便携数据目录。
便携更新不写这份全局记录。显式的运行模式配置仍优先。

Windows 单文件更新目标保留 `windows-x86_64-portable-exe`，附件名改为 `-win-amd64.exe`；
安装包继续使用 `windows-x86_64` 和 `-setup.exe`。Linux ELF 保留 `linux-x86_64-elf`，
DEB 仍交由包管理器维护。内部目标名与公开附件名是不同概念。

旧 macOS 客户端不会保存包外模式，因此不把通用包挂到旧 `darwin-aarch64` 或
`darwin-aarch64-portable` 目标；这些客户端需要手动替换一次，安装用户用 DMG，便携
用户替换 App 并保留旁边的数据。旧 Windows 便携客户端校验了 `-portable.exe` 文件名，
也需手动替换一次。新的实例 ZIP 导出同时识别统一包和历史便携包，避免误用旧安装模式
App；导出的 App 外层目录不再含 Portable 字样。此前的旧 AppImage 和 dev0.9.4 便携
迁移要求仍然适用。

不提供 `.app.zip`、独立 `.sig` 或 `SHA256SUMS`。构建在产物旁生成 `.sig`，
收集时检查签名对应的公钥及版本，发布时内嵌到 `latest.json`。客户端安装前验证
数字签名及版本绑定；Linux ELF 复用现有更新器的 ELF 验签与文件替换实现。
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
| Linux 包/仓库签名 | 当前 ELF 使用上面的更新签名；DEB 用于手动安装 | 若增加 APT 仓库，使用独立、持久的 GPG 签名密钥；不每次构建重新生成 |

Windows 的发布者证书与更新包签名不同。自签证书可以用于本机测试，但不能让其他
用户的 Windows 自动信任发布者或保证消除 SmartScreen 提示。正式发布可使用
[Tauri 支持的云签名/硬件密钥方式](https://v2.tauri.app/distribute/sign/windows/)。
Linux ELF 使用同一套更新签名验证；未来如增加 APT 仓库，可另行配置仓库 GPG
签名，不能用普通文件校验和替代发布者签名。

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

该自签方案只固定签名要求，无法固定 macOS 钥匙串的分区身份；自签应用的分区仍为
随编译变化的 `cdhash`，因此「始终允许」后也可能在下一次 Rust 重编译后再次询问密码。
跨构建的稳定分区身份需要 Apple 开发者签名的 Team ID，详见[账户凭据说明](authentication.md)。

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
