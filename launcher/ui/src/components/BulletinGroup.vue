<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { motion } from "motion-v";
import type { BulletinItem } from "../types/launcher";
import BulletinEntry from "./BulletinEntry.vue";
import { useControlMotion } from "../composables/useControlMotion";
const props = defineProps<{ items: BulletinItem[] }>();
defineEmits<{ open: [url: string] }>();
const expanded = ref(false);
const moving = ref(false);
function toggle() {
  moving.value = true;
  expanded.value = !expanded.value;
}
const root = ref<HTMLElement | null>(null);
const heights = ref<number[]>([]);
const { surface, reduced } = useControlMotion();
const positions = computed(() => {
  let y = 0;
  return props.items.map((_, index) => {
    const height = heights.value[index] ?? 150;
    const position = { y, height };
    y += height + 12;
    return position;
  });
});
const height = computed(() =>
  expanded.value
    ? positions.value.reduce((sum, item) => sum + item.height + 12, -12)
    : (heights.value[0] ?? 150) + Math.min(props.items.length - 1, 2) * 10,
);
let observer: ResizeObserver;
function observe() {
  observer.disconnect();
  root
    .value!.querySelectorAll<HTMLElement>(".bulletin-stack__measure")
    .forEach((element) => {
      heights.value[Number(element.dataset.index)] = element.offsetHeight + 2;
      observer.observe(element);
    });
}
onMounted(() => {
  observer = new ResizeObserver((entries) => {
    for (const entry of entries) {
      const element = entry.target as HTMLElement;
      heights.value[Number(element.dataset.index)] = element.offsetHeight + 2;
    }
  });
  observe();
});
watch(
  () => props.items,
  async () => {
    await nextTick();
    observe();
  },
);
onBeforeUnmount(() => observer.disconnect());
</script>
<template>
  <section class="bulletin-group" :class="{ 'is-expanded': expanded }">
    <button
      class="bulletin-group__toggle"
      type="button"
      :aria-expanded="expanded"
      @click="toggle"
    >
      <span
        >{{ items[0].source
        }}<span class="bulletin-group__count">{{ items.length }}</span></span
      >
      <span
        >{{ expanded ? "收起" : "展开"
        }}<motion.svg
          class="icon"
          aria-hidden="true"
          :animate="{ rotate: expanded ? 180 : 0 }"
          :transition="moving ? surface : { duration: 0 }"
          ><use href="#i-caret-down" /></motion.svg
      ></span>
    </button>
    <div ref="root" class="bulletin-stack">
      <motion.div
        class="bulletin-stack__space"
        :initial="false"
        :animate="{ height }"
        :transition="moving ? surface : { duration: 0 }"
      />
      <motion.div
        v-for="(item, index) in items"
        :key="item.url"
        class="bulletin-stack__slot"
        :style="{ zIndex: items.length - index }"
        :inert="!expanded && index > 0"
        :aria-hidden="(!expanded && index > 0) || undefined"
        :initial="false"
        :animate="{
          y: expanded ? positions[index].y : Math.min(index, 2) * 10,
          x: expanded ? 0 : Math.min(index, 2) * 8,
          width: `calc(100% - ${expanded ? 0 : Math.min(index, 2) * 16}px)`,
          height: expanded ? positions[index].height : (heights[0] ?? 150),
          opacity: expanded || index < 3 ? 1 : 0,
        }"
        :transition="{
          ...(moving ? surface : { duration: 0 }),
          delay:
            !moving || reduced
              ? 0
              : expanded
                ? Math.min(index, 4) * 0.025
                : Math.min(items.length - index - 1, 4) * 0.018,
        }"
        @click.capture="
          (event) => {
            if (!expanded) {
              event.stopPropagation();
              toggle();
            }
          }
        "
      >
        <motion.div
          :initial="false"
          :animate="{ opacity: expanded || index === 0 ? 1 : 0 }"
          :transition="{
            duration: reduced ? 0 : 0.16,
            delay: reduced || !expanded ? 0 : 0.05 + Math.min(index, 4) * 0.025,
          }"
        >
          <div class="bulletin-stack__measure" :data-index="index">
            <BulletinEntry
              :item="item"
              :expand-label="
                !expanded && index === 0
                  ? `展开 ${items.length} 条公告`
                  : undefined
              "
              @open="$emit('open', $event)"
            />
          </div>
        </motion.div>
      </motion.div>
    </div>
  </section>
</template>
