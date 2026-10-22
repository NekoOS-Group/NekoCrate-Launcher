# Microsoft 登录配置

NekoCrate 使用公共客户端的 OAuth 设备授权流程。Client ID 是公开应用标识；不要在启动器中填写或嵌入 Client Secret，也不要复用其他启动器的 ID。

1. 在 Microsoft Entra 的应用注册中创建你自己的应用。
2. 账户类型需要允许个人 Microsoft 账户（Xbox/Minecraft 使用的消费者账户）。
3. 在 Authentication 中允许公共客户端流程（Allow public client flows）。设备流程无需启动器接收重定向回调。
4. 点击左上角账户卡片，再点击头像列表中的「+」。尚未配置登录应用时，填入 Application (client) ID 并点击「保存并登录」。这是开发版本的应用配置步骤，全局设置不再包含该字段；正式发行版应由开发者统一提供自己的应用标识。应用配置错误时，可从账户弹窗的错误提示下重新进入配置。
5. 申请/确认你的应用具备 Minecraft API 的访问许可。微软 OAuth 成功不代表 Minecraft 会接受该应用；若 Minecraft 登录阶段返回应用访问错误，需先处理注册/许可问题。
6. 点击「微软登录」后，启动器显示设备码，并自动在系统默认浏览器打开微软返回的授权地址。在网页中输入设备码并授权，启动器自动完成登录。打开浏览器失败不会终止授权，可使用「打开登录页」重试或复制地址。启动器只接收令牌，不接收账户密码。

请求权限：`XboxLive.signin offline_access`。

流程：

```text
Microsoft device code → Microsoft access/refresh token
→ Xbox Live user token → XSTS token
→ Minecraft access token → entitlements → player profile
```

刷新令牌按数据目录命名空间和玩家 UUID 存储到系统钥匙串，访问令牌只在内存与游戏进程参数中短暂使用。前端、数据库、启动器日志不输出令牌。Java 进程接收访问令牌属于游戏启动协议的一部分。

已选择微软账户时，启动游戏会刷新授权并重新检查权益，失败时保留错误，不自动改用其他玩家身份。Linux 的微软账户需要解锁的 Secret Service（例如 GNOME Keyring/KWallet），钥匙串不可用时会显示错误，不回退到明文文件。

未选择账户时可以直接启动，默认玩家名为 `Player`，本地 UUID 固定为 `a01e3843-e521-3998-958a-f459800e4d11`（`OfflinePlayer:Player` 对应的 Java name-based UUID）。这个身份只用于游戏进程，不创建账户记录，不需要 Client ID，也不访问微软身份服务或钥匙串。无认证的进程参数使用 `accessToken=0`，此占位值与 [Prism 的离线会话实现](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/auth/AuthSession.cpp) 一致；它不是凭据，不参与日志令牌替换。需要微软身份验证的在线服务仍需登录账户。

已区分的错误包括：用户拒绝授权、设备码过期、刷新令牌被撤销、Client ID/权限配置错误、Xbox 档案缺失、家庭/年龄限制、游戏权益缺失与 Java 档案未创建。

文档参考：

- [Microsoft 设备授权流程](https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code)
- [Microsoft 身份验证流程](https://learn.microsoft.com/entra/identity-platform/msal-authentication-flows)
- [Prism 应用密钥政策](https://github.com/PrismLauncher/PrismLauncher#forkingredistributingcustom-builds-policy)

本原型的真实登录验收需要项目自己的 Client ID 和用户亲自完成微软授权。代码编译、错误分类测试或设备码生成均不等于完整登录验收。
