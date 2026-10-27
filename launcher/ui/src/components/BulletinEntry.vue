<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { motion } from "motion-v";
import type { BulletinItem } from "../types/launcher";
import BulletinMedia from "./BulletinMedia.vue";
import { useControlMotion } from "../composables/useControlMotion";
const props = defineProps<{ item: BulletinItem; expandLabel?: string }>();
defineEmits<{ open: [url: string] }>();
const imageFailed = ref(false);
const publishedAt = computed(() => {
  const value = props.item.publishedAt;
  if (props.item.kind !== "x") return value;
  const date = Date.parse(value);
  return Number.isNaN(date)
    ? value
    : new Intl.DateTimeFormat("zh-CN").format(date);
});
const { control } = useControlMotion();
watch(
  () => props.item.imageUrl,
  () => {
    imageFailed.value = false;
  },
);
</script>
<template>
  <motion.button
    class="bulletin-entry"
    :class="{
      'has-image': item.imageUrl && !imageFailed,
    }"
    type="button"
    :aria-label="expandLabel"
    :while-hover="{ backgroundColor: 'var(--hover)' }"
    :transition="control"
    @click="$emit('open', item.url)"
  >
    <BulletinMedia
      v-if="item.imageUrl && !imageFailed"
      :key="item.imageUrl"
      :src="item.imageUrl"
      @error="imageFailed = true"
    />
    <span class="bulletin-entry__copy">
      <span class="bulletin-entry__meta"
        ><span>{{ item.source }}</span
        ><time v-if="publishedAt">{{ publishedAt }}</time></span
      >
      <strong>{{ item.title }}</strong>
      <span
        v-if="item.summary && item.summary !== item.title"
        class="bulletin-entry__summary"
        >{{ item.summary }}</span
      >
      <span class="bulletin-entry__open"
        >{{ expandLabel || "查看原文" }}
        <svg class="icon" aria-hidden="true">
          <use href="#i-arrow-up-right" /></svg
      ></span>
    </span>
  </motion.button>
</template>
