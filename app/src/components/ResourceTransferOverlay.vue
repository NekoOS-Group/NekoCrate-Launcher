<script setup lang="ts">
import {
  Copy,
  Package,
  Image,
  Sparkles,
  FileCode2,
  Map,
  Trash2,
  Coffee,
} from "lucide-vue-next";
import { t } from "../i18n";
import { activeDialog } from "../dialogSurface";
import type {
  ResourceDestination,
  ResourceDragKind,
} from "../useResourceTransfer";
type DragDisplay = {
  kind: ResourceDragKind | "java";
  name: string;
  removable?: boolean;
};
defineProps<{
  trashOnly?: boolean;
  drag?: {
    source: DragDisplay;
    sources?: DragDisplay[];
    x: number;
    y: number;
    target?: ResourceDestination;
    native?: boolean;
  };
}>();
</script>
<template>
  <Teleport :to="activeDialog || 'body'">
    <Transition name="instance-trash">
      <div
        v-if="
          drag &&
          (drag.sources || [drag.source]).every(
            (source) => source.removable !== false,
          )
        "
        class="instance-trash-target resource-trash-target"
        :class="{ active: drag.target?.trash }"
        role="img"
        :aria-label="t('拖到此处移入回收站')"
      >
        <Trash2 :size="28" />
        <small v-if="drag.target?.trash">{{ t("移入回收站") }}</small>
      </div>
    </Transition>
    <div
      v-if="drag && !drag.native"
      class="resource-drag-ghost glass-surface"
      :class="{
        ready: drag.target,
        removing: drag.target?.trash,
        multiple: (drag.sources?.length || 0) > 1,
      }"
      :style="{
        transform: `translate3d(${drag.x + 16}px, ${drag.y + 16}px, 0)`,
      }"
    >
      <component
        :is="
          {
            mods: Package,
            resourcepacks: Image,
            shaderpacks: Sparkles,
            datapacks: FileCode2,
            saves: Map,
            java: Coffee,
          }[drag.source.kind]
        "
        :size="18"
      />
      <span>{{ drag.source.name }}</span
      ><small v-if="(drag.sources?.length || 0) > 1" class="drag-count">{{
        drag.sources?.length
      }}</small
      ><component
        :is="trashOnly || drag.target?.trash ? Trash2 : Copy"
        :size="13"
      />
    </div>
  </Teleport>
</template>
<style>
.resource-transferring,
.resource-transferring * {
  user-select: none !important;
  -webkit-user-select: none !important;
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
.resource-drag-ghost.multiple {
  box-shadow:
    3px 3px 0 color-mix(in srgb, var(--blue) 25%, transparent),
    6px 6px 0 color-mix(in srgb, var(--blue) 12%, transparent),
    0 8px 30px #0003;
}
.drag-count {
  padding: 2px 5px;
  color: var(--blue);
  font-variant-numeric: tabular-nums;
}
.resource-drag-ghost span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.resource-drag-ghost.ready {
  color: var(--blue);
}
.resource-drag-ghost.removing {
  color: var(--danger);
}
.resource-trash-target {
  z-index: 122;
}
.resource-trash-target small {
  position: absolute;
  top: 64px;
  white-space: nowrap;
  font-size: 11px;
}
</style>
