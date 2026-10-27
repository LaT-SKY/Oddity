<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { motion, useAnimate, useMotionValue, useTransform } from "motion-v";
import { dismissNotification, pauseNotifications } from "../core/notifications";

const props = defineProps<{
  id: number;
  revision: number;
  enabled: boolean;
  leaving: boolean;
  reduced: boolean;
}>();
const root = ref<HTMLElement | null>(null);
const x = useMotionValue(0);
const opacity = useTransform(x, [-360, -40, 0, 40, 360], [0, 1, 1, 1, 0]);
const [, animate] = useAnimate();
const dragging = ref(false);
let gesture:
  | {
      id: number;
      startX: number;
      startY: number;
      lastX: number;
      time: number;
      velocity: number;
    }
  | undefined;
let animation: { stop: () => void } | undefined;
let run = 0;
let suppressClick = false;
const pause = (value: boolean) =>
  pauseNotifications(`swipe:${props.id}`, value);

function release() {
  const id = gesture?.id;
  gesture = undefined;
  dragging.value = false;
  if (id !== undefined && root.value?.hasPointerCapture(id))
    root.value.releasePointerCapture(id);
}
async function settle(dismiss = false) {
  const token = ++run;
  animation?.stop();
  release();
  const box = root.value?.getBoundingClientRect();
  const target =
    dismiss && box
      ? x.get() +
        (x.get() < 0 ? -box.right - 24 : window.innerWidth - box.left + 24)
      : 0;
  const playback = animate(
    x,
    target,
    props.reduced
      ? { duration: 0 }
      : dismiss
        ? { duration: 0.18, ease: [0.3, 0, 0.8, 1] }
        : { type: "spring", stiffness: 480, damping: 34, velocity: 0 },
  );
  animation = playback;
  await playback;
  if (token !== run) return;
  if (dismiss) dismissNotification(props.id, true);
  pause(false);
}
function down(event: PointerEvent) {
  if (event.button === 0 && event.isPrimary) suppressClick = false;
  if (
    !props.enabled ||
    event.button !== 0 ||
    !event.isPrimary ||
    gesture ||
    (event.target instanceof Element &&
      event.target.closest("button, a, input, select, textarea"))
  )
    return;
  animation?.stop();
  run++;
  suppressClick = false;
  gesture = {
    id: event.pointerId,
    startX: event.clientX - x.get(),
    startY: event.clientY,
    lastX: event.clientX,
    time: performance.now(),
    velocity: 0,
  };
  pause(true);
}
function move(event: PointerEvent) {
  if (!gesture || event.pointerId !== gesture.id) return;
  const dx = event.clientX - gesture.startX;
  const dy = event.clientY - gesture.startY;
  if (!dragging.value) {
    if (Math.abs(dy) > 7 && Math.abs(dy) > Math.abs(dx)) {
      void settle();
      return;
    }
    if (Math.abs(dx) < 7 || Math.abs(dx) < Math.abs(dy) * 1.2) return;
    dragging.value = true;
    suppressClick = true;
    root.value?.setPointerCapture(event.pointerId);
  }
  const now = performance.now();
  const dt = now - gesture.time;
  if (dt > 0) gesture.velocity = ((event.clientX - gesture.lastX) / dt) * 1000;
  gesture.lastX = event.clientX;
  gesture.time = now;
  x.set(dx);
}
function up(event: PointerEvent) {
  if (!gesture || event.pointerId !== gesture.id) return;
  const distance = x.get();
  const velocity = performance.now() - gesture.time < 90 ? gesture.velocity : 0;
  const threshold = Math.min(100, (root.value?.offsetWidth ?? 352) * 0.28);
  const dismiss =
    dragging.value &&
    (Math.abs(distance) >= threshold ||
      (Math.abs(distance) > 20 &&
        Math.abs(velocity) > 650 &&
        distance * velocity > 0));
  void settle(dismiss);
}
function cancel(event?: PointerEvent) {
  if (!gesture || (event && event.pointerId !== gesture.id)) return;
  void settle();
}
function lost(event: PointerEvent) {
  if (event.target === root.value) cancel(event);
}
function click(event: MouseEvent) {
  if (!suppressClick || event.detail === 0) return;
  event.preventDefault();
  event.stopPropagation();
  suppressClick = false;
}
watch(
  () => [props.enabled, props.revision],
  () => {
    if (!props.leaving) void settle();
    else {
      run++;
      animation?.stop();
      release();
      pause(false);
    }
  },
);
function blur() {
  cancel();
}
onMounted(() => {
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", cancel);
  window.addEventListener("blur", blur);
});
onBeforeUnmount(() => {
  run++;
  animation?.stop();
  release();
  pause(false);
  window.removeEventListener("pointermove", move);
  window.removeEventListener("pointerup", up);
  window.removeEventListener("pointercancel", cancel);
  window.removeEventListener("blur", blur);
});
</script>

<template>
  <div
    ref="root"
    class="notification-swipe"
    @pointerdown="down"
    @lostpointercapture="lost"
    @click.capture="click"
  >
    <motion.div :style="{ x, opacity }" :data-swiping="dragging">
      <slot />
    </motion.div>
  </div>
</template>
