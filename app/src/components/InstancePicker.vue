<script setup lang="ts">
import { FolderInput, FolderOpen, LoaderCircle, Plus } from "lucide-vue-next";
import InstanceIcon from "./InstanceIcon.vue";
import { t } from "../i18n";
import type { SuggestedInstance } from "../types";

defineProps<{
  candidates: SuggestedInstance[];
  loading: boolean;
  disabled: boolean;
  linking: string;
  picking: boolean;
}>();
defineEmits<{ select: [path: string]; import: []; create: [] }>();
</script>

<template>
  <div class="instance-picker-content">
    <TransitionGroup
      name="instance-choice"
      tag="div"
      class="account-picker instance-picker"
      role="group"
      :aria-label="t('添加游戏目录')"
      :aria-busy="loading || !!linking"
    >
      <button
        v-for="candidate in candidates"
        :key="candidate.path"
        class="instance-choice"
        :disabled="disabled"
        :title="`${t(candidate.source === 'official' ? '官方游戏目录' : '启动器旁的游戏目录')}\n${candidate.path}`"
        :aria-label="
          t('关联实例 {name}：{path}', {
            name: candidate.name,
            path: candidate.path,
          })
        "
        @click="$emit('select', candidate.path)"
      >
        <span class="instance-choice-frame">
          <LoaderCircle
            v-if="linking === candidate.path"
            :size="24"
            class="spin"
          />
          <InstanceIcon v-else :icon="0" />
        </span>
        <span class="account-name">{{ candidate.name }}</span>
      </button>
      <button
        key="import"
        class="instance-choice"
        :disabled="disabled"
        :aria-label="t('导入文件夹')"
        @click="$emit('import')"
      >
        <span class="instance-choice-frame">
          <LoaderCircle v-if="picking" :size="24" class="spin" />
          <FolderOpen v-else :size="24" :stroke-width="1.5" />
        </span>
        <span class="account-name">{{ t("导入文件夹") }}</span>
      </button>
      <button
        key="create"
        class="instance-choice"
        :disabled="disabled"
        :aria-label="t('新建实例')"
        @click="$emit('create')"
      >
        <span class="instance-choice-frame"
          ><Plus :size="24" :stroke-width="1.5"
        /></span>
        <span class="account-name">{{ t("新建实例") }}</span>
      </button>
    </TransitionGroup>
    <span class="instance-discovery-status" role="status">
      <template v-if="loading"
        ><LoaderCircle :size="12" class="spin" />{{
          t("正在查找实例")
        }}</template
      >
      <template v-else>
        <FolderInput :size="12" />{{ t("可拖拽文件夹到窗口导入") }}
      </template>
    </span>
  </div>
</template>

<style scoped>
.instance-picker-content {
  min-height: 0;
  overflow: auto;
  padding: 0 var(--panel-inset) 20px;
}
.instance-picker {
  position: relative;
  min-height: 152px;
  padding-bottom: 12px;
}
.instance-choice {
  display: flex;
  flex: 0 0 104px;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  width: 104px;
  padding: 12px 8px;
  border: 0;
  background: transparent;
  color: var(--muted);
  transition: color 180ms var(--motion-ease);
}
.instance-choice-frame {
  display: grid;
  place-items: center;
  width: 56px;
  height: 56px;
  flex-shrink: 0;
  border: 1px solid var(--glass-stroke);
  background: color-mix(in srgb, var(--blue) 4%, transparent);
  box-shadow: var(--glass-rim);
  transition:
    border-color 200ms var(--motion-ease),
    box-shadow 240ms var(--motion-ease),
    transform 240ms var(--motion-ease);
}
.instance-choice-frame :deep(.instance-art) {
  width: 44px;
  height: 44px;
}
.instance-choice:hover:not(:disabled),
.instance-choice:focus-visible {
  color: var(--ink);
  outline: none;
}
.instance-choice:hover:not(:disabled) .instance-choice-frame,
.instance-choice:focus-visible .instance-choice-frame {
  border-color: var(--blue);
  box-shadow:
    var(--glass-rim),
    0 0 18px var(--glow);
  transform: translateY(-2px);
}
.instance-choice-frame > svg:not(.instance-art) {
  color: var(--blue);
}
.instance-discovery-status {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  min-height: 16px;
  font-size: 11px;
  line-height: 16px;
  color: var(--muted);
}
.instance-choice-enter-active,
.instance-choice-leave-active,
.instance-choice-move {
  transition:
    transform 320ms var(--motion-ease),
    opacity 220ms ease;
}
.instance-choice-enter-from,
.instance-choice-leave-to {
  opacity: 0;
  transform: translateY(8px) scale(0.94);
}
.instance-choice-leave-active {
  position: absolute;
}
@media (prefers-reduced-motion: reduce) {
  .instance-choice,
  .instance-choice-frame {
    transition: none;
  }
}
</style>
