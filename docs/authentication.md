# Microsoft 登录配置

NekoCrate 使用公共客户端的 OAuth 设备授权流程。Client ID 是公开应用标识；不要在启动器中填写或嵌入 Client Secret，也不要复用其他启动器的 ID。

## 猫咪盒子应用注册信息

以下信息由项目维护者于 2026-10-25 提供。

| 标识 | 值 | 用途 |
| --- | --- | --- |
| 应用程序（客户端）ID | `31dbb1aa-d741-4983-bae9-c1bc13dc71b2` | 启动器 Microsoft OAuth 的 Client ID，以及 Minecraft AppID 审核申请 |
| 对象 ID | `3e0f3fe3-fac4-4ca8-87b5-30eac6132c27` | Entra 应用注册对象的管理标识 |
| 目录（租户）ID | `4c41e974-f6cb-41ee-81ec-a9bda6658a7d` | Minecraft AppID 审核申请中的 Tenant ID |

Minecraft API 审核状态：项目维护者于 2026-10-27 确认已通过 Mojang 审核。登录端点继续使用 `consumers`，而非目录（租户）ID。

## 账户皮肤切换

资源管理的皮肤卡片可直接应用到当前选中的微软账户。Rust 使用有效的 Minecraft 会话向 `https://api.minecraftservices.com/minecraft/profile/skins` 发送 multipart PNG 与 `classic`／`slim` 模型，前端不接收访问令牌。首次使用或令牌到期按既有流程刷新，上传返回 HTTP 401 时最多刷新并重试一次；其他错误不自动重复上传。官方上传限制为 64×64／64×32 PNG，本地高分辨率预览不受此限制。

上传固定绑定请求的账户，验证返回的档案 UUID 与官方纹理地址后更新本地档案和会话缓存。账户写入、会话刷新与移除使用独立的账户锁，不等待游戏下载／安装；浏览器授权等待阶段不占用该锁。请求禁止重定向，保留 TLS 校验，最多等待 30 秒，不记录令牌或服务响应正文。仅点击皮肤卡片会改变云端皮肤，导入、浏览、旋转、导出与编辑本地模型不会上传。协议测试使用本地模拟服务，真实账户上传需要用户在界面实际验证。

## Java 版玩家名称

账户弹窗中的微软账户名称在悬停时显示，单击后原位编辑，不切换当前启动账户。回车或输入框右侧确认图标提交；未提交时失焦或 Esc 放弃。编辑提示仅为“30天可修改一次”。失败后保留输入内容与右上角错误提示，重新进入编辑、重试期间不清空错误；成功或手动关闭错误提示才清除。此操作修改 Java 版云端玩家名称，不修改 Microsoft 显示名或 Xbox Gamertag。

`rename_account` 将点击时的账户 ID 固定传给 Rust，复用账户会话。先校验 3–16 位 ASCII 字母、数字、下划线，再查询官方 `GET /minecraft/profile/namechange` 和 `GET /minecraft/profile/name/{name}/available`，允许后提交 `PUT /minecraft/profile/name/{name}`。冷却资格以服务返回为准，不根据本机日期推算；仅改变大小写时跳过自身名称的可用性查询，仍由服务执行冷却规则。相同名称不发送请求。返回档案须匹配账户 UUID 与目标名称，然后同步数据库、内存启动会话和前端名称，保留皮肤、游客和当前选中账户。

改名与皮肤写入在前端按序提交，并与后端账户锁配合，避免旧档案响应互相覆盖。网络请求禁止重定向、保留 TLS 校验；仅明确的 HTTP 401 会刷新并重试一次，超时或其他不确定结果不自动重发改名。失败原因写入启动器诊断日志，不包含访问令牌或原始服务正文。测试使用本地协议模拟服务，不会实际修改用户的云端名称。参考：[Minecraft 官方改名说明](https://help.minecraft.net/hc/en-us/articles/4408950195341)、[MojangAPI 改名接口实现](https://github.com/CmlLib/MojangAPI/blob/master/MojangAPI/Mojang.cs)。

## 配置与登录流程

1. 在 Microsoft Entra 的应用注册中创建你自己的应用。
2. 账户类型需要允许个人 Microsoft 账户（Xbox/Minecraft 使用的消费者账户）。
3. 在 Authentication 中允许公共客户端流程（Allow public client flows）。设备流程无需启动器接收重定向回调。
4. 点击左上角账户卡片，再点击头像列表中的「+」，直接开始微软登录。启动器内置上表中的项目 Client ID，账户界面不再要求用户配置应用 ID。开发者仍可通过数据文件中的 `settings.microsoft_client_id` 覆盖默认值；空值（包括旧版本留下的空配置）自动使用内置 ID。
5. 申请/确认你的应用具备 Minecraft API 的访问许可。微软 OAuth 成功不代表 Minecraft 会接受该应用；若 Minecraft 登录阶段返回应用访问错误，需先处理注册/许可问题。
6. 点击添加账户后，启动器显示设备码并自动复制到剪贴板，在系统默认浏览器打开微软返回的授权地址。在网页中输入设备码并授权，启动器自动完成登录。打开浏览器失败不会终止授权，可点击验证码下方的登录地址重试。启动器只接收令牌，不接收账户密码。

请求权限：`XboxLive.signin offline_access`。

流程：

```text
Microsoft device code → Microsoft access/refresh token
→ Xbox Live user token → XSTS token
→ Minecraft access token → entitlements → player profile
```

刷新令牌按数据目录命名空间和玩家 UUID 存储到系统钥匙串，访问令牌只在内存与游戏进程参数中短暂使用。前端、数据库、启动器日志不输出令牌。Java 进程接收访问令牌属于游戏启动协议的一部分。

已选择微软账户时，登录或首次刷新得到的会话按账户和 Client ID 缓存在当前进程中。游戏启动直接复用有效会话；在有效期结束前 60 秒才重新刷新授权、检查权益。有效期同时检查单调时钟和系统时间，避免时间回拨或系统睡眠延长令牌有效期。刷新令牌首次读取后同样保留在内存中，只有轮换时才写回钥匙串；手动「刷新账户」仍强制刷新。macOS 保存凭据时只查询条目引用再更新，不额外读取旧凭据，不改变现有条目的访问控制。移除账户会清除内存中的凭据，退出后缓存自然释放。刷新失败保留错误，不自动改用其他玩家身份。Linux 的微软账户需要解锁的 Secret Service（例如 GNOME Keyring/KWallet），钥匙串不可用时会显示错误，不回退到明文文件。

macOS 的钥匙串授权还取决于程序的代码签名身份。`cargo ui` 使用固定的本地自签证书，在首次启动和每次 Rust 重载前签名；指定要求绑定 `dev.nekocrate.launcher` 和证书指纹，不再绑定随编译变化的二进制哈希。私钥保存在当前用户钥匙串，仅允许系统 `codesign` 使用；公证书与身份配置保存在 `~/Library/Application Support/NekoCrate/signing/`。缺失原身份时停止启动，不静默生成新证书。

从旧的临时签名迁移后，已有账户的凭据可能仍需授权一次；在系统提示中选择「始终允许」才会持久记住授权。钥匙串锁定、系统策略或证书被替换仍可能触发提示。此实现不放开凭据条目的访问控制，也不读取用户的钥匙串密码。GitHub Release 需要另外配置持久的发布签名身份，本机证书不会自动用于远程构建。参见 [Apple TN2206：代码签名与钥匙串](https://developer.apple.com/library/archive/technotes/tn2206/_index.html)。

Windows 使用 Windows Credential Manager 存储刷新令牌，Linux 使用 Secret Service；更新或重新编译不需要照搬 macOS 的应用签名授权流程。用户凭据不会进入构建环境，发布签名私钥也不会装入用户凭据库或分发到客户端，具体见[签名与密钥管理](build-infrastructure.md#签名与密钥管理)。

账户列表始终保留游客入口；选择它会清空 `selected_account`，保留所有微软账户与凭据。游客名称默认 `Player`，可点击游客头像下悬停显示的名称原位修改（1–16 位英文字母、数字或下划线，回车或离开输入框保存，Esc 放弃），单独持久化在 `guest_name`。旧数据库没有这个字段时自动使用默认名称。本地 UUID 继续使用 `a01e3843-e521-3998-958a-f459800e4d11`（原 `OfflinePlayer:Player` 的 Java name-based UUID），改名不改变本地玩家数据的归属。这个身份只用于游戏进程，不创建账户记录，不需要 Client ID，也不访问微软身份服务或钥匙串。无认证的进程参数使用 `accessToken=0`，此占位值与 [Prism 的离线会话实现](https://github.com/PrismLauncher/PrismLauncher/blob/develop/launcher/minecraft/auth/AuthSession.cpp) 一致；它不是凭据，不参与日志令牌替换。需要微软身份验证的在线服务仍需登录账户。

已区分的错误包括：用户拒绝授权、设备码过期、刷新令牌被撤销、Client ID/权限配置错误、Xbox 档案缺失、家庭/年龄限制、游戏权益缺失与 Java 档案未创建。

文档参考：

- [Microsoft 设备授权流程](https://learn.microsoft.com/entra/identity-platform/v2-oauth2-device-code)
- [Microsoft 身份验证流程](https://learn.microsoft.com/entra/identity-platform/msal-authentication-flows)
- [Prism 应用密钥政策](https://github.com/PrismLauncher/PrismLauncher#forkingredistributingcustom-builds-policy)

本原型的真实登录验收需要项目自己的 Client ID 和用户亲自完成微软授权。代码编译、错误分类测试或设备码生成均不等于完整登录验收。
