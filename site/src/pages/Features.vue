<script setup>
import { computed } from "vue";
import { ArrowRight, ArrowUpRight, Check } from "lucide-vue-next";
import LauncherPreview from "../components/LauncherPreview.vue";
import DragDemo from "../components/DragDemo.vue";
import TaskDemo from "../components/TaskDemo.vue";
import LogDemo from "../components/LogDemo.vue";
import { features } from "../content/features";
import { repositoryUrl } from "../releases.mjs";
import { href } from "../router";
import { lang } from "../i18n";

const c = computed(() => features[lang.value]);
const chapters = computed(() => [
  { id: "home", copy: c.value.home, demo: LauncherPreview },
  { id: "drag", copy: c.value.drag, demo: DragDemo, link: "resources" },
  { id: "tasks", copy: c.value.tasks, demo: TaskDemo, link: "downloads" },
  { id: "logs", copy: c.value.logs, demo: LogDemo, link: "downloads" },
]);
const docsLabel = computed(() =>
  lang.value === "zh" ? "相关文档" : "Read the docs",
);
</script>

<template>
  <header class="page-heading features-heading">
    <h1>{{ c.title[0] }}<br />{{ c.title[1] }}</h1>
    <p>{{ c.lede }}</p>
  </header>

  <section
    v-for="(chapter, index) in chapters"
    :id="`feature-${chapter.id}`"
    :key="chapter.id"
    class="chapter"
    :class="{ flip: index % 2 }"
    :aria-labelledby="`feature-${chapter.id}-title`"
  >
    <div class="chapter-copy">
      <h2 :id="`feature-${chapter.id}-title`">{{ chapter.copy.title }}</h2>
      <p>{{ chapter.copy.text }}</p>
      <ul v-if="chapter.copy.points" class="chapter-points">
        <li v-for="point in chapter.copy.points" :key="point">
          <Check :size="15" />{{ point }}
        </li>
      </ul>
      <p v-if="chapter.copy.hint" class="chapter-hint">{{ chapter.copy.hint }}</p>
      <a v-if="chapter.link" :href="href('docs', chapter.link)" class="text-link"
        >{{ docsLabel }}<ArrowRight :size="15"
      /></a>
    </div>
    <div class="chapter-demo">
      <component :is="chapter.demo" :copy="chapter.copy" />
    </div>
    <div v-if="chapter.id === 'drag'" class="drag-table">
      <h3>{{ chapter.copy.tableTitle }}</h3>
      <dl>
        <div v-for="row in chapter.copy.table" :key="row[0]">
          <dt>{{ row[0] }}</dt>
          <dd>{{ row[1] }}</dd>
        </div>
      </dl>
    </div>
  </section>

  <section class="more-features" aria-labelledby="more-title">
    <h2 id="more-title">{{ c.more.title }}</h2>
    <div class="more-grid">
      <section v-for="group in c.more.groups" :key="group[0]">
        <h3>{{ group[0] }}</h3>
        <ul>
          <li v-for="item in group[1]" :key="item">{{ item }}</li>
        </ul>
      </section>
    </div>
  </section>

  <section class="feature-bottom">
    <div>
      <h2>{{ c.bottomTitle }}</h2>
      <p>
        {{ c.bottomText }}
        <a
          :href="`${repositoryUrl}/releases`"
          target="_blank"
          rel="noopener noreferrer"
          >{{ c.changelog }}<ArrowUpRight :size="12"
        /></a>
      </p>
    </div>
    <a class="primary-button" :href="href('download')"
      >{{ c.bottomButton }} <ArrowRight :size="19"
    /></a>
  </section>
</template>
