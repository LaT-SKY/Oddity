<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId } from "vue";
defineProps<{
  label: string;
  icon: string;
  description: string;
  theme: string;
}>();
const id = useId();
const trigger = ref<HTMLElement | null>(null);
const tooltip = ref<HTMLElement | null>(null);
const open = ref(false);
const position = ref({ left: "0px", top: "0px" });
let hideTimer: ReturnType<typeof setTimeout> | undefined;
function place() {
  if (!trigger.value || !tooltip.value) return;
  const anchor = trigger.value.getBoundingClientRect();
  const box = tooltip.value.getBoundingClientRect();
  const left = Math.max(
    12,
    Math.min(anchor.left, window.innerWidth - box.width - 12),
  );
  const below = anchor.bottom + 8;
  const top =
    below + box.height <= window.innerHeight - 12
      ? below
      : Math.max(12, anchor.top - box.height - 8);
  position.value = { left: `${left}px`, top: `${top}px` };
}
async function show() {
  clearTimeout(hideTimer);
  if (open.value) return;
  open.value = true;
  await nextTick();
  if (!open.value) return;
  place();
  window.addEventListener("resize", hide);
  window.addEventListener("scroll", hide, true);
}
function hide() {
  clearTimeout(hideTimer);
  open.value = false;
  window.removeEventListener("resize", hide);
  window.removeEventListener("scroll", hide, true);
}
function leave() {
  if (document.activeElement === trigger.value) return;
  hideTimer = setTimeout(hide, 100);
}
function escape(event: KeyboardEvent) {
  if (!open.value) return;
  event.preventDefault();
  event.stopPropagation();
  hide();
}
onBeforeUnmount(hide);
</script>
<template>
  <button
    ref="trigger"
    type="button"
    class="task-capsule"
    :aria-describedby="open ? id : undefined"
    @mouseenter="show"
    @mouseleave="leave"
    @focus="show"
    @blur="hide"
    @click="show"
    @keydown.esc="escape"
  >
    <svg class="icon" aria-hidden="true"><use :href="`#${icon}`" /></svg>
    <span>{{ label }}</span>
  </button>
  <Teleport to="body">
    <aside
      v-if="open"
      :id="id"
      ref="tooltip"
      role="tooltip"
      class="task-tooltip"
      :data-theme="theme"
      :style="position"
      @mouseenter="show"
      @mouseleave="leave"
    >
      <strong>{{ label }}</strong>
      <p>{{ description }}</p>
    </aside>
  </Teleport>
</template>
