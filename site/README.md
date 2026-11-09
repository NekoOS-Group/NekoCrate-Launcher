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

GitHub Pages 使用 `.github/workflows/pages.yml`：网站相关提交部署，PR 只检查，桌面发布工作流完成后更新下载快照。构建只安装网站依赖，不触发 Rust 编译。Pages 的发布源需设为 **GitHub Actions**，默认 URL 为 `https://nekoos-group.github.io/NekoCrate-Launcher/`。路径前缀由 `SITE_BASE` 控制；更换域名或仓库路径时需同步调整部署环境变量。

网站沿用仓库的 GPL-3.0-only 许可。Logo 来源见 `app/src/assets/neko-cat-ui.provenance.json`；示意图中的方块素材来源见 `app/src/assets/blocks/provenance.json`。
