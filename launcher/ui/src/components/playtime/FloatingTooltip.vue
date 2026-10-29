<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { AnimatePresence, motion, useReducedMotion } from "motion-v";

interface TooltipPointer {
  x: number;
  y: number;
}

const props = defineProps<{
  open: boolean;
  pointer: TooltipPointer;
  theme?: string;
  reduceMotion?: boolean;
  name?: string;
}>();
const tip = ref<HTMLElement | null>(null);
const placed = ref(false);
const prefersReduced = useReducedMotion();
const reduced = computed(() => props.reduceMotion || prefersReduced.value);
// 尺寸只在提示出现或内容变化时测量, 指针移动时仅改写合成层位移。
const size = { width: 0, height: 0 };
let pendingX = 0;
let pendingY = 0;
let placeFrame = 0;
let moveFrame = 0;
let attempts = 0;
let observer: ResizeObserver | undefined;
let observed: HTMLElement | null = null;

function tipElement() {
  const value = tip.value as unknown as
    { $el?: HTMLElement | null } | HTMLElement | null;
  if (!value) return null;
  return value instanceof HTMLElement ? value : (value.$el ?? null);
}

function measure(element: HTMLElement) {
  const box = element.getBoundingClientRect();
  if (box.width <= 0) return false;
  size.width = box.width;
  size.height = box.height;
  return true;
}

function apply(element: HTMLElement) {
  const left = Math.max(
    12,
    Math.min(pendingX - size.width / 2, window.innerWidth - size.width - 12),
  );
  const above = pendingY - size.height - 10;
  const top =
    above >= 12
      ? above
      : Math.min(pendingY + 16, window.innerHeight - size.height - 12);
  element.style.transform = `translate3d(${left}px, ${top}px, 0)`;
  if (placed.value) return;
  element.style.visibility = "visible";
  placed.value = true;
}

async function place() {
  if (!props.open) return;
  await nextTick();
  const element = tipElement();
  if (!element) return;
  pendingX = props.pointer.x;
  pendingY = props.pointer.y;
  if (!measure(element)) {
    if (attempts++ < 4) schedule();
    return;
  }
  attempts = 0;
  observe(element);
  apply(element);
}

function observe(element: HTMLElement) {
  if (typeof ResizeObserver === "undefined") return;
  if (observer && observed === element) return;
  observer?.disconnect();
  observed = element;
  observer = new ResizeObserver(() => {
    if (!props.open) return;
    // 提示可能重新挂载, 以当前元素为准。
    const current = tipElement();
    if (current && measure(current)) apply(current);
  });
  observer.observe(element);
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
  () => [props.pointer.x, props.pointer.y],
  () => {
    if (!props.open) return;
    pendingX = props.pointer.x;
    pendingY = props.pointer.y;
    if (moveFrame) return;
    moveFrame = requestAnimationFrame(() => {
      moveFrame = 0;
      const element = tipElement();
      if (!element) return;
      if (size.width <= 0 && !measure(element)) {
        attempts = 0;
        schedule();
        return;
      }
      apply(element);
    });
  },
  { flush: "post" },
);
watch(
  tip,
  (element) => {
    if (!element) {
      observer?.disconnect();
      observer = undefined;
      observed = null;
      return;
    }
    if (props.open) {
      attempts = 0;
      schedule();
    }
  },
  { flush: "post" },
);
onBeforeUnmount(() => {
  if (placeFrame) cancelAnimationFrame(placeFrame);
  if (moveFrame) cancelAnimationFrame(moveFrame);
  observer?.disconnect();
  observer = undefined;
  observed = null;
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
        :style="{ visibility: placed ? 'visible' : 'hidden' }"
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
