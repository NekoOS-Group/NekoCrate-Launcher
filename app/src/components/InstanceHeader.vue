<script setup lang="ts">
import { t } from "../i18n";
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { FolderOpen } from "lucide-vue-next";
import { instanceIcons } from "../instanceIcons";
import InstanceIcon from "./InstanceIcon.vue";

const props = defineProps<{
  name: string;
  icon: number;
  directory: string;
  directoryLoading: boolean;
  disabled: boolean;
}>();
const emit = defineEmits<{
  "update:name": [name: string];
  "update:icon": [icon: number];
  openDirectory: [];
}>();
const renaming = ref(false);
const draftName = ref("");
const nameInput = ref<HTMLInputElement>();
const nameButton = ref<HTMLButtonElement>();
const iconControl = ref<HTMLElement>();
const iconButton = ref<HTMLButtonElement>();
const choosingIcon = ref(false);

async function rename() {
  choosingIcon.value = false;
  draftName.value = props.name;
  renaming.value = true;
  await nextTick();
  nameInput.value?.focus();
  nameInput.value?.select();
}
function finishRename(restoreFocus = false, reportError = true): boolean {
  if (!renaming.value) return true;
  const name = draftName.value.trim();
  const valid = name.length > 0 && [...name].length <= 80;
  nameInput.value?.setCustomValidity(
    valid ? "" : t("实例名称应为 1–80 个字符"),
  );
  if (!valid) {
    if (reportError) nameInput.value?.reportValidity();
    return false;
  }
  emit("update:name", name);
  renaming.value = false;
  if (restoreFocus) void nextTick(() => nameButton.value?.focus());
  return true;
}
function cancelRename() {
  renaming.value = false;
  void nextTick(() => nameButton.value?.focus());
}
function closeIcons(restoreFocus = false) {
  choosingIcon.value = false;
  if (restoreFocus) iconButton.value?.focus({ preventScroll: true });
}
function chooseIcon(icon: number, event: MouseEvent) {
  emit("update:icon", icon);
  closeIcons(event.detail === 0);
}
function enterChoices(element: Element) {
  element.removeAttribute("inert");
  element.removeAttribute("aria-hidden");
}
function leaveChoices(element: Element) {
  element.setAttribute("inert", "");
  element.setAttribute("aria-hidden", "true");
}
function outside(event: PointerEvent) {
  if (
    event.target instanceof Node &&
    !iconControl.value?.contains(event.target)
  )
    closeIcons();
}
function leaveIcons(event: FocusEvent) {
  if (
    !(event.relatedTarget instanceof Node) ||
    !iconControl.value?.contains(event.relatedTarget)
  )
    closeIcons();
}
onMounted(() => document.addEventListener("pointerdown", outside, true));
onBeforeUnmount(() =>
  document.removeEventListener("pointerdown", outside, true),
);
defineExpose({ finishRename });
</script>

<template>
  <div class="settings-heading dialog-heading" tabindex="-1" autofocus>
    <div
      ref="iconControl"
      class="instance-icon-control"
      @focusout="leaveIcons"
      @keydown.esc.stop.prevent="closeIcons(true)"
    >
      <button
        ref="iconButton"
        class="instance-heading-icon"
        :disabled="disabled"
        :aria-label="t('切换实例图标')"
        :title="t('切换实例图标')"
        :aria-expanded="choosingIcon"
        aria-controls="instance-icon-choices"
        @pointerdown.prevent
        @click="choosingIcon = !choosingIcon"
      >
        <Transition name="instance-icon-swap">
          <span :key="icon" class="instance-heading-art">
            <InstanceIcon :icon="icon" />
          </span>
        </Transition>
      </button>
      <Transition
        name="icon-picker"
        :duration="{ enter: 320, leave: 180 }"
        @before-enter="enterChoices"
        @before-leave="leaveChoices"
      >
        <div
          v-if="choosingIcon"
          id="instance-icon-choices"
          class="instance-icon-choices glass-surface"
          role="group"
          :aria-label="t('实例图标')"
        >
          <button
            v-for="(material, index) in instanceIcons"
            :key="material.id"
            class="instance-tile"
            :class="{ selected: icon === material.id }"
            :style="{
              '--icon-delay': `${(Math.floor(index / 4) + (index % 4)) * 12}ms`,
            }"
            :disabled="disabled"
            :aria-label="t(material.name)"
            :title="t(material.name)"
            :aria-pressed="icon === material.id"
            @pointerdown.prevent
            @click="chooseIcon(material.id, $event)"
          >
            <InstanceIcon :icon="material.id" />
          </button>
        </div>
      </Transition>
    </div>
    <div class="settings-heading-details">
      <h2>
        <input
          v-if="renaming"
          ref="nameInput"
          v-model="draftName"
          class="instance-name-input"
          :aria-label="t('实例名称')"
          :disabled="disabled"
          maxlength="80"
          autocomplete="off"
          @input="nameInput?.setCustomValidity('')"
          @blur="finishRename(false, false)"
          @keydown.enter.stop.prevent="finishRename(true)"
          @keydown.esc.stop.prevent="cancelRename"
        />
        <button
          v-else
          ref="nameButton"
          class="instance-name-button"
          :disabled="disabled"
          :title="t('{name} · 点击修改名称', { name })"
          :aria-label="t('修改实例名称')"
          @click="rename"
        >
          {{ name }}
        </button>
      </h2>
      <button
        class="settings-directory-link"
        :disabled="!directory"
        :title="directory"
        :aria-label="
          t('打开游戏目录：{path}', { path: directory || t('读取中') })
        "
        @click="emit('openDirectory')"
      >
        <FolderOpen :size="13" /><span>{{
          directory || (directoryLoading ? t("读取目录…") : t("目录不可用"))
        }}</span>
      </button>
    </div>
  </div>
</template>
