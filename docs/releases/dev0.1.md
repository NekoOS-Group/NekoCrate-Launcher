猫咪盒子启动器的首个开发预览版，应用版本 `0.1.0-dev.1`。

## 下载

| 平台 | 安装包 |
| --- | --- |
| macOS Apple Silicon（M1 及更新芯片） | `mac-appsilicon.dmg`；另提供 `.app.zip` |
| Windows AMD64（Intel / AMD 64 位） | `win-amd64-setup.exe` |
| Linux AMD64（Intel / AMD 64 位） | `linux-amd64.AppImage`、`linux-amd64.deb` |

文件名均以 `NekoCrate-Launcher-dev0.1-` 开头。`SHA256SUMS` 提供文件校验值，`BUILDINFO.json` 记录源代码提交、版本、目标架构及每个安装包的校验值。

## 本版功能

- 一个实例对应一个独立 `.minecraft` 目录，可安装和切换多个原版游戏版本；支持已有目录导入、内存与 Java 配置、图标选择和改名。
- 游戏版本安装、并发下载、重试、取消、完整性校验与修复，以及官方和 BMCLAPI 来源切换。
- 按版本匹配 Java、自动下载运行时，管理启动进程、停止游戏和脱敏日志。
- 可在未登录状态以 Player 本地身份启动；支持微软设备码登录流程、多账户选择、刷新和皮肤读取。
- 五套 HUD / pixel glass 主题与连续动态背景，可拖拽旋转的 3D 玩家皮肤，居中的设置弹窗和版本菜单。

## 安装与已知限制

- 这是开发预览版，尚未完成三个平台的完整实机交互和真实游戏进入验收；构建、自动化测试通过不代表全部功能已在每个平台验收。
- macOS 要求 11.0 或更新版本。此包使用 ad-hoc 签名，未做 Apple 公证；系统可能要求在「隐私与安全性」中确认打开。Windows 安装程序尚无发布者代码签名。
- Windows 使用 WebView2，安装时可能需要联网安装该运行时。Linux 包在 Ubuntu 22.04 构建；Debian 包由系统安装依赖，AppImage 需要执行权限，部分发行版还需要 FUSE 2。
- 此版本未内置项目自己的 Microsoft Client ID。首次添加微软账户需在添加账户流程中配置有效的应用编号；微软授权和 Minecraft API 访问许可仍需由项目开发者完成。未登录启动不需要此配置。配置说明见仓库的 `docs/authentication.md`。
- 当前支持官方原版 Java Edition 1.13+ 的元数据协议；Apple Silicon 使用具备 ARM 原生库的 1.19+。各游戏版本未逐一验证，暂不支持模组加载器和整合包安装。
- 默认 Steve 纹理归 Mojang Studios / Microsoft 所有，本项目不是 Minecraft 官方产品。
