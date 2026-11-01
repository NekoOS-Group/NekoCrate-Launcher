# dev0.9.5

便携模式内置于程序，移动或改名后，下次启动直接管理程序所在目录的 `.minecraft`，不再需要 `nekocrate.portable.json` 或预先创建数据文件夹。

## 下载与使用

| 平台 | 便携版 | 使用方式 |
| --- | --- | --- |
| macOS Apple Silicon | `mac-appsilicon-portable.app.tar.gz` | 解压得到 `NekoCrate Portable.app`，放到游戏目录后双击 |
| Windows x64 | `win-amd64-portable.exe` | 单个 EXE，放到游戏目录后双击 |
| Linux x64 | `linux-amd64.elf` | 单个 ELF，赋予执行权限后运行 |

- `.minecraft` 不存在时自动创建；设置、Java、索引和任务按需保存在旁边的 `.nekocrate`。
- macOS 的游戏目录位于 `.app` 旁边，不在应用包内部。若 macOS 临时隔离应用，启动器会提示用 Finder 移动 `.app` 后再打开。
- 只移动程序会使用新位置的数据；需要一起搬迁游戏、设置和任务时，关闭启动器后移动整个文件夹。不会自动搬动旧目录的文件。
- 安装版与便携版数据独立；仍保留 DMG、Windows 安装程序和 Linux DEB。
- Windows 仍需 WebView2。Linux ELF 依赖系统 GTK 3、WebKitGTK 4.1 等运行库，基于 Ubuntu 22.04 构建；缺少依赖时可以使用 DEB 让包管理器安装依赖。不再构建或发布 AppImage，也不再需要 FUSE。
- 账户凭据仍保存在操作系统凭据库，换电脑后需要重新登录。

## 自动更新

- 新便携版使用各自的更新目标，更新后继续保持便携模式。
- Windows 下载并验签 EXE 后，通过后台辅助进程替换并重新启动；Linux ELF 通过更新器验签、替换并重启；macOS 使用同一个 `.app.tar.gz` 完成手动分发和自动更新。
- 保留签名及版本绑定验证；不发布独立 `.sig`，校验清单仍统一存放在 `BUILDINFO.json`。
- **dev0.9.4 的旧便携版请手动换用本次便携程序一次**，放回原来的游戏目录即可继续使用 `.minecraft` 和 `.nekocrate`。旧标记文件可删除。旧 AppImage 也需手动换用 ELF；旧更新目标不会被指向不兼容的新格式。安装版继续使用原更新通道。

## 构建与验证

Core、前端与依赖构建结果共用缓存，只为便携入口增加桌面构建与链接。发布会检查安装版、便携版内嵌前端，执行无标记启动、程序改名、整体搬迁和单独移动程序测试；Linux 测试会实际替换并运行签名 ELF，Windows 保留原生文件替换及失败回滚检查。
