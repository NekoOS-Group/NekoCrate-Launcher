<script setup lang="ts">
import {
  Copy,
  Package,
  Image,
  Sparkles,
  FileCode2,
  Map,
} from "lucide-vue-next";
import { activeDialog } from "../dialogSurface";
import type {
  ResourceCopySource,
  ResourceDestination,
} from "../useResourceTransfer";
defineProps<{
  drag?: {
    source: ResourceCopySource;
    x: number;
    y: number;
    target?: ResourceDestination;
    native?: boolean;
  };
}>();
</script>
<template>
  <Teleport :to="activeDialog || 'body'">
    <div
      v-if="drag && !drag.native"
      class="resource-drag-ghost glass-surface"
      :class="{ ready: drag.target }"
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
          }[drag.source.kind]
        "
        :size="18"
      />
      <span>{{ drag.source.name }}</span
      ><Copy :size="13" />
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
.resource-drag-ghost span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.resource-drag-ghost.ready {
  color: var(--blue);
}
</style>
