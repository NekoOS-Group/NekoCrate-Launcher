# 猫咪盒子网站

下载首页、功能介绍与使用文档，独立于桌面端构建。直接复用启动器的 `WorldBackdrop`、几何流动渲染和主题定义；不会打包 Tauri、账户信息或本机游戏数据。

在仓库根目录运行：

```sh
npm --prefix site ci
npm --prefix site run dev
```

本地地址：`http://127.0.0.1:1422/NekoCrate-Launcher/`。构建用 `npm --prefix site run build`，产物在 `site/dist`；预览用 `npm --prefix site run preview`。

- `/`：下载页，按平台、架构和运行模式匹配真实发布附件。
- `/features/`：功能与设计，使用同一套主题与交互示意。
- `/docs/`：使用文档，支持全文检索、章节链接和复制命令。

`src/release.json` 是经过校验的公开发布快照。页面先显示快照，再尝试刷新 GitHub Releases（包含预发布），网络失败时保留可用链接。浏览器缓存有效期 30 分钟。更新快照运行 `npm --prefix site run sync-release`；CI 使用只读 `GH_TOKEN` 获取数据，密钥不会写入静态产物。

正式地址是 `https://bzy.moe/NekoCrate-Launcher/`，由 `bzy-nya/bzy-nya.github.io` 的 Pages 工作流将网站构建到个人主页的同名子目录；原主页保持独立。该工作流每小时检查本仓库成功的网站发布，只有上游发布或个人主页改变时才重新构建、部署，也可在个人主页仓库手动运行 **Publish website** 立即同步。不会将整个域名绑定到启动器仓库。

本仓库的 `.github/workflows/pages.yml` 仍提供 `https://nekoos-group.github.io/NekoCrate-Launcher/` 作为备用地址：网站相关提交部署，PR 只检查，桌面发布工作流完成后更新下载快照。页面的 canonical 指向正式地址。两处构建只安装网站依赖，不触发 Rust 编译。路径前缀由 `SITE_BASE` 控制，当前均为 `/NekoCrate-Launcher/`。Pages 的发布源需设为 **GitHub Actions**。

网站沿用仓库的 GPL-3.0-only 许可。Logo 来源见 `app/src/assets/neko-cat-ui.provenance.json`；示意图中的方块素材来源见 `app/src/assets/blocks/provenance.json`。
