# 构建缓存与 Linux 环境

## 缓存清理

`Maintain build caches` 在 CI、发布或镜像构建完成后运行，并在每天 UTC 20:17
兜底运行。也可手动触发；手动默认只预览。

- 仅管理本仓库约定的 `v0-rust-*` 与 `node-cache-*-npm-*` 缓存。
- 清理 `refs/tags/dev*` 和旧 `refs/heads/refs/tags/dev*` 范围内的缓存。
  当前发布在默认分支执行、检出标签对应提交，因此不会再依赖这些旧范围。
- 默认分支按构建任务、系统、架构分组：保留最近两代 Rust 缓存和一代 npm 缓存。
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
记录实际安装的系统包版本。

`Build Linux dependency image` 在 Dockerfile、依赖清单或相关脚本变更时运行，
也可手动触发以刷新系统包。候选镜像按 digest 测试，六个全新的 Ubuntu 22.04
runner 分别执行三次 apt 安装和三次镜像冷拉取、启动、依赖检查。镜像已在本地
时测试直接失败，避免热缓存污染数据。

中位数快至少 10%，且容器中的 Actions、Rust/Node/Python 工具准备、WebKit 原生
链接和发布脚本测试通过后，才更新 `jammy-v1` 标签。结果保留在 workflow summary
和 `linux-setup-comparison` artifact。镜像构建与推送的一次性成本不算入每次拉取
耗时；镜像失败或没有收益时保留原有标签。

## 发布文件的职责

| 文件 | 用途 | 是否为客户端更新所需 |
| --- | --- | --- |
| macOS `.dmg` | 面向用户的拖拽安装介质 | 手动安装入口 |
| macOS `.app.zip` | 同一 `.app` 的 ZIP 分发形式，与 DMG 内容重叠 | 否，可精简 |
| macOS `.app.tar.gz` | Tauri updater 使用的应用归档 | 是 |
| Windows `.exe` / Linux `.AppImage` | 手动安装与自动更新共用 | 是 |
| Linux `.deb` | 发行版包管理器安装 | 当前自动更新不使用 |
| `.sig` | 更新包的数字签名 | 签名内容必须保留；独立附件可选 |
| `latest.json` | 版本、各平台包地址及内嵌签名 | 是 |
| `BUILDINFO.json` | 提交、平台、文件大小、哈希与构建追溯信息 | 否 |
| `SHA256SUMS` | 方便用户用标准工具检查下载文件完整性 | 否 |

独立 `.sig` 的内容也写入 `latest.json`，客户端读取内嵌签名。当前发布脚本仍要求
上传独立签名文件；若要精简附件，需要同时调整产物校验与上传规则。
数字签名证明更新包由持有私钥的一方签发；普通 SHA-256 校验和本身不提供发布者
身份保证。Tauri 更新签名也不同于 Apple 公证或 Windows 发布者代码签名。

`BUILDINFO.json` 与 `SHA256SUMS` 的哈希信息有重叠，但一个面向机器追溯，一个方便
命令行校验。可以将 BUILDINFO 转为仅保留在 CI artifact，保留公开的校验和文件。
`build-<platform>.json` 是流水线内部用于汇总的清单，不是额外公开的 Release 附件。

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

桌面 UI 还需处理前端产物打包：当前 `frontendDist` 指向 crate 外的 `app/dist`，且
Cargo 不会执行 Tauri CLI 的 `beforeBuildCommand`。需要将预构建前端包含进 desktop
crate、配置对应资源路径，并明确系统依赖与安装方式。`cargo install` 安装二进制，
不会自动制作 macOS `.app`、DMG 或系统开始菜单快捷方式。
