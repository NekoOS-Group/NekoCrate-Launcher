# 原型验证记录

日期：2026-10-21。平台：macOS / Apple Silicon；Rust 1.93.1，Node.js 22.14。

## 已验证

- `cargo test --workspace`：13 项核心测试通过。覆盖模拟的 Xbox → XSTS → Minecraft 授权链、无权益拒绝、设备码隔离、下载重试、校验失败拒收、流式取消与临时文件清理、缓存修复、路径约束、实例复制隔离与回收、迁移排除凭据、日志脱敏与超长行处理。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo fmt --all -- --check`：通过。
- `npm run build`：TypeScript 检查与 Vite 生产构建通过。
- `npm run tauri build -- --debug`：成功生成 macOS ARM 桌面应用，约 49 MiB。
- 实际打开 `.app`，确认 Tauri 本地资源加载、中文首页与新建实例对话框正常显示。图形界面的完整操作回归尚未完成；实例生命周期由核心集成测试验证。
- 官方 Minecraft 1.21.1：3,953 个去重游戏文件下载与完整性校验完成。首次测试触发真实网络重试。

## 实际网络检查

使用 `python3 scripts/smoke_install.py --version 1.21.1`。测试数据位于 `.runtime/smoke-1.21.1`，不会提交到 Git。

自动 Java 安装及主动损坏资源后的修复验收仍在进行。首次 Adoptium 元数据请求失败，后续 Java 压缩包请求出现重试。客户端已启用 HTTP/2，元数据请求设为 30 秒超时，并保留详细错误原因。独立诊断中 Mojang 与 Adoptium 元数据端点均返回 HTTP 200；这不能代替完整 Java 下载验收。

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
