import { ref, computed, watchEffect } from "vue";

const messages = {
  zh: {
    meta: {
      download: "下载 · 猫咪盒子启动器 NekoCrate Launcher",
      features: "功能与设计 · 猫咪盒子启动器",
      docs: "使用文档 · 猫咪盒子启动器",
    },
    nav: { download: "下载", features: "功能", docs: "文档", main: "主导航" },
    skip: "跳到内容",
    home: "猫咪盒子启动器首页",
    repo: "GitHub 仓库",
    language: "Switch to English",
    languageShort: "EN",
    themes: "主题配色",
    themeSuffix: "主题",
    feedback: "反馈问题",
    trademark: "独立开源项目，与 Mojang 或 Microsoft 无隶属关系。",
    hero: {
      title: ["猫咪盒子", "启动器"],
      lede: "Minecraft Java 版启动器，用 Rust 构建。游戏版本、加载器、整合包、Java 和资源，都在一个窗口里安排好。",
      noMatch: "启动器运行在 Windows、macOS 和 Linux 电脑上。",
    },
    dl: {
      download: (system) => `下载 ${system} 版`,
      options: "更改系统、运行方式或版本",
      system: "系统",
      mode: "运行方式",
      version: "版本",
      installed: "安装版",
      portable: "便携版",
      latest: "最新",
      preview: "开发版",
      stable: "正式版",
      missing: "该版本未提供此构建",
      viewRelease: "查看发布页",
      choose: "如何选择",
      history: "全部历史版本",
      notes: {
        windows: {
          installed: "安装程序，设置保存在系统目录。",
          portable: "免安装，程序旁的 .minecraft 即游戏目录。",
        },
        macos: {
          installed: "打开 DMG，拖入「应用程序」。",
          portable: "解压即用，数据保存在应用旁。",
        },
        linux: {
          installed: "适用于 Debian / Ubuntu。",
          portable: "赋予执行权限后直接运行。",
        },
      },
      source: "所有文件来自项目的 GitHub Releases。",
    },
    next: {
      label: "继续了解",
      features: ["先看看它能做什么", "拖放资源、整理版本，定制你的启动界面。"],
      docs: ["从第一次启动开始", "系统要求、安装、关联目录、登录和常见问题。"],
    },
    docs: {
      title: "使用文档",
      search: "搜索文档",
      toc: "文档目录",
      none: "没有找到相关文档。",
      source: "仓库技术文档",
      root: "文档",
      copy: "复制命令",
      copied: "已复制",
      copyFailed: "未能复制，请选中命令手动复制。",
      siblings: "相邻文档",
      prev: "上一篇",
      next: "下一篇",
    },
  },
  en: {
    meta: {
      download: "Download · NekoCrate Launcher",
      features: "Features · NekoCrate Launcher",
      docs: "Docs · NekoCrate Launcher",
    },
    nav: { download: "Download", features: "Features", docs: "Docs", main: "Main" },
    skip: "Skip to content",
    home: "NekoCrate Launcher home",
    repo: "GitHub repository",
    language: "切换到中文",
    languageShort: "中",
    themes: "Colour theme",
    themeSuffix: " theme",
    feedback: "Report an issue",
    trademark: "An independent open-source project, not affiliated with Mojang or Microsoft.",
    hero: {
      title: ["NekoCrate", "Launcher"],
      lede: "A Minecraft: Java Edition launcher built in Rust. Game versions, loaders, modpacks, Java and resources, all arranged in one window.",
      noMatch: "The launcher runs on Windows, macOS and Linux computers.",
    },
    dl: {
      download: (system) => `Download for ${system}`,
      options: "Change system, install type or version",
      system: "System",
      mode: "Install type",
      version: "Version",
      installed: "Installer",
      portable: "Portable",
      latest: "Latest",
      preview: "Preview",
      stable: "Stable",
      missing: "This version has no build for that choice",
      viewRelease: "Open release page",
      choose: "Which one?",
      history: "All past releases",
      notes: {
        windows: {
          installed: "Setup program; settings live in your user folder.",
          portable: "No install; the .minecraft next to it is your game folder.",
        },
        macos: {
          installed: "Open the DMG and drag the app into Applications.",
          portable: "Unpack and run; data stays next to the app.",
        },
        linux: {
          installed: "For Debian and Ubuntu.",
          portable: "Mark it executable, then run it.",
        },
      },
      source: "Every file comes from the project's GitHub Releases.",
    },
    next: {
      label: "Learn more",
      features: ["See what it can do", "Drag in resources, organise versions, arrange your home screen."],
      docs: ["Start with the first launch", "Requirements, install, linking folders, sign-in and common problems."],
    },
    docs: {
      title: "Documentation",
      search: "Search docs",
      toc: "Contents",
      none: "No matching articles.",
      source: "Technical docs in the repo",
      root: "Docs",
      copy: "Copy command",
      copied: "Copied",
      copyFailed: "Couldn't copy. Select the command and copy it by hand.",
      siblings: "More articles",
      prev: "Previous",
      next: "Next",
    },
  },
};

const storageKey = "nekocrate-site-lang";
function initial() {
  try {
    const saved = localStorage.getItem(storageKey);
    if (saved in messages) return saved;
  } catch {
    /* optional */
  }
  return /^zh\b/i.test(navigator.language || "zh") ? "zh" : "en";
}

export const lang = ref(initial());
export const t = computed(() => messages[lang.value]);

export function toggleLang() {
  lang.value = lang.value === "zh" ? "en" : "zh";
  try {
    localStorage.setItem(storageKey, lang.value);
  } catch {
    /* optional */
  }
}

watchEffect(() => {
  document.documentElement.lang = lang.value === "zh" ? "zh-CN" : "en";
});
