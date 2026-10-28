<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { AnimatePresence, motion, useReducedMotion } from "motion-v";

const props = defineProps<{
  open: boolean;
  x: number;
  y: number;
  theme?: string;
  reduceMotion?: boolean;
  name?: string;
}>();
const tip = ref<HTMLElement | null>(null);
const position = ref({ left: "0px", top: "0px" });
const placed = ref(false);
const prefersReduced = useReducedMotion();
const reduced = computed(() => props.reduceMotion || prefersReduced.value);
let placeFrame = 0;
let attempts = 0;

function tipElement() {
  const value = tip.value as unknown as
    { $el?: HTMLElement | null } | HTMLElement | null;
  if (!value) return null;
  return value instanceof HTMLElement ? value : (value.$el ?? null);
}

async function place() {
  if (!props.open) return;
  await nextTick();
  const element = tipElement();
  const box = element?.getBoundingClientRect();
  if (!box || box.width <= 0) {
    if (attempts++ < 4) schedule();
    return;
  }
  attempts = 0;
  const left = Math.max(
    12,
    Math.min(props.x - box.width / 2, window.innerWidth - box.width - 12),
  );
  const above = props.y - box.height - 10;
  const top =
    above >= 12
      ? above
      : Math.min(props.y + 16, window.innerHeight - box.height - 12);
  position.value = { left: `${left}px`, top: `${top}px` };
  placed.value = true;
}
function schedule() {
  if (placeFrame) cancelAnimationFrame(placeFrame);
  placeFrame = requestAnimationFrame(() => {
    placeFrame = 0;
    void place();
  });
}
watch(
  () => props.open,
  (open) => {
    if (!open) return;
    placed.value = false;
    attempts = 0;
    schedule();
  },
  { flush: "post" },
);
watch(
  () => [props.x, props.y],
  () => {
    if (!props.open) return;
    attempts = 0;
    schedule();
  },
  { flush: "post" },
);
watch(
  tip,
  (element) => {
    if (element && props.open) {
      attempts = 0;
      schedule();
    }
  },
  { flush: "post" },
);
onBeforeUnmount(() => {
  if (placeFrame) cancelAnimationFrame(placeFrame);
});
</script>

<template>
  <Teleport to="body">
    <AnimatePresence>
      <motion.aside
        v-if="open"
        ref="tip"
        role="tooltip"
        class="task-tooltip playtime-tooltip"
        :data-theme="theme"
        :data-tooltip="name"
        :style="{
          ...position,
          visibility: placed ? 'visible' : 'hidden',
        }"
        :initial="{ opacity: 0 }"
        :animate="{ opacity: 1 }"
        :exit="{ opacity: 0 }"
        :transition="reduced ? { duration: 0 } : { duration: 0.1 }"
      >
        <slot />
      </motion.aside>
    </AnimatePresence>
  </Teleport>
</template>
