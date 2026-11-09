export const articles = [
  {
    id: "start",
    title: "第一次启动",
    group: "开始使用",
    summary: "从下载到启动你的第一个游戏版本。",
    sections: [
      {
        title: "1. 下载并打开",
        paragraphs: [
          "在下载页选择操作系统与运行方式。Windows / Linux 提供 x64 构建，macOS 提供 Apple Silicon 构建；macOS Intel 用户暂时没有对应的预编译包。",
        ],
        link: { label: "选择下载", href: "" },
      },
      {
        title: "2. 添加游戏目录",
        paragraphs: [
          "点击实例栏的加号，可以关联已有的 .minecraft，也可以新建游戏目录。官方启动器的目录会自动显示在可选列表中。",
          "已有游戏文件无需搬家。把包含 .minecraft 的文件夹拖入窗口，启动器会扫描并让你选择要关联的目录。",
        ],
      },
      {
        title: "3. 选择账户",
        paragraphs: [
          "点击左上角账户入口，添加微软账户并在浏览器中完成授权。登录成功后返回启动器选择账户。",
          "也可以选择游客身份游玩本地内容；需要正版验证的服务器与在线服务仍要求有效的游戏账户和权益。",
        ],
      },
      {
        title: "4. 添加版本并启动",
        paragraphs: [
          "在启动按钮旁的版本菜单选择「添加新的版本」，选择 Minecraft 版本和所需加载器；也可以切换到整合包浏览。",
          "下载完成后点击「启动游戏」。启动器会准备兼容 Java、依赖和资源，游戏运行时可查看日志或停止游戏。",
        ],
      },
    ],
  },
  {
    id: "install",
    title: "系统要求与安装",
    group: "开始使用",
    summary: "三个平台的运行条件与安装方式。",
    sections: [
      {
        title: "Windows x64",
        paragraphs: [
          "安装版运行 setup.exe，按安装向导完成即可。便携版直接运行 EXE。启动器界面需要 Microsoft Edge WebView2 运行时。",
          "当前发布包尚无 Windows 发布者代码签名。请核对下载来自 NekoOS-Group/NekoCrate-Launcher 的 Releases 页面。",
        ],
      },
      {
        title: "macOS Apple Silicon",
        paragraphs: [
          "支持 Apple Silicon Mac，应用最低系统版本为 macOS 11。安装版打开 DMG，将应用拖入「应用程序」；便携版解压 .app.tar.gz 后双击应用。",
          "当前开发版未进行 Apple 公证。如果系统阻止打开，请在确认来源后，通过「系统设置 → 隐私与安全性」查看该应用的打开选项。不要全局关闭系统安全检查。",
        ],
      },
      {
        title: "Linux x64",
        paragraphs: [
          "Debian / Ubuntu 可使用 DEB 包安装。其他兼容发行版可以运行 ELF，但仍需系统提供 GTK 3、WebKitGTK 4.1 和 D-Bus 等运行库。",
          "下载 ELF 后先赋予文件执行权限；可在文件管理器的属性中修改，也可在终端执行以下命令，将文件名替换为实际下载名称。",
        ],
        code: "chmod +x NekoCrate-Launcher-版本-linux-amd64.elf\n./NekoCrate-Launcher-版本-linux-amd64.elf",
      },
      {
        title: "游戏需要的 Java",
        paragraphs: [
          "启动器本身不依赖 Java，Minecraft Java 版需要。猫咪盒子会发现本机运行时并检查兼容性，缺少时下载合适版本。游戏与整合包还各自有内存、显卡和系统要求。",
        ],
      },
    ],
  },
  {
    id: "portable",
    title: "安装版与便携版",
    group: "开始使用",
    summary: "选择启动器数据的位置，了解迁移方式。",
    sections: [
      {
        title: "安装版：放进系统",
        paragraphs: [
          "安装版使用系统应用数据目录保存启动器设置、索引和任务。游戏目录可以放在任意合适的位置，通过启动器关联。适合长期在同一台电脑使用。",
        ],
      },
      {
        title: "便携版：跟着文件夹走",
        paragraphs: [
          "便携程序从所在目录寻找 .minecraft，不存在时创建；启动器数据保存在旁边的 .nekocrate。macOS 的游戏目录和数据位于 .app 外部，与 .app 并列。",
        ],
        code: "我的游戏/\n├── 猫咪盒子启动器.app  （或 .exe / .elf）\n├── .minecraft/       游戏、版本与存档\n└── .nekocrate/       设置、Java、索引与任务",
      },
      {
        title: "迁移与并存",
        paragraphs: [
          "关闭启动器和游戏后，移动整个便携目录可以保留游戏与设置。只移动可执行程序，会在新的位置使用新的数据。",
          "安装版与便携版的数据各自独立。同一数据目录不能同时由两个启动器进程使用；账户凭据仍保存在系统凭据库，换电脑需要重新登录。",
        ],
      },
      {
        title: "更新",
        paragraphs: [
          "macOS .app、Windows 安装版与便携版、Linux ELF 支持应用内更新。更新包经过签名验证后安装并重启；Linux DEB 通过下载新的安装包更新。",
        ],
      },
    ],
  },
  {
    id: "directories",
    title: "游戏目录与版本",
    group: "管理游戏",
    summary: "实例就是一个 .minecraft，一个实例可以有多个版本。",
    sections: [
      {
        title: "关联已有目录",
        paragraphs: [
          "选择或拖入现有游戏目录后，猫咪盒子直接在原位置识别、读取和管理游戏文件，不会复制整个目录到启动器数据目录。也可以拖入包含多个 .minecraft 的上层目录进行扫描。",
        ],
      },
      {
        title: "版本隔离",
        paragraphs: [
          "版本管理负责一个游戏目录中的各个启动版本。实例设置中可选择版本隔离方式；新安装的加载器版本和整合包使用独立工作目录，避免 Mods、配置与存档混用。",
          "已有整合包尽量沿用原工作目录，不要为了整理名字手动移动加载器配置或依赖。版本显示名可以直接在启动器中修改，显示名与磁盘目录相互独立。",
        ],
      },
      {
        title: "排序与管理",
        paragraphs: [
          "拖动实例或版本条目可以调整排列顺序。版本管理中可以浏览元数据、Mods、数据包、资源包、运行依赖与存档。实例设置提供 Java、内存、游戏窗口和启动选项。",
        ],
      },
      {
        title: "移除时会发生什么",
        paragraphs: [
          "移除关联的游戏目录只会取消关联，原文件保留。移除由启动器管理的实例或单个版本会使用系统废纸篓／回收站，并检查其他版本的依赖。共享文件不会随单个版本一起删除。",
        ],
      },
    ],
  },
  {
    id: "resources",
    title: "整合包与资源",
    group: "管理游戏",
    summary: "在线浏览、本地导入，以及版本之间的资源复用。",
    sections: [
      {
        title: "安装整合包",
        paragraphs: [
          "在「添加新的版本」中切换到整合包，浏览 Modrinth 或 CurseForge。进入详情后选择版本，检查目标游戏目录，再下载。也支持导入本地 .mrpack 和 CurseForge 格式的整合包。",
          "整合包会按清单安装。部分作者不允许第三方直接分发文件，这些项目会出现在任务窗口的手动导入步骤；从对应项目页下载后，将文件拖入提示区域即可继续。",
        ],
      },
      {
        title: "拖入与复制资源",
        paragraphs: [
          "将 Mods、资源包、光影或地图拖到目标版本的对应区域。ZIP 文件会根据实际内容识别，不会因为拖入位置不同就改变资源类型。数据包可以放入指定存档。",
          "现有资源可以拖到其他版本复制，源文件保留。全局资源库还支持收藏、外部导入和在线下载，再将资源分配给不同版本。",
        ],
      },
      {
        title: "启用、禁用和配置",
        paragraphs: [
          "在 Mod 列表点击状态图标切换启用状态。展开条目可查看元数据、依赖和已关联的在线介绍；有配置文件的 Mod 可打开配置编辑器。",
          "JSON、TOML 和 CFG 提供相应编辑辅助，保存时会进行校验与备份。删除整合包清单声明的 Mod 前会再次确认。",
        ],
      },
      {
        title: "加载器兼容性",
        paragraphs: [
          "支持安装 Fabric、Forge 和 NeoForge。Mod 必须匹配游戏版本和加载器；同为 JAR 文件并不意味着可以混用。Quilt 安装与其他游戏版本类型目前不在支持范围内。",
        ],
      },
    ],
  },
  {
    id: "java",
    title: "Java 与内存",
    group: "管理游戏",
    summary: "优先复用兼容的 Java，按实例调整启动环境。",
    sections: [
      {
        title: "自动选择 Java",
        paragraphs: [
          "启动器会搜索本机 Java，并综合游戏、加载器和 Mod 声明的约束选择兼容版本。已有兼容运行时会优先复用；缺少时通过 Adoptium / GitHub 下载。",
          "Java 越新并不总是越兼容。旧 Forge、特定原生库和部分 Mod 可能限制可用范围。Apple Silicon 上的旧游戏还可能需要 Intel Java 及系统的 Intel 程序运行支持。",
        ],
      },
      {
        title: "指定本机运行时",
        paragraphs: [
          "实例设置的 Java 列表可选择自动、已探测的运行时，或通过文件选择器指定可执行文件。全局资源管理的 Java 页可查看发行版、架构和托管标识。",
          "启动器托管的 Java 可以删除；系统安装的 Java 由系统或原安装工具管理。",
        ],
      },
      {
        title: "分配内存",
        paragraphs: [
          "提供「最小」「推荐」「自动」与「自定义」。自动模式会结合游戏和整合包规模及可用内存估算；自定义值以 MB 为单位。",
          "为系统与其他程序保留足够内存。若游戏崩溃，请结合日志判断是内存不足、Java 不兼容还是其他问题。",
        ],
      },
    ],
  },
  {
    id: "downloads",
    title: "下载、修复与日志",
    group: "问题排查",
    summary: "了解任务进度，保留完成量，定位失败原因。",
    sections: [
      {
        title: "任务中的几个阶段",
        paragraphs: [
          "下载表示从网络传输，复用表示使用本地已有文件，校验检查文件完整性，安装处理负责解压、运行加载器安装步骤等操作。任务窗口显示当前流程与文件进度。",
          "下载支持暂停、续传和失败文件重试。恢复时保留已完成的工作，服务器不支持 Range 时对应文件可能需要重新下载。安装成功后会清理下载暂存。",
        ],
      },
      {
        title: "网络设置",
        paragraphs: [
          "游戏本体与 Mod 可分别选择下载源，支持自动选择及系统 / HTTP / SOCKS 代理。并发数最高可设为 256，也可使用自适应模式；更高并发并不一定更快。",
          "如果多个 HTTPS 下载同时失败，请先核对系统日期、网络与代理配置，再查看任务中的具体错误。",
        ],
      },
      {
        title: "校验修复",
        paragraphs: [
          "安装不完整或文件损坏时，可以执行校验修复。主动修复会检查所需文件，并优先使用本地可复用资源，必要时重新下载。",
          "修复流程不会把所有游戏崩溃都当成文件损坏；缺少前置、版本冲突和配置错误需要根据诊断结果处理。",
        ],
      },
      {
        title: "反馈启动问题",
        paragraphs: [
          "异常退出时打开日志，查看高亮错误与诊断信息。反馈时附上启动器版本、操作系统、游戏版本、加载器及错误前后的日志；不要附上账户凭据或访问令牌。",
        ],
        external: {
          label: "到 GitHub 反馈问题",
          href: "https://github.com/NekoOS-Group/NekoCrate-Launcher/issues",
        },
      },
    ],
  },
  {
    id: "development",
    title: "开发与参与",
    group: "开发者",
    summary: "本地运行、仓库结构与开源许可。",
    sections: [
      {
        title: "本地开发",
        paragraphs: [
          "需要 Rust 1.93 或更新的 stable、Node.js 22，以及 Tauri 对应平台的系统依赖。在仓库根目录运行以下命令；macOS 的本地签名还需要 Python 3。",
        ],
        code: "git switch neko_dev\nnpm --prefix app ci\ncargo ui",
      },
      {
        title: "核心与界面",
        paragraphs: [
          "Rust 核心位于 crates/nekocrate-core，Vue 前端位于 app/src，Tauri IPC 与打包在 app/src-tauri。桌面应用与 CLI 共享核心逻辑；网站独立放在 site，直接复用启动器的背景渲染与主题。",
        ],
        external: {
          label: "阅读架构说明",
          href: "https://github.com/NekoOS-Group/NekoCrate-Launcher/blob/neko_dev/docs/architecture.md",
        },
      },
      {
        title: "参与项目",
        paragraphs: [
          "欢迎反馈可复现的问题、提交改进或完善文档。代码采用 GPL-3.0-only；第三方依赖和 Minecraft 素材保留各自许可与权利声明。",
        ],
        external: {
          label: "浏览源码与贡献",
          href: "https://github.com/NekoOS-Group/NekoCrate-Launcher",
        },
      },
    ],
  },
];
