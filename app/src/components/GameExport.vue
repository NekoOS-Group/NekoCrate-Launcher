<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from "vue";
import { Archive, Download, Grip, X } from "lucide-vue-next";
import ContentSwitch from "./ContentSwitch.vue";
import { listRevealStyle } from "../listMotion";
import GlassDialog from "./GlassDialog.vue";
import GlassSelect from "./GlassSelect.vue";
import InstanceIcon from "./InstanceIcon.vue";
import { t } from "../i18n";
import type { Instance, Task } from "../types";
import type { ResourceRpc } from "../projectPresentation";
const props = defineProps<{
  instance: Instance;
  version?: string;
  rpc: ResourceRpc;
  build: (
    platform?: string,
  ) => Promise<{ id: string; name: string; size: number } | undefined>;
  task?: Task;
  cancel: () => unknown;
}>();
const emit = defineEmits<{ close: []; error: [error: unknown] }>();
const portable = ref(false);
const platform = ref(
  /mac/i.test(navigator.platform)
    ? "macos"
    : /win/i.test(navigator.platform)
      ? "windows"
      : "linux",
);
const platforms = [
  { value: "windows", label: "Windows · x64" },
  { value: "macos", label: "macOS · Apple Silicon" },
  { value: "linux", label: "Linux · x64" },
];
const output = ref<{ id: string; name: string; size: number }>();
const building = ref(false),
  dragging = ref(false),
  saving = ref(false);
const locked = computed(() => building.value || dragging.value || saving.value);
let disposed = false;
let handedToFileManager = false;
let suppressTileClick = false;
function clickTile(event: MouseEvent) {
  if (suppressTileClick) {
    event.preventDefault();
    event.stopPropagation();
    return;
  }
  void save();
}
async function build() {
  if (locked.value) return;
  building.value = true;
  try {
    output.value = await props.build(
      portable.value ? platform.value : undefined,
    );
    if (disposed && output.value)
      await props.rpc("discard_game_export", { export: output.value.id });
  } finally {
    building.value = false;
  }
}
async function save() {
  if (!output.value || locked.value) return;
  saving.value = true;
  try {
    await props.rpc("save_game_export", {
      export: output.value.id,
      name: output.value.name,
    });
  } catch (e) {
    emit("error", e);
  } finally {
    saving.value = false;
  }
}
// The ZIP already exists when native drag starts. No asynchronous file promise
// or delayed synthetic mouse event is needed on any supported platform.
let origin: { x: number; y: number; id: number } | undefined;
function down(e: PointerEvent) {
  suppressTileClick = false;
  if (e.button === 0 && !locked.value) {
    origin = { x: e.clientX, y: e.clientY, id: e.pointerId };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
}
async function move(e: PointerEvent) {
  if (
    !origin ||
    !output.value ||
    Math.hypot(e.clientX - origin.x, e.clientY - origin.y) < 6
  )
    return;
  const id = origin.id;
  origin = undefined;
  const target = e.currentTarget as HTMLElement;
  if (target.hasPointerCapture(id)) target.releasePointerCapture(id);
  dragging.value = true;
  suppressTileClick = true;
  try {
    handedToFileManager =
      (await props.rpc<boolean>("drag_game_export", {
        export: output.value.id,
      })) || handedToFileManager;
  } catch (e) {
    emit("error", e);
  } finally {
    dragging.value = false;
  }
}
onBeforeUnmount(() => {
  disposed = true;
  if (output.value && !handedToFileManager)
    void props
      .rpc("discard_game_export", { export: output.value.id })
      .catch(() => {});
});
</script>
<template>
  <GlassDialog :label="t('导出 ZIP')" :locked="locked" @close="emit('close')">
    <section class="modal glass-surface game-export-panel">
      <header
        class="modal-titlebar list-reveal-item"
        :style="listRevealStyle(0)"
      >
        <h2 class="dialog-heading" tabindex="-1">
          <Archive :size="19" />{{ t("导出 ZIP") }}
        </h2>
        <button
          class="icon-button"
          :aria-label="t('关闭')"
          :disabled="locked"
          @click="emit('close')"
        >
          <X :size="18" />
        </button>
      </header>
      <div
        class="game-export-identity list-reveal-item"
        :style="listRevealStyle(1)"
      >
        <InstanceIcon :icon="instance.icon ?? 0" />
        <div>
          <strong>{{ version || instance.name }}</strong
          ><small v-if="version">{{ instance.name }}</small>
        </div>
      </div>
      <ContentSwitch :content-key="output ? 'ready' : 'prepare'">
        <template v-if="!output">
          <div
            class="game-export-options list-reveal-item"
            :style="listRevealStyle(2)"
          >
            <div class="toggle-row export-portable-toggle">
              <span id="export-portable-label">{{ t("附带便携版启动器") }}</span
              ><input
                v-model="portable"
                type="checkbox"
                role="switch"
                aria-labelledby="export-portable-label"
                :disabled="building"
              />
            </div>
            <GlassSelect
              v-if="portable"
              id="export-platform"
              v-model="platform"
              :options="platforms"
              :disabled="building"
              :aria-label="t('平台')"
              floating
              compact
            />
          </div>
          <div v-if="building" class="game-export-progress" role="status">
            <span>{{ t(task?.message || "正在准备…") }}</span
            ><progress
              :value="task?.indeterminate ? undefined : task?.progress"
              max="100"
            />
          </div>
          <footer class="modal-actions">
            <button v-if="building" class="text-button" @click="cancel">
              {{ t("取消") }}</button
            ><button v-else class="button primary" @click="build">
              <Archive :size="16" />{{ t("生成 ZIP") }}
            </button>
          </footer>
        </template>
        <template v-else>
          <button
            class="game-export-file list-reveal-item"
            :style="listRevealStyle(0)"
            :disabled="saving"
            @pointerdown="down"
            @pointermove="move"
            @pointerup="origin = undefined"
            @pointercancel="origin = undefined"
            @dragstart.prevent
            @click="clickTile"
          >
            <Archive :size="32" /><span
              ><strong>{{ output.name }}</strong
              ><small
                >{{ (output.size / 1048576).toFixed(1) }} MB ·
                {{ t("拖到文件管理器，或点击保存") }}</small
              ></span
            ><Grip :size="17" />
          </button>
          <footer class="modal-actions">
            <button class="button" :disabled="locked" @click="save">
              <Download :size="16" />{{ t("保存 ZIP") }}
            </button>
          </footer>
        </template>
      </ContentSwitch>
    </section>
  </GlassDialog>
</template>
<style scoped>
:deep(.content-switch-page) {
  gap: 20px;
}
.game-export-panel {
  width: min(470px, calc(100vw - 36px));
  padding: 24px;
  display: grid;
  gap: 20px;
}
.modal-titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.dialog-heading {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 0;
}
.game-export-identity {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.game-export-identity > :first-child {
  width: 42px;
  height: 42px;
  flex: none;
}
.game-export-identity div {
  display: grid;
  gap: 5px;
  min-width: 0;
}
.game-export-identity strong {
  overflow-wrap: anywhere;
}
.game-export-identity small,
.game-export-file small {
  opacity: 0.65;
  font-size: 12px;
}
.game-export-options {
  display: flex;
  gap: 14px;
  align-items: center;
  justify-content: space-between;
}
.export-portable-toggle {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
}
.game-export-progress {
  display: grid;
  gap: 10px;
  font-size: 12px;
}
.game-export-progress span {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.game-export-progress progress {
  width: 100%;
  height: 4px;
  accent-color: var(--accent);
}
.game-export-file {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 20px 14px;
  background: transparent;
  border: 1px dashed currentColor;
  color: inherit;
  text-align: left;
  cursor: grab;
  touch-action: none;
  user-select: none;
}
.game-export-file > span {
  display: grid;
  gap: 8px;
  flex: 1;
  min-width: 0;
}
.game-export-file strong {
  overflow-wrap: anywhere;
}
.game-export-file:active {
  cursor: grabbing;
}
.modal-actions {
  margin: 0;
}
</style>
