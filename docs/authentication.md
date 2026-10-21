# Microsoft 登录配置

NekoCrate 使用公共客户端的 OAuth 设备授权流程。Client ID 是公开应用标识；不要在启动器中填写或嵌入 Client Secret，也不要复用其他启动器的 ID。

1. 在 Microsoft Entra 的应用注册中创建你自己的应用。
2. 账户类型需要允许个人 Microsoft 账户（Xbox/Minecraft 使用的消费者账户）。
3. 在 Authentication 中允许公共客户端流程（Allow public client flows）。设备流程无需启动器接收重定向回调。
4. 将 Application (client) ID 填入 NekoCrate 设置并保存。
5. 申请/确认你的应用具备 Minecraft API 的访问许可。微软 OAuth 成功不代表 Minecraft 会接受该应用；若 Minecraft 登录阶段返回应用访问错误，需先处理注册/许可问题。
6. 添加账户，在微软网页中输入设备码。启动器只接收令牌，不接收账户密码。

请求权限：`XboxLive.signin offline_access`。

流程：

```text
Microsoft device code → Microsoft access/refresh token
→ Xbox Live user token → XSTS token
→ Minecraft access token → entitlements → player profile
```

刷新令牌按数据目录命名空间和玩家 UUID 存储到系统钥匙串，访问令牌只在内存与游戏进程参数中短暂使用。前端、数据库、启动器日志不输出令牌。Java 进程接收访问令牌属于游戏启动协议的一部分。

启动游戏会刷新授权并重新检查权益；原型不支持离线降级。Linux 需要解锁的 Secret Service（例如 GNOME Keyring/KWallet），钥匙串不可用时会显示错误，不回退到明文文件。

已区分的错误包括：用户拒绝授权、设备码过期、刷新令牌被撤销、Client ID/权限配置错误、Xbox 档案缺失、家庭/年龄限制、游戏权益缺失与 Java 档案未创建。

文档参考：

- [Microsoft 设备授权流程](https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code)
- [Microsoft 身份验证流程](https://learn.microsoft.com/entra/identity-platform/msal-authentication-flows)
- [Prism 应用密钥政策](https://github.com/PrismLauncher/PrismLauncher#forkingredistributingcustom-builds-policy)

本原型的真实登录验收需要项目自己的 Client ID 和用户亲自完成微软授权。代码编译、错误分类测试或设备码生成均不等于完整登录验收。
