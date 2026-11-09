<script setup>
import {
  ArrowRight,
  ArrowUpRight,
  MousePointer2,
  Boxes,
  Layers3,
  Coffee,
  FileText,
  ShieldCheck,
  Move,
  Download,
  Palette,
  Folder,
  Package,
} from "lucide-vue-next";
import LauncherPreview from "../components/LauncherPreview.vue";
import { repositoryUrl } from "../releases.mjs";
import grass from "../../../app/src/assets/blocks/grass_block_side.png";
import ice from "../../../app/src/assets/blocks/ice.png";
const base = import.meta.env.BASE_URL;
const capabilities = [
  {
    icon: Layers3,
    title: "版本和加载器，一起准备",
    text: "原版、Fabric、Forge、NeoForge。同一游戏目录管理多个版本，按需要隔离 Mods、配置和存档。",
  },
  {
    icon: Package,
    title: "从整合包，到每一个 Mod",
    text: "浏览 Modrinth 与 CurseForge，查看介绍、选择版本。安装中断后可以继续，缺少的手动文件也有清晰指引。",
  },
  {
    icon: Coffee,
    title: "Java 交给启动器",
    text: "发现本机 Java，匹配游戏与加载器的要求。优先复用兼容版本，缺少时再下载。",
  },
  {
    icon: Download,
    title: "每一步都有进度",
    text: "下载、复用、校验和安装分别展示。支持暂停、续传和失败文件重试，保留已经完成的工作。",
  },
  {
    icon: FileText,
    title: "遇到问题，看到原因",
    text: "日志支持筛选、搜索和等级高亮。异常退出时展示诊断依据，并为常见依赖问题提供处理入口。",
  },
  {
    icon: ShieldCheck,
    title: "开源，也尊重你的文件",
    text: "已有游戏目录原地关联，复制保留源文件。账户凭据保存在系统凭据库，更新安装前验证签名。",
  },
];
</script>

<template>
  <header class="page-heading">
    <span class="eyebrow">INSIDE NEKOCRATE</span>
    <h1>少一点整理，<br /><span class="accent">多一点游玩。</span></h1>
    <p>版本、资源和运行环境各就其位。常用操作，放在顺手的位置。</p>
  </header>
  <section class="design-feature">
    <div class="feature-intro">
      <span class="section-index">01 / YOUR SPACE</span>
      <h2>界面，按你的习惯来。</h2>
      <p>
        透明玻璃、像素细节和流动的几何背景。移动组件、调整布局，把服务器、存档和常用入口留在首页。
      </p>
      <div class="inline-tags">
        <span><Move :size="14" />自由布局</span
        ><span><Palette :size="14" />五种主题</span>
      </div>
      <p class="preview-hint">试试下方配色，或右侧的实例方块。</p>
    </div>
    <LauncherPreview />
  </section>
  <section class="drag-feature glass">
    <div class="drag-illustration" aria-hidden="true">
      <div class="resource-stack">
        <span><Package :size="19" />my-mod.jar</span
        ><span><Folder :size="19" />resourcepacks</span>
      </div>
      <div class="drag-path"><i /><i /><i /><ArrowRight :size="21" /></div>
      <div class="drop-target">
        <img :src="grass" alt="" /><img :src="ice" alt="" /><MousePointer2
          :size="27"
        />
      </div>
    </div>
    <div>
      <span class="section-index">02 / DRAG & DROP</span>
      <h2>拿起来，放到目标位置。</h2>
      <p>
        把游戏目录拖进窗口，把 Mod
        拖到另一个版本。目标区域会告诉你接下来发生什么，导入、复制和整理由同一套流程完成。
      </p>
      <a :href="`${base}docs/#resources`" class="text-link"
        >了解资源与拖放 <ArrowRight :size="16"
      /></a>
    </div>
  </section>
  <section class="capability-grid" aria-label="功能介绍">
    <article v-for="item in capabilities" :key="item.title">
      <component :is="item.icon" :size="23" />
      <h2>{{ item.title }}</h2>
      <p>{{ item.text }}</p>
    </article>
  </section>
  <section class="feature-bottom">
    <div>
      <span class="eyebrow">OPEN SOURCE / RUST + TAURI</span>
      <h2>下载后，开始安排你的游戏。</h2>
    </div>
    <a class="primary-button" :href="base"
      >下载启动器 <ArrowRight :size="19"
    /></a>
  </section>
  <p class="feature-footnote">
    功能持续完善中，完整记录见
    <a
      :href="`${repositoryUrl}/releases`"
      target="_blank"
      rel="noopener noreferrer"
      >更新日志 <ArrowUpRight :size="12" /></a
    >。目前支持 Minecraft Java 版。
  </p>
</template>
