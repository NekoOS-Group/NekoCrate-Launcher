<script setup>
import { computed, ref } from "vue";
import {
  ArrowDown,
  ArrowRight,
  ArrowUpRight,
  Box,
  BookOpen,
  ChevronRight,
  Monitor,
  Command,
  Terminal,
  Check,
  PackageOpen,
} from "lucide-vue-next";
import logo from "../icon.webp";
import { useRelease } from "../useRelease";
import {
  releaseAsset,
  detectPlatform,
  formatSize,
  repositoryUrl,
} from "../releases.mjs";

const base = import.meta.env.BASE_URL;
const release = useRelease();
const platform = ref(detectPlatform(navigator.userAgent) || "windows");
const mode = ref("installed");
const platforms = [
  {
    id: "windows",
    label: "Windows",
    arch: "x64",
    icon: Monitor,
    installed: ".exe",
    portable: ".exe",
  },
  {
    id: "macos",
    label: "macOS",
    arch: "Apple Silicon",
    icon: Command,
    installed: ".dmg",
    portable: ".app.tar.gz",
  },
  {
    id: "linux",
    label: "Linux",
    arch: "x64",
    icon: Terminal,
    installed: ".deb",
    portable: ".elf",
  },
];
const current = computed(() =>
  platforms.find((item) => item.id === platform.value),
);
const asset = computed(() =>
  releaseAsset(release.value, platform.value, mode.value),
);
const date = computed(() =>
  new Date(release.value.publishedAt).toLocaleDateString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }),
);
const note = computed(
  () =>
    ({
      windows:
        mode.value === "installed"
          ? "安装到这台电脑，使用系统目录保存启动器设置。"
          : "无需安装。程序旁的 .minecraft 就是你的游戏目录。",
      macos:
        mode.value === "installed"
          ? "打开 DMG，将猫咪盒子启动器拖入「应用程序」。"
          : "解压后双击 .app，游戏目录与设置保存在应用旁边。",
      linux:
        mode.value === "installed"
          ? "适用于 Debian / Ubuntu，通过系统包管理器安装。"
          : "赋予 ELF 执行权限后运行，使用程序旁的游戏目录。",
    })[platform.value],
);
</script>

<template>
  <section class="download-hero">
    <div class="hero-copy">
      <div class="eyebrow">
        <span class="status-dot" /> MINECRAFT JAVA EDITION
      </div>
      <h1>
        猫咪盒子<span>启动器<span class="title-dot">.</span></span>
      </h1>
      <p class="hero-description">
        用 Rust 构建的 Minecraft 启动器。<br />从游戏版本到整合包，把准备工作变简单。
      </p>
      <div class="hero-facts">
        <span><Check :size="14" />开源免费</span
        ><span><Check :size="14" />多版本管理</span
        ><span><Check :size="14" />跨平台</span>
      </div>
    </div>
    <div class="hero-art" aria-hidden="true">
      <div class="artifact-grid"><i /><i /><i /><i /></div>
      <img :src="logo" width="384" height="384" fetchpriority="high" alt="" />
      <span class="artifact-label">NEKOCRATE<span>LAUNCHER</span></span>
      <div class="artifact-coordinates">[ 0, 64, 0 ]</div>
    </div>
  </section>

  <section class="download-panel glass" aria-labelledby="download-heading">
    <div class="download-heading">
      <div>
        <span class="eyebrow small">GET NEKOCRATE</span>
        <h2 id="download-heading">下载启动器</h2>
      </div>
      <a
        :href="release.url"
        class="release-link"
        target="_blank"
        rel="noopener noreferrer"
        ><span class="version">{{ release.tag }}</span
        ><span class="release-badge">{{
          release.prerelease ? "开发版" : "正式版"
        }}</span
        ><span class="release-date">{{ date }}</span
        ><ArrowUpRight :size="15"
      /></a>
    </div>
    <div class="download-content">
      <div class="platform-section">
        <div class="platforms" role="group" aria-label="选择操作系统">
          <button
            v-for="item in platforms"
            :key="item.id"
            :aria-pressed="platform === item.id"
            @click="platform = item.id"
          >
            <component :is="item.icon" :size="23" /><span
              >{{ item.label }}<small>{{ item.arch }}</small></span
            ><i />
          </button>
        </div>
        <div class="download-mode-row">
          <div class="segmented" role="group" aria-label="运行方式">
            <button
              :aria-pressed="mode === 'installed'"
              @click="mode = 'installed'"
            >
              <Box :size="15" />安装版</button
            ><button
              :aria-pressed="mode === 'portable'"
              @click="mode = 'portable'"
            >
              <PackageOpen :size="15" />便携版
            </button>
          </div>
          <a :href="`${base}docs/#portable`" class="quiet-link"
            >如何选择 <ChevronRight :size="13"
          /></a>
        </div>
      </div>
      <div class="download-action">
        <a
          v-if="asset"
          class="primary-button"
          :href="asset.url"
          :aria-label="`下载 ${current.label} ${current.arch} ${mode === 'installed' ? '安装版' : '便携版'}`"
          ><ArrowDown :size="21" /><span>下载 {{ current.label }}</span
          ><span class="download-extension">{{ current[mode] }}</span></a
        >
        <a v-else class="primary-button" :href="release.url"
          >查看可用构建 <ArrowUpRight :size="18"
        /></a>
        <p class="download-meta">
          <template v-if="asset"
            >{{ formatSize(asset.size) }}<span>·</span>{{ current.arch
            }}<span>·</span>GitHub 官方发布仓库</template
          ><template v-else>当前发布尚未提供此构建</template>
        </p>
        <Transition name="text-change" mode="out-in"
          ><p :key="`${platform}-${mode}`" class="download-note">
            {{ note }}
          </p></Transition
        >
      </div>
    </div>
    <div class="download-bottom">
      <span
        ><span class="status-dot" />{{
          release.prerelease
            ? "持续开发中，欢迎反馈使用体验。"
            : "下载来自项目的 GitHub Releases。"
        }}</span
      ><a :href="`${base}docs/#install`"
        >系统要求与安装帮助 <ArrowUpRight :size="14"
      /></a>
    </div>
  </section>

  <section class="explore-links" aria-label="继续了解">
    <a :href="`${base}features/`"
      ><Box :size="23" />
      <div>
        <h2>先看看它能做什么</h2>
        <p>拖放资源、整理版本，定制你的启动界面。</p>
      </div>
      <ArrowRight :size="20"
    /></a>
    <a :href="`${base}docs/`"
      ><BookOpen :size="23" />
      <div>
        <h2>从第一次启动开始</h2>
        <p>安装、关联目录、登录，以及常见问题。</p>
      </div>
      <ArrowRight :size="20"
    /></a>
  </section>
  <div class="download-history">
    <a
      :href="`${repositoryUrl}/releases`"
      target="_blank"
      rel="noopener noreferrer"
      >历史版本与完整更新记录 <ArrowUpRight :size="13"
    /></a>
  </div>
</template>
