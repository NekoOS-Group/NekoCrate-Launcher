猫咪盒子启动器 dev0.3.1，应用版本 `0.3.1-dev.1`。这是 dev0.3 的修复版。

## 修复

- 修复 Windows 校验缓存的文件替换识别：除了大小和时间戳，还检查卷标识、文件 ID 和变更时间；即使新文件保留原大小和修改时间，也会重新校验。
- Windows 旧校验记录会失效并重新建立；未变化的文件继续复用校验结果，显式「校验修复」仍完整读取文件。文件系统不支持可靠身份信息时，回退为完整校验。
- 修复 Windows 测试设置文件时间戳时的访问权限，并保留 Windows/macOS 的文件系统专项回归测试。
- 更新检查和发布脚本支持 `devX.Y.Z` 修复版标签。

## 发布流程

完整 Rust 测试、Clippy、格式检查、前端测试及构建各集中执行一次；Windows/macOS 只补充文件系统差异测试。发布等待同一提交的 CI 成功后直接复用前端产物，三个打包机只安装锁定版本的 Tauri CLI，各进行一次原生发布编译；保留安装包架构、来源提交和 SHA-256 校验。

## 下载与升级

| 平台 | 安装包 |
| --- | --- |
| macOS Apple Silicon | `mac-appsilicon.dmg`、`mac-appsilicon.app.zip` |
| Windows AMD64 | `win-amd64-setup.exe` |
| Linux AMD64 | `linux-amd64.AppImage`、`linux-amd64.deb` |

文件名前缀为 `NekoCrate-Launcher-dev0.3.1-`。`SHA256SUMS` 提供校验值，`BUILDINFO.json` 记录源代码提交与架构。dev0.3 保留原发布内容。

dev0.3 及更早版本尚不能识别三段式开发标签，请从本页手动下载安装。此修复版安装后可识别后续 `devX.Y` 和 `devX.Y.Z` 版本。微软登录配置、原版游戏支持范围和未公证／未做发布者签名等限制与 [dev0.3](https://github.com/NekoOS-Group/NekoCrate-Launcher/releases/tag/dev0.3) 相同。
