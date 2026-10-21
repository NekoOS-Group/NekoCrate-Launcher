# 原型验证记录

日期：2026-10-21。平台：macOS / Apple Silicon；Rust 1.93.1，Node.js 22.14。

## 已验证

- `cargo test --workspace`：13 项核心测试通过。覆盖模拟的 Xbox → XSTS → Minecraft 授权链、无权益拒绝、设备码隔离、下载重试、校验失败拒收、流式取消与临时文件清理、缓存修复、路径约束、实例复制隔离与回收、迁移排除凭据、日志脱敏与超长行处理。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo fmt --all -- --check`：通过。
- `npm run build`：TypeScript 检查与 Vite 生产构建通过。
- `npm run tauri build -- --debug`：成功生成 macOS ARM 桌面应用，约 51 MiB。
- 实际打开 `.app`，确认 Tauri 本地资源加载、中文首页与新建实例对话框正常显示。图形界面的完整操作回归尚未完成；实例生命周期由核心集成测试验证。
- 官方 Minecraft 1.21.1：3,953 个去重游戏文件下载与完整性校验完成。首次测试触发真实网络重试。

## 实际网络检查

使用 `python3 scripts/smoke_install.py --version 1.21.1`。测试数据位于 `.runtime/smoke-1.21.1`，不会提交到 Git。

真实安装脚本最终返回 **PASS**：官方游戏安装、Adoptium Java 21 自动下载/校验/解压/版本检测，以及人为损坏资源文件后的缓存修复均通过。Java 为 Temurin 21.0.12.1（macOS aarch64）。

测试过程中出现过 Adoptium 元数据与 Java 下载重试。客户端已启用 HTTP/2，元数据请求设为 30 秒超时；大文件按 45 秒读取停滞超时处理，避免正常慢速传输被固定总时长截断。下载错误保留底层原因。

修复脚本核对了资源 SHA-1 恢复为官方索引值，实例最终标记为已安装。完整游戏启动仍需真实账户授权。

网络诊断（不发送账户信息）：

```sh
cargo run -p nekocrate-core --example check_network
```

## 仍需外部条件的验收

- 真实微软登录、系统钥匙串写入/读取、刷新真实凭据、进入游戏：需要项目自己的 Microsoft Client ID、Minecraft API 许可及用户亲自完成账户授权。模拟协议测试不等于真实登录验收。
- Windows/Linux：已配置构建、测试和静态检查工作流，不能据此宣称已在这两个平台实机运行。
- 国内网络：提供 BMCLAPI 切换及回退机制，尚未完成多地区、不同运营商的性能验证。
- 版本覆盖：当前实际安装目标为 1.21.1，1.13+ 的协议支持未逐版本验收；Apple Silicon 限带 ARM 原生库的版本。

应用为开发原型，尚未做发行签名、公证或自动更新。

## 界面调整（2026-10-21）

- 使用 `#CCDBDC`、`#9AD1D4`、`#80CED7`、`#007EA7`、`#003249` 作为主题色。移除宣传文案、首页横幅、插画和统计卡片。
- 参考 [HMCL 官方界面说明](https://docs.hmcl.net/launcher/gui-features.html) 的账户导航及底部启动区域，改为侧边导航、实例列表、实例配置与固定启动栏。默认窗口为 1140 × 720，最小 900 × 600。
- 在真实 macOS 应用中检查布局，验证切换已安装/待安装实例时，配置区域、底部实例选择和启动/安装按钮同步更新。
- TypeScript/Vite 生产构建与 macOS 应用打包通过。表单完整自动点测未完成：后续桌面操作工具未获应用控制许可。未将其记为通过。
