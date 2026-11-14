<script setup lang="ts">
import { computed } from "vue";
import { ArrowRight, Download, Users } from "lucide-vue-next";
import { locale, t } from "../i18n";
import { categoryLabel } from "../catalogFilters";
import type { ResourceProject } from "../resources";
import type { ResourceRpc } from "../projectPresentation";
import ProjectImage from "./ProjectImage.vue";
withDefaults(
  defineProps<{
    project: ResourceProject;
    rpc: ResourceRpc;
    disabled?: boolean;
    hideDownloads?: boolean;
  }>(),
  { disabled: false, hideDownloads: false },
);
const emit = defineEmits<{ open: [] }>();
const downloadFormat = computed(
  () =>
    new Intl.NumberFormat(locale.value, {
      notation: "compact",
      maximumFractionDigits: 1,
    }),
);
</script>
<template>
  <article class="catalog-project" :class="{ disabled }">
    <ProjectImage :url="project.icon_url" :rpc="rpc" :size="42">
      <slot name="icon" />
    </ProjectImage>
    <div class="catalog-project-copy">
      <div class="catalog-project-heading">
        <div class="catalog-project-title">
          <button
            type="button"
            class="catalog-project-open"
            :disabled="disabled"
            :title="project.name"
            @click="emit('open')"
          >
            <strong>{{ project.name }}</strong>
          </button>
          <span
            v-if="project.categories?.length"
            class="catalog-project-tags"
            @click="!disabled && !$slots.tags && emit('open')"
          >
            <slot name="tags">
              <span
                v-for="tag in project.categories.slice(0, 3)"
                :key="tag"
                class="project-badge"
                :title="categoryLabel(tag)"
                >{{ categoryLabel(tag) }}</span
              >
              <span
                v-if="project.categories.length > 3"
                class="project-badge catalog-project-more-tags"
                :title="
                  project.categories.slice(3).map(categoryLabel).join(' · ')
                "
                >+{{ project.categories.length - 3 }}</span
              >
            </slot>
          </span>
        </div>
        <span
          v-if="!hideDownloads"
          class="catalog-project-downloads"
          :title="`${t('下载量')}: ${project.downloads.toLocaleString(locale)}`"
        >
          <Download :size="12" />{{ downloadFormat.format(project.downloads) }}
        </span>
        <span
          v-if="project.authors?.length"
          class="catalog-project-authors"
          :title="`${t('作者')}: ${project.authors.join(', ')}`"
        >
          <Users :size="12" /><span>{{ project.authors.join(", ") }}</span>
        </span>
      </div>
      <p>{{ project.description }}</p>
    </div>
    <ArrowRight :size="16" />
  </article>
</template>
<style scoped>
.catalog-project {
  position: relative;
  display: flex;
  gap: 15px;
  align-items: center;
  width: 100%;
  text-align: left;
  background: transparent;
  color: var(--ink);
  border: 0;
  border-bottom: 1px solid var(--line);
  padding: 16px 8px;
}
.catalog-project.disabled {
  opacity: 0.5;
}
.catalog-project > svg:first-child {
  color: var(--blue);
}
.catalog-project > svg {
  flex-shrink: 0;
}
.catalog-project > svg:last-child {
  color: var(--muted);
}
.catalog-project-copy {
  flex: 1;
  min-width: 0;
}
.catalog-project-heading {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 68px minmax(0, 16%);
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.catalog-project-title {
  grid-column: 1;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.catalog-project-open {
  min-width: 0;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}
.catalog-project-open::after {
  content: "";
  position: absolute;
  inset: 0;
}
.catalog-project-open:focus-visible {
  outline: none;
  color: var(--blue);
  text-shadow: 0 0 10px var(--glow);
}
.catalog-project-heading strong {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
  font-weight: 500;
}
.catalog-project-downloads,
.catalog-project-authors {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--muted);
  font-size: 11px;
  white-space: nowrap;
}
.catalog-project-downloads > svg,
.catalog-project-authors > svg {
  flex-shrink: 0;
}
.catalog-project-downloads {
  grid-column: 2;
  font-variant-numeric: tabular-nums;
}
.catalog-project-authors {
  grid-column: 3;
  min-width: 0;
}
.catalog-project-authors > span {
  overflow: hidden;
  text-overflow: ellipsis;
}
.catalog-project-tags {
  position: relative;
  z-index: 1;
  display: flex;
  gap: 4px;
  min-width: 0;
  max-width: 55%;
  flex-shrink: 0;
}
.catalog-project-tags :deep(.project-badge) {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: inherit;
  transition:
    color 160ms,
    background-color 160ms,
    box-shadow 160ms;
}
.catalog-project-tags :deep(button) {
  cursor: pointer;
}
.catalog-project-tags :deep(button:hover:not(:disabled)) {
  border-color: var(--blue);
}
.catalog-project-tags :deep(button.active) {
  background: var(--blue);
  color: var(--night);
  border-color: var(--blue);
}
.catalog-project-tags :deep(button:focus-visible) {
  outline: 1px solid var(--blue);
  outline-offset: 2px;
}
.catalog-project-tags :deep(button:disabled) {
  cursor: default;
}
.catalog-project-tags :deep(.catalog-project-more-tags) {
  flex-shrink: 0;
}
.catalog-project p {
  margin: 6px 0 0;
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  overflow-wrap: anywhere;
}
.catalog-project:has(.catalog-project-open:hover) strong {
  color: var(--blue);
  text-shadow: 0 0 10px var(--glow);
}
</style>
