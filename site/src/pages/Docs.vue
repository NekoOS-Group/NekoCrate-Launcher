<script setup>
import { ref, computed, onMounted, onBeforeUnmount } from "vue";
import {
  Search,
  ArrowLeft,
  ArrowRight,
  ArrowUpRight,
  BookOpen,
  Copy,
  Check,
  ChevronRight,
} from "lucide-vue-next";
import { articles } from "../docs";
import { repositoryUrl } from "../releases.mjs";
const base = import.meta.env.BASE_URL;
const query = ref("");
const current = ref("start");
const copied = ref("");
const copyIssue = ref("");
let copyTimer;
const article = computed(
  () => articles.find((item) => item.id === current.value) || articles[0],
);
const position = computed(() => articles.indexOf(article.value));
const filtered = computed(() => {
  const term = query.value.trim().toLocaleLowerCase();
  return articles.filter(
    (item) =>
      !term ||
      [
        item.title,
        item.summary,
        ...item.sections.flatMap((section) => [
          section.title,
          ...(section.paragraphs || []),
        ]),
      ]
        .join(" ")
        .toLocaleLowerCase()
        .includes(term),
  );
});
const groups = computed(() => [
  ...new Set(filtered.value.map((item) => item.group)),
]);
function sync() {
  const hash = location.hash.slice(1);
  current.value = articles.some((item) => item.id === hash) ? hash : "start";
  document.title = `${article.value.title} · 猫咪盒子使用文档`;
}
function navigate() {
  query.value = "";
  requestAnimationFrame(() =>
    document
      .querySelector(".doc-article")
      ?.scrollIntoView({ block: "start", behavior: "smooth" }),
  );
}
function keyboard(event) {
  if (
    event.key === "/" &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.target.closest('input, textarea, [contenteditable="true"]')
  ) {
    event.preventDefault();
    document.querySelector(".doc-search input")?.focus();
  }
}
async function copy(code, id) {
  try {
    await navigator.clipboard.writeText(code);
    copied.value = id;
    copyIssue.value = "";
    clearTimeout(copyTimer);
    copyTimer = setTimeout(() => (copied.value = ""), 1800);
  } catch {
    copyIssue.value = "未能复制，请选中命令手动复制。";
  }
}
onMounted(() => {
  sync();
  window.addEventListener("hashchange", sync);
  window.addEventListener("keydown", keyboard);
});
onBeforeUnmount(() => {
  window.removeEventListener("hashchange", sync);
  window.removeEventListener("keydown", keyboard);
  clearTimeout(copyTimer);
});
</script>

<template>
  <header class="docs-heading">
    <div>
      <span class="eyebrow">DOCUMENTATION</span>
      <h1>使用文档</h1>
    </div>
    <label class="doc-search"
      ><Search :size="17" /><input
        v-model="query"
        type="search"
        placeholder="搜索文档"
        aria-label="搜索文档"
      /><kbd>/</kbd></label
    >
  </header>
  <div class="docs-layout">
    <aside class="docs-sidebar" aria-label="文档目录">
      <nav>
        <section v-for="group in groups" :key="group">
          <h2>{{ group }}</h2>
          <a
            v-for="item in filtered.filter((item) => item.group === group)"
            :key="item.id"
            :href="`#${item.id}`"
            :aria-current="article.id === item.id ? 'page' : undefined"
            @click="navigate"
            ><span>{{ item.title }}</span
            ><ChevronRight :size="13"
          /></a>
        </section>
      </nav>
      <p v-if="!filtered.length" class="no-results">没有找到相关文档。</p>
      <a
        class="docs-source"
        :href="`${repositoryUrl}/tree/neko_dev/docs`"
        target="_blank"
        rel="noopener noreferrer"
        ><BookOpen :size="15" />仓库技术文档<ArrowUpRight :size="13"
      /></a>
    </aside>
    <Transition name="article" mode="out-in"
      ><article :key="article.id" class="doc-article">
        <div class="breadcrumb">
          文档 <ChevronRight :size="12" /> {{ article.group }}
        </div>
        <h2>{{ article.title }}</h2>
        <p class="doc-summary">{{ article.summary }}</p>
        <section
          v-for="(section, index) in article.sections"
          :key="section.title"
          class="doc-section"
        >
          <h3>{{ section.title }}</h3>
          <p v-for="text in section.paragraphs" :key="text">{{ text }}</p>
          <div v-if="section.code" class="code-block">
            <button
              class="icon-button"
              :aria-label="
                copied === `${article.id}-${index}` ? '已复制' : '复制命令'
              "
              @click="copy(section.code, `${article.id}-${index}`)"
            >
              <Check
                v-if="copied === `${article.id}-${index}`"
                :size="16"
              /><Copy v-else :size="16" />
            </button>
            <pre><code>{{ section.code }}</code></pre>
          </div>
          <a
            v-if="section.link"
            class="text-link"
            :href="`${base}${section.link.href}`"
            >{{ section.link.label }}<ArrowRight :size="15"
          /></a>
          <a
            v-if="section.external"
            class="text-link"
            :href="section.external.href"
            target="_blank"
            rel="noopener noreferrer"
            >{{ section.external.label }}<ArrowUpRight :size="15"
          /></a>
        </section>
        <p class="copy-feedback" role="status">
          {{ copyIssue || (copied ? "已复制" : "") }}
        </p>
        <nav class="doc-pagination" aria-label="相邻文档">
          <a
            v-if="position > 0"
            :href="`#${articles[position - 1].id}`"
            @click="navigate"
            ><ArrowLeft :size="16" /><span
              ><small>上一篇</small>{{ articles[position - 1].title }}</span
            ></a
          ><span v-else /><a
            v-if="position < articles.length - 1"
            :href="`#${articles[position + 1].id}`"
            @click="navigate"
            ><span><small>下一篇</small>{{ articles[position + 1].title }}</span
            ><ArrowRight :size="16"
          /></a>
        </nav></article
    ></Transition>
  </div>
</template>
