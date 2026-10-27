<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from "vue";
import { UsersRound } from "lucide-vue-next";
import { loadSkinImage, skinAvatar } from "../previewSkin";
import type { Account } from "../types";

const props = defineProps<{
  account: Account;
  preview?: string;
  loadTexture: (id: string) => Promise<string | null>;
}>();
const image = ref("");
let generation = 0;
async function refresh() {
  const request = ++generation;
  if (props.preview) {
    image.value = props.preview;
    return;
  }
  try {
    const source = props.account.skin
      ? await props.loadTexture(props.account.id)
      : null;
    const texture = await loadSkinImage(source);
    if (request === generation)
      image.value = skinAvatar(texture, texture.width);
  } catch {
    try {
      const texture = await loadSkinImage(null);
      if (request === generation)
        image.value = skinAvatar(texture, texture.width);
    } catch {
      // Keep the account selectable even if its thumbnail cannot be decoded.
    }
  }
}
watch(
  () => [props.account.id, props.account.skin?.url, props.preview].join("|"),
  refresh,
  { immediate: true },
);
onBeforeUnmount(() => {
  generation++;
});
</script>

<template>
  <img
    v-if="image"
    class="account-face"
    :src="image"
    alt=""
    :draggable="false"
  />
  <UsersRound v-else :size="24" aria-hidden="true" />
</template>
