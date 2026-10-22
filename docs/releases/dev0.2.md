猫咪盒子启动器 dev 0.2，应用版本 `0.2.0-dev.1`。

## 下载

| 平台 | 安装包 |
| --- | --- |
| macOS Apple Silicon（M1 及更新芯片） | `mac-appsilicon.dmg`；另提供 `.app.zip` |
| Windows AMD64（Intel / AMD 64 位） | `win-amd64-setup.exe` |
| Linux AMD64（Intel / AMD 64 位） | `linux-amd64.AppImage`、`linux-amd64.deb` |

文件名均以 `NekoCrate-Launcher-dev0.2-` 开头。`SHA256SUMS` 提供文件校验值，`BUILDINFO.json` 记录源代码提交、版本、目标架构及每个安装包的校验值。

## 本版变化

- 更新像素猫咪盒子图标：Creeper 和浅浅探头的 Minecraft 猫猫共同藏在盒中。
- 完善五套透明 HUD 主题、主题选择点击范围、弹窗与设置分页动效；优化动态背景和皮肤渲染，保留原有画面效果。
- 实例支持拖拽排序和自动补位动效，提供更多 Minecraft 方块图标，下载进度围绕实例图标显示。
- 导入已有 `.minecraft` 目录时直接引用原路径，不再复制到启动器数据目录；支持隐藏目录和 `~` 路径。
- 空实例显示「下载游戏」，已安装实例可在上拉菜单中切换版本；优化按钮文字和版本名称切换动效。
- 增加简体中文、English、跟随系统语言；可选择「游戏启动时跟随」同步 Minecraft 语言，仅点击开关本身触发。
- 全局设置分为通用与网络页面，显示启动器版本并检查 GitHub 发布更新；可打开对应平台的安装包下载链接。
- 修复游戏下载期间无法编辑、保存全局设置，以及下载结束误关弹窗的问题；同时保护重新打开的设置草稿，防止异步旧数据覆盖当前状态。

## 安装与已知限制

- 这是开发预览版；三个平台的自动化测试与构建不等于全部功能已完成实机交互和真实游戏进入验收。
- macOS 要求 11.0 或更新版本，采用 ad-hoc 签名，未做 Apple 公证。Windows 安装程序尚无发布者代码签名，使用 WebView2，安装时可能需要联网安装运行时。
- Linux 包在 Ubuntu 22.04 构建；Debian 包由系统安装依赖，AppImage 需要执行权限，部分发行版还需要 FUSE 2。
- 微软登录仍需配置有效的 Microsoft Client ID，并满足 Minecraft API 访问要求；见仓库 `docs/authentication.md`。也可不登录，以本地 Player 身份启动游戏。
- 支持官方原版 Java Edition 1.13+ 的元数据协议；Apple Silicon 使用具备 ARM 原生库的 1.19+。各版本未逐一验证，暂不支持模组加载器和整合包安装。
- 更新检查提供安装包链接，当前不支持自动替换已安装的启动器。首次从 dev0.1 升级请下载并安装本版。
- Minecraft 纹理归 Mojang Studios / Microsoft 所有，本项目不是 Minecraft 官方产品。
