<script setup lang="ts">
import {
  computed,
  defineAsyncComponent,
  onBeforeUnmount,
  ref,
  shallowRef,
  watch,
} from "vue";
import { Download, LoaderCircle, RefreshCw, Users } from "lucide-vue-next";
import ProjectGallery from "./ProjectGallery.vue";
import ProjectLinks from "./ProjectLinks.vue";
import {
  projectMedia,
  projectLink,
  type ProjectDetails,
  type ResourceRpc,
} from "../projectPresentation";
import type { ResourcePlatform, ResourceProject } from "../resources";
import { locale, t } from "../i18n";
import { categoryLabel } from "../catalogFilters";
import { listRevealStyle } from "../listMotion";
const Description = defineAsyncComponent(
  () => import("./ProjectDescription.vue"),
);
const props = defineProps<{
  project: ResourceProject;
  platform: ResourcePlatform;
  rpc: ResourceRpc;
  compact?: boolean;
}>();
const emit = defineEmits<{
  link: [url: string];
  loaded: [detail: ProjectDetails];
}>();
const detail = shallowRef<ProjectDetails>();
const loading = ref(false),
  failed = ref(false);
let generation = 0;
async function load(refresh = false) {
  const request = ++generation;
  loading.value = !detail.value;
  failed.value = false;
  try {
    const result = await projectMedia(props.rpc).details(
      props.platform,
      props.project.id,
      refresh,
      props.project,
    );
    if (request === generation) {
      detail.value = result;
      emit("loaded", result);
      loading.value = false;
    }
    if (!refresh && result.refresh_required && request === generation) {
      try {
        const updated = await projectMedia(props.rpc).details(
          props.platform,
          props.project.id,
          true,
          props.project,
        );
        if (request === generation) {
          detail.value = updated;
          emit("loaded", updated);
        }
      } catch {
        /* Revalidation must never remove a readable cached description. */
      }
    }
  } catch {
    if (request === generation) failed.value = true;
  } finally {
    if (request === generation) loading.value = false;
  }
}
watch(
  () => [
    props.platform,
    props.project.id,
    props.project.updated,
    props.project.latest_version,
  ],
  (current, previous) => {
    if (!previous || current[0] !== previous[0] || current[1] !== previous[1])
      detail.value = undefined;
    void load();
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  generation++;
});
const downloads = computed(() =>
  new Intl.NumberFormat(locale.value, {
    notation: "compact",
    maximumFractionDigits: 1,
  }).format(detail.value?.project.downloads ?? props.project.downloads),
);
const date = computed(() => {
  const value = detail.value?.updated && new Date(detail.value.updated);
  return value && !Number.isNaN(+value)
    ? value.toLocaleDateString(locale.value)
    : "";
});
const links = computed(() => {
  const seen = new Set<string>();
  return [
    {
      label: props.platform === "modrinth" ? "Modrinth" : "CurseForge",
      url: detail.value?.project.url || props.project.url,
    },
    ...(detail.value?.links || []),
  ].filter((item) => {
    const url = projectLink(item.url);
    if (!url || seen.has(url)) return false;
    seen.add(url);
    return true;
  });
});
const license = computed(() =>
  detail.value?.license
    ? {
        name: detail.value.license,
        url: projectLink(detail.value.license_url || ""),
      }
    : undefined,
);
function link(value: string) {
  const url = projectLink(value);
  if (url) emit("link", url);
}
</script>
<template>
  <section
    class="project-details"
    :class="{ 'project-details-compact': compact }"
    :aria-label="t('项目介绍')"
  >
    <div
      class="project-heading"
      :class="{ 'list-reveal-item': compact }"
      :style="compact ? listRevealStyle(0) : undefined"
    >
      <slot name="leading" />
      <div class="project-heading-content">
        <slot name="heading" :links="links" :updated="detail?.updated" />
        <div class="project-metadata">
          <div class="project-facts">
            <slot name="facts" />
            <span :title="t('下载量')"
              ><Download :size="12" />{{ downloads }}</span
            >
            <span
              v-if="detail?.authors.length"
              class="project-authors"
              :title="`${t('作者')}: ${detail.authors.join(', ')}`"
              ><Users :size="12" /><span class="project-fact-text">{{
                detail.authors.join(", ")
              }}</span></span
            >
            <span v-if="date && !compact">{{ t("更新于") }} {{ date }}</span>
            <ProjectLinks
              v-if="!compact"
              :links="links"
              :license="license"
              @link="link"
            />
            <LoaderCircle v-if="loading" class="spin" :size="12" />
          </div>
          <div v-if="detail?.categories.length" class="project-tags">
            <span
              v-for="category in detail?.categories"
              :key="category"
              class="project-badge"
              >{{ categoryLabel(category) }}</span
            >
          </div>
        </div>
      </div>
      <slot name="actions" :links="links" :license="license" />
    </div>
    <p class="project-summary">
      {{ detail?.project.description || project.description }}
    </p>
    <ProjectGallery
      v-if="detail?.gallery.length"
      :images="detail.gallery"
      :rpc="rpc"
    />
    <Description
      v-if="detail?.body"
      :body="detail.body"
      :format="detail.body_format"
      :base="detail?.project.url || project.url"
      :rpc="rpc"
      @link="link"
    />
    <button v-if="failed" class="text-button project-retry" @click="load(true)">
      <RefreshCw :size="12" />{{ t("重新加载介绍") }}
    </button>
  </section>
</template>
<style scoped>
.project-details {
  min-width: 0;
}
.project-facts,
.project-tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 16px;
  color: var(--muted);
  font-size: 11px;
}
.project-facts > span {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.project-facts .text-button,
.project-tags .text-button {
  font-size: 11px;
  gap: 5px;
  padding: 0;
}
.project-tags {
  margin-top: 10px;
  gap: 6px;
}
.project-summary {
  font-size: 13px;
  line-height: 1.8;
  margin: 16px 0;
  overflow-wrap: anywhere;
}
.project-retry {
  font-size: 11px;
  color: var(--muted);
}
.project-heading-content {
  min-width: 0;
}
.project-details-compact .project-heading {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 4px;
}
.project-details-compact .project-heading-content {
  flex: 1;
}
.project-details-compact .project-metadata {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 3px 0 0;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.project-details-compact .project-metadata::-webkit-scrollbar {
  display: none;
}
.project-details-compact .project-facts {
  flex-wrap: nowrap;
  flex-shrink: 0;
  gap: 12px;
}
.project-details-compact .project-tags {
  flex-wrap: nowrap;
  flex-shrink: 0;
  margin: 0;
}
.project-details-compact .project-authors {
  min-width: 0;
}
.project-details-compact .project-facts svg {
  flex-shrink: 0;
}
.project-details-compact .project-fact-text {
  display: block;
  max-width: 18ch;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.project-details-compact .project-summary {
  margin: 12px 0;
}
</style>
