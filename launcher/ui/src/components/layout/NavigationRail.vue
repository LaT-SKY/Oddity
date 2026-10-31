<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { motion } from "motion-v";
import { state, showPage } from "../../core/controller";
import { navigationScrubbing } from "../../composables/navigationGesture";

const items = [
  { page: "home", label: "启动", icon: "play" },
  { page: "update", label: "更新", icon: "download-simple" },
  { page: "game-mode", label: "游戏模式", icon: "game-controller" },
  { page: "bulletin", label: "公告", icon: "newspaper-clipping" },
  { page: "records", label: "记录", icon: "clock" },
  { page: "settings", label: "设置", icon: "gear-six" },
] as const;
const itemWidth = 40;
const itemGap = 8;
const activeIndex = computed(() =>
  items.findIndex((item) => item.page === state.page),
);
const hoveredPage = ref<string | null>(null);
const pressedPage = ref<string | null>(null);
const nav = ref<HTMLElement | null>(null);
let gesture: { id: number; x: number; y: number; page: string } | undefined;
let suppressClick = false;
let finishing = 0;
function buttons() {
  return [
    ...(nav.value?.querySelectorAll<HTMLButtonElement>("[data-page]") ?? []),
  ];
}
function inCorridor(y: number) {
  const box = nav.value!.getBoundingClientRect();
  return y >= box.top - 24 && y <= box.bottom + 24;
}
function select(x: number) {
  const targets = buttons().map((button) => {
    const box = button.getBoundingClientRect();
    return { button, center: box.left + box.width / 2 };
  });
  const nearest = targets.reduce((best, entry) =>
    Math.abs(entry.center - x) < Math.abs(best.center - x) ? entry : best,
  ).button;
  const active = targets.find(
    (entry) => entry.button.dataset.page === state.page,
  );
  if (active && active.button !== nearest) {
    const midpoint =
      (active.center +
        targets.find((entry) => entry.button === nearest)!.center) /
      2;
    if (Math.abs(x - midpoint) < 3) return;
  }
  if (nearest.dataset.page !== state.page) showPage(nearest.dataset.page!);
}
function down(event: PointerEvent) {
  if (
    !event.isPrimary ||
    event.button !== 0 ||
    gesture ||
    !(event.target instanceof Element) ||
    !event.target.closest("[data-page]")
  )
    return;
  finishing++;
  suppressClick = false;
  gesture = {
    id: event.pointerId,
    x: event.clientX,
    y: event.clientY,
    page: state.page,
  };
}
function move(event: PointerEvent) {
  if (!gesture || event.pointerId !== gesture.id) return;
  const dx = event.clientX - gesture.x;
  const dy = event.clientY - gesture.y;
  if (!navigationScrubbing.value) {
    if (Math.abs(dy) > 8 && Math.abs(dy) > Math.abs(dx)) {
      void finish(true);
      return;
    }
    if (Math.abs(dx) < 6) return;
    navigationScrubbing.value = true;
    suppressClick = true;
    nav.value?.setPointerCapture(event.pointerId);
  }
  if (inCorridor(event.clientY)) select(event.clientX);
}
async function finish(cancelled: boolean) {
  if (!gesture) return;
  const token = ++finishing;
  const previous = gesture;
  const wasScrubbing = navigationScrubbing.value;
  gesture = undefined;
  if (cancelled) {
    suppressClick = true;
    if (wasScrubbing) showPage(previous.page);
  }
  if (nav.value?.hasPointerCapture(previous.id))
    nav.value.releasePointerCapture(previous.id);
  await nextTick();
  if (token !== finishing) return;
  navigationScrubbing.value = false;
  if (wasScrubbing)
    buttons()
      .find((button) => button.dataset.page === state.page)
      ?.focus({ preventScroll: true });
}
function up(event: PointerEvent) {
  if (!gesture || event.pointerId !== gesture.id) return;
  if (navigationScrubbing.value && inCorridor(event.clientY))
    select(event.clientX);
  void finish(!inCorridor(event.clientY));
}
function cancel(event?: PointerEvent) {
  if (gesture && (!event || event.pointerId === gesture.id)) void finish(true);
}
function lost(event: PointerEvent) {
  if (event.target === nav.value) cancel(event);
}
function click(event: MouseEvent, page: string) {
  if (suppressClick && event.detail !== 0) {
    event.preventDefault();
    return;
  }
  showPage(page);
}
function key(event: KeyboardEvent) {
  if (event.key === "Escape" && gesture) {
    event.preventDefault();
    event.stopPropagation();
    cancel();
    return;
  }
  if (!(event.target instanceof Element) || !nav.value?.contains(event.target))
    return;
  const current = buttons().findIndex(
    (button) => button === document.activeElement,
  );
  const index =
    event.key === "ArrowRight"
      ? (current + 1) % items.length
      : event.key === "ArrowLeft"
        ? (current - 1 + items.length) % items.length
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? items.length - 1
            : -1;
  if (index < 0) return;
  event.preventDefault();
  if (gesture) cancel();
  navigationScrubbing.value = true;
  showPage(items[index].page);
  void nextTick(() => {
    navigationScrubbing.value = false;
    buttons()[index]?.focus({ preventScroll: true });
  });
}
function blur() {
  hoveredPage.value = null;
  pressedPage.value = null;
  cancel();
}
onMounted(() => {
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("blur", blur);
  window.addEventListener("keydown", key, true);
});
onBeforeUnmount(() => {
  finishing++;
  gesture = undefined;
  navigationScrubbing.value = false;
  window.removeEventListener("pointermove", move);
  window.removeEventListener("pointerup", up);
  window.removeEventListener("pointercancel", cancel);
  window.removeEventListener("blur", blur);
  window.removeEventListener("keydown", key, true);
});
</script>

<template>
  <aside class="rail">
    <nav
      ref="nav"
      class="rail__nav"
      aria-label="主导航"
      :data-scrubbing="navigationScrubbing"
      @pointerdown="down"
      @lostpointercapture="lost"
      @mousedown.stop
      @dragstart.prevent
      :style="{
        '--rail-item-width': `${itemWidth}px`,
        '--rail-item-gap': `${itemGap}px`,
      }"
    >
      <motion.span
        class="nav-focus"
        aria-hidden="true"
        :initial="false"
        :animate="{
          x: activeIndex * (itemWidth + itemGap),
          y: hoveredPage === state.page ? -2 : 0,
          scale:
            pressedPage === state.page ? 0.88 : navigationScrubbing ? 1.09 : 1,
        }"
        :transition="{ type: 'spring', stiffness: 420, damping: 32 }"
      />
      <motion.button
        v-for="item in items"
        :key="item.page"
        v-memo="[state.page === item.page, navigationScrubbing]"
        type="button"
        :while-hover="{ y: -2 }"
        :while-press="{ scale: 0.88 }"
        :animate="{
          scale: navigationScrubbing && state.page === item.page ? 1.09 : 1,
        }"
        :transition="{ type: 'spring', stiffness: 460, damping: 25 }"
        class="rail-item"
        :data-page="item.page"
        :aria-label="item.label"
        :title="navigationScrubbing ? undefined : item.label"
        :class="{ 'is-active': state.page === item.page }"
        :aria-current="state.page === item.page ? 'page' : undefined"
        @click="click($event, item.page)"
        @pointerenter="
          (event) => {
            if (event.pointerType === 'mouse') hoveredPage = item.page;
          }
        "
        @pointerleave="
          hoveredPage = null;
          pressedPage = null;
        "
        @pointerdown="pressedPage = item.page"
        @pointerup="pressedPage = null"
        @pointercancel="pressedPage = null"
        @keydown="
          (event) => {
            if (event.key === 'Enter' || event.key === ' ')
              pressedPage = item.page;
          }
        "
        @keyup="pressedPage = null"
        @blur="pressedPage = null"
      >
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use :href="`#i-${item.icon}`" />
        </svg>
      </motion.button>
    </nav>
  </aside>
</template>
