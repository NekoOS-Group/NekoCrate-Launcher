<script setup>
import { ref, computed, watchEffect, onMounted, onBeforeUnmount } from "vue";
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
import { articles as zhArticles } from "../docs";
import { articles as enArticles } from "../docs.en";
import { route, href } from "../router";
import { lang, t } from "../i18n";
import { repositoryUrl } from "../releases.mjs";
const query = ref("");
const articles = computed(() => (lang.value === "zh" ? zhArticles : enArticles));
const copied = ref("");
const copyIssue = ref("");
let copyTimer;
const article = computed(
  () =>
    articles.value.find((item) => item.id === route.value.sub) ||
    articles.value[0],
);
const position = computed(() => articles.value.indexOf(article.value));
const filtered = computed(() => {
  const term = query.value.trim().toLocaleLowerCase();
  return articles.value.filter(
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
watchEffect(() => {
  if (route.value.page === "docs")
    document.title = `${article.value.title} · ${t.value.meta.docs}`;
});
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
    route.value.page === "docs" &&
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
    copyIssue.value = t.value.docs.copyFailed;
  }
}
onMounted(() => window.addEventListener("keydown", keyboard));
onBeforeUnmount(() => {
  window.removeEventListener("keydown", keyboard);
  clearTimeout(copyTimer);
});
</script>

<template>
  <header class="docs-heading">
    <h1>{{ t.docs.title }}</h1>
    <label class="doc-search"
      ><Search :size="17" /><input
        v-model="query"
        type="search"
        :placeholder="t.docs.search"
        :aria-label="t.docs.search"
      /><kbd>/</kbd></label
    >
  </header>
  <div class="docs-layout">
    <aside class="docs-sidebar" :aria-label="t.docs.toc">
      <nav>
        <section v-for="group in groups" :key="group">
          <h2>{{ group }}</h2>
          <a
            v-for="item in filtered.filter((item) => item.group === group)"
            :key="item.id"
            :href="href('docs', item.id)"
            :aria-current="article.id === item.id ? 'page' : undefined"
            @click="navigate"
            ><span>{{ item.title }}</span
            ><ChevronRight :size="13"
          /></a>
        </section>
      </nav>
      <p v-if="!filtered.length" class="no-results">{{ t.docs.none }}</p>
      <a
        class="docs-source"
        :href="`${repositoryUrl}/tree/neko_dev/docs`"
        target="_blank"
        rel="noopener noreferrer"
        ><BookOpen :size="15" />{{ t.docs.source }}<ArrowUpRight :size="13"
      /></a>
    </aside>
    <Transition name="article" mode="out-in"
      ><article :key="article.id" class="doc-article">
        <div class="breadcrumb">
          {{ t.docs.root }} <ChevronRight :size="12" /> {{ article.group }}
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
                copied === `${article.id}-${index}` ? t.docs.copied : t.docs.copy
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
            :href="section.link.href ? href('docs', section.link.href) : href('download')"
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
          {{ copyIssue || (copied ? t.docs.copied : "") }}
        </p>
        <nav class="doc-pagination" :aria-label="t.docs.siblings">
          <a
            v-if="position > 0"
            :href="href('docs', articles[position - 1].id)"
            @click="navigate"
            ><ArrowLeft :size="16" /><span
              ><small>{{ t.docs.prev }}</small>{{ articles[position - 1].title }}</span
            ></a
          ><span v-else /><a
            v-if="position < articles.length - 1"
            :href="href('docs', articles[position + 1].id)"
            @click="navigate"
            ><span><small>{{ t.docs.next }}</small>{{ articles[position + 1].title }}</span
            ><ArrowRight :size="16"
          /></a>
        </nav></article
    ></Transition>
  </div>
</template>
