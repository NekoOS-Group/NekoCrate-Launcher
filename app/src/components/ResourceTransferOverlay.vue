<script setup lang="ts">
import { computed } from "vue";
import { Box, Copy, Package, Image, Sparkles } from "lucide-vue-next";
import { activeDialog } from "../dialogSurface";
import { t } from "../i18n";
import { versionName } from "../versionDescription";
import type { Instance } from "../types";
import type {
  ResourceCopySource,
  ResourceDestination,
} from "../useResourceTransfer";
const props = defineProps<{
  drag?: {
    source: ResourceCopySource;
    x: number;
    y: number;
    target?: ResourceDestination;
  };
  instances: readonly Instance[];
  running: readonly string[];
}>();
const groups = computed(() =>
  props.instances
    .map((instance) => ({
      ...instance,
      versions: instance.versions.filter(
        (version) =>
          version.installed &&
          !(
            instance.id === props.drag?.source.instance &&
            version.id === props.drag?.source.version
          ),
      ),
    }))
    .filter((instance) => instance.versions.length),
);
</script>
<template>
  <Teleport :to="activeDialog || 'body'">
    <Transition name="resource-drop">
      <aside
        v-if="drag"
        class="resource-drop-panel glass-surface"
        :aria-label="t('复制到版本')"
      >
        <header><Copy :size="16" />{{ t("复制到版本") }}</header>
        <div class="resource-drop-list">
          <section v-for="instance in groups" :key="instance.id">
            <h4>{{ instance.name }}</h4>
            <button
              v-for="version in instance.versions"
              :key="version.id"
              type="button"
              :data-resource-copy-instance="instance.id"
              :data-resource-copy-version="version.id"
              :disabled="running.includes(instance.id)"
              :class="{
                'resource-drop-hover':
                  drag.target?.instance === instance.id &&
                  drag.target?.version === version.id,
              }"
            >
              <Box :size="17" /><span>{{ versionName(version) }}</span>
              <small v-if="running.includes(instance.id)">{{
                t("运行中")
              }}</small>
            </button>
          </section>
          <p v-if="!groups.length">{{ t("暂无其他已安装版本") }}</p>
        </div>
      </aside>
    </Transition>
    <div
      v-if="drag"
      class="resource-drag-ghost glass-surface"
      :class="{ ready: drag.target }"
      :style="{
        transform: `translate3d(${drag.x + 16}px, ${drag.y + 16}px, 0)`,
      }"
    >
      <component
        :is="
          { mods: Package, resourcepacks: Image, shaderpacks: Sparkles }[
            drag.source.kind
          ]
        "
        :size="18"
      /><span>{{ drag.source.name }}</span
      ><Copy :size="13" />
    </div>
  </Teleport>
</template>
<style>
.resource-transferring,
.resource-transferring * {
  cursor: copy !important;
  user-select: none !important;
}
.resource-drag-handle {
  cursor: grab;
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
}
.resource-drag-handle * {
  user-select: none;
  -webkit-user-select: none;
}
.resource-drop-panel.glass-surface {
  position: fixed;
  z-index: 120;
  right: 24px;
  top: 88px;
  bottom: 64px;
  width: min(270px, 42vw);
  display: flex;
  flex-direction: column;
  padding: 16px 12px;
  box-shadow: 0 12px 48px #0005;
}
.resource-drop-panel header {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 8px 12px;
  font-size: 13px;
}
.resource-drop-list {
  overflow-y: auto;
  min-height: 0;
  overscroll-behavior: contain;
}
.resource-drop-list h4 {
  font-size: 11px;
  font-weight: 400;
  opacity: 0.6;
  margin: 15px 8px 6px;
}
.resource-drop-list section:first-child h4 {
  margin-top: 4px;
}
.resource-drop-list button {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 11px 8px;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  font: inherit;
  font-size: 13px;
  transition:
    color 150ms,
    filter 150ms,
    transform 180ms;
}
.resource-drop-list button span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.resource-drop-list button small {
  font-size: 10px;
  white-space: nowrap;
}
.resource-drop-list button:disabled {
  opacity: 0.35;
}
.resource-drop-list p {
  font-size: 12px;
  opacity: 0.6;
}
.resource-drop-hover {
  color: var(--blue) !important;
  filter: drop-shadow(0 0 7px var(--blue));
}
.resource-drop-list .resource-drop-hover {
  transform: translateX(3px);
}
.resource-drag-ghost.glass-surface {
  position: fixed;
  z-index: 121;
  left: 0;
  top: 0;
  pointer-events: none;
  display: flex;
  align-items: center;
  gap: 9px;
  max-width: 260px;
  padding: 12px 15px;
  box-shadow: 0 8px 30px #0003;
  font-size: 12px;
}
.resource-drag-ghost span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.resource-drag-ghost.ready {
  color: var(--blue);
}
.resource-drop-enter-active,
.resource-drop-leave-active {
  transition:
    opacity 150ms,
    transform 220ms cubic-bezier(0.2, 0.8, 0.2, 1);
}
.resource-drop-enter-from,
.resource-drop-leave-to {
  opacity: 0;
  transform: translateX(10px);
}
@media (prefers-reduced-motion: reduce) {
  .resource-drop-panel,
  .resource-drop-list button {
    transition: none;
  }
}
</style>
