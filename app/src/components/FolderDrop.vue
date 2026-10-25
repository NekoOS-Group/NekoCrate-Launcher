<script setup lang="ts">
import { Check, FolderInput, Info, LoaderCircle, X } from "lucide-vue-next";
import GlassDialog from "./GlassDialog.vue";
import { activeDialog } from "../dialogSurface";
import { prepareDialogLeave, finishDialogLeave } from "../dialogMotion";
import { t, translateMessage } from "../i18n";
import type { FolderDropController } from "../useFolderDrop";
const { controller, pack = false } = defineProps<{
  controller: FolderDropController;
  pack?: boolean;
}>();
const { state, available, selected, blocked, previewVisible, cornerBusy } =
  controller;
</script>

<template>
  <Transition
    name="dialog"
    appear
    @before-leave="prepareDialogLeave"
    @after-leave="finishDialogLeave"
  >
    <GlassDialog
      v-if="previewVisible"
      :label="t('关联实例')"
      :locked="state.phase === 'importing'"
      @close="controller.close"
    >
      <section
        class="utility-panel glass-surface folder-import-panel"
        :aria-busy="state.phase !== 'preview'"
      >
        <header class="utility-panel-header">
          <h2 class="dialog-heading" tabindex="-1">
            {{ t("关联实例") }}
          </h2>
          <button
            class="icon-button"
            :aria-label="t('关闭面板')"
            :disabled="state.phase === 'importing'"
            @click="controller.close"
          >
            <X :size="18" />
          </button>
        </header>
        <div class="utility-panel-content folder-import-content">
          <div class="folder-import-summary">
            <span>{{
              t("找到 {count} 个游戏目录", {
                count: state.scan.candidates.length,
              })
            }}</span>
            <button
              v-if="available.length > 1"
              class="text-button"
              :disabled="state.phase === 'importing'"
              @click="controller.toggleAll"
            >
              {{
                t(selected.length === available.length ? "取消全选" : "全选")
              }}
            </button>
          </div>
          <p class="folder-import-hint">
            {{ t("直接使用原目录，游戏文件保持原位。") }}
          </p>
          <div class="folder-candidate-list">
            <label
              v-for="candidate in state.scan.candidates"
              :key="candidate.path"
              class="folder-candidate"
              :class="{
                unavailable: candidate.existing_id || candidate.blocked,
              }"
              :title="
                candidate.blocked
                  ? translateMessage(candidate.blocked)
                  : undefined
              "
            >
              <input
                type="checkbox"
                :checked="
                  state.selected.has(candidate.path) &&
                  !candidate.existing_id &&
                  !candidate.blocked
                "
                :disabled="
                  !!candidate.existing_id ||
                  !!candidate.blocked ||
                  state.phase === 'importing'
                "
                @change="controller.toggle(candidate.path)"
              />
              <span class="folder-candidate-details">
                <span class="folder-candidate-heading"
                  ><strong>{{ candidate.name }}</strong
                  ><small v-if="candidate.existing_id">{{ t("已关联") }}</small
                  ><small v-else-if="candidate.blocked">{{
                    t("无法关联")
                  }}</small></span
                >
                <small class="folder-candidate-path">{{
                  candidate.path
                }}</small>
                <small class="folder-candidate-versions">{{
                  candidate.versions.map((v) => v.id).join(" · ") ||
                  t("尚无版本")
                }}</small>
              </span>
            </label>
          </div>
          <details v-if="state.scan.skipped" class="folder-scan-issues">
            <summary>
              {{ t("已跳过 {count} 项", { count: state.scan.skipped }) }}
            </summary>
            <p v-for="(issue, index) in state.scan.issues" :key="index">
              <span>{{ issue.path }}</span
              ><br />{{ translateMessage(issue.message) }}
            </p>
          </details>
        </div>
        <footer class="modal-actions">
          <button
            class="button"
            :disabled="state.phase === 'importing'"
            @click="controller.close"
          >
            {{ t("取消") }}
          </button>
          <button
            class="button primary"
            :disabled="blocked || !selected.length"
            :title="
              blocked && state.phase === 'preview'
                ? t('请等待当前任务完成后关联')
                : undefined
            "
            @click="controller.importSelected"
          >
            <LoaderCircle
              v-if="state.phase === 'importing'"
              class="spin"
              :size="16"
            />{{ t("关联所选 ({count})", { count: selected.length }) }}
          </button>
        </footer>
      </section>
    </GlassDialog>
  </Transition>
  <Teleport :to="activeDialog || 'body'" :disabled="!activeDialog">
    <Transition name="error-notice">
      <div v-if="state.dragging" class="folder-drop-overlay" aria-hidden="true">
        <div>
          <FolderInput :size="30" /><span>{{
            t(pack ? "拖拽导入整合包" : "拖拽导入实例")
          }}</span>
        </div>
      </div>
    </Transition>
    <Transition name="error-notice">
      <div
        v-if="cornerBusy || state.notice"
        class="folder-drop-notice glass-surface"
        role="status"
      >
        <template v-if="cornerBusy">
          <LoaderCircle class="spin" :size="16" />
          <span>{{
            t(state.phase === "scanning" ? "正在查找实例" : "正在关联实例")
          }}</span>
          <button
            v-if="state.phase === 'scanning'"
            class="icon-button"
            :aria-label="t('取消')"
            @click="controller.close"
          >
            <X :size="15" />
          </button>
        </template>
        <template v-else-if="state.notice">
          <Check v-if="state.notice.length" :size="16" /><Info
            v-else
            :size="16"
          />
          <div class="folder-drop-messages">
            <p v-if="!state.notice.length">{{ t("未找到实例") }}</p>
            <p v-for="item in state.notice" :key="item.path">
              {{
                t("已关联实例 {name}: {path}", {
                  name: item.name,
                  path: item.path,
                })
              }}
            </p>
          </div>
        </template>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.folder-import-panel {
  width: 580px;
}
.folder-import-panel .utility-panel-header {
  margin-bottom: 12px;
}
.folder-import-content {
  padding-bottom: 0;
}
.folder-import-summary,
.folder-candidate-heading {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
}
.folder-import-summary {
  font-size: 13px;
}
.folder-import-hint,
.folder-scan-issues {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
}
.folder-import-hint {
  margin: 6px 0 12px;
}
.folder-candidate {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 14px 0;
  border-top: 1px solid var(--line);
  cursor: pointer;
}
.folder-candidate.unavailable {
  cursor: default;
  opacity: 0.6;
}
.folder-candidate input {
  flex: 0 0 16px;
  width: 16px;
  height: 16px;
  margin: 0;
  accent-color: var(--blue);
}
.folder-candidate-details {
  display: flex;
  flex: 1;
  min-width: 0;
  flex-direction: column;
  gap: 4px;
}
.folder-candidate-heading strong {
  font-size: 14px;
  overflow-wrap: anywhere;
}
.folder-candidate-heading small {
  white-space: nowrap;
  color: var(--muted);
}
.folder-candidate-path {
  color: var(--muted);
  font-size: 11px;
  overflow-wrap: anywhere;
  line-height: 1.5;
}
.folder-candidate-versions {
  color: var(--muted);
  font-size: 11px;
  overflow-wrap: anywhere;
}
.folder-scan-issues p {
  overflow-wrap: anywhere;
}
.folder-drop-overlay {
  position: fixed;
  inset: 14px;
  display: grid;
  place-items: center;
  border: 1px dashed var(--blue);
  box-shadow: inset 0 0 40px var(--glow);
  z-index: 90;
  pointer-events: none;
}
.folder-drop-overlay > div {
  display: flex;
  gap: 18px;
  align-items: center;
  padding: 24px 32px;
  color: var(--blue);
}
.folder-drop-notice {
  position: fixed;
  top: 90px;
  right: 28px;
  z-index: 80;
  display: flex;
  gap: 12px;
  align-items: center;
  padding: 10px 12px 10px 16px;
  max-width: calc(100vw - 56px);
  font-size: 12px;
  box-shadow: 0 0 24px var(--glow);
}
.folder-drop-messages {
  min-width: 0;
  max-height: 180px;
  overflow-y: auto;
}
.folder-drop-messages p {
  margin: 0;
  overflow-wrap: anywhere;
  line-height: 1.6;
}
</style>
