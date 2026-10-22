<script setup lang="ts">
import { nextTick, onBeforeUnmount, onUpdated, ref, useId, watch } from "vue";
import { animateSingleValue } from "motion-dom";
const props = defineProps<{
  open: boolean;
  title: string;
  theme?: string;
  reduceMotion?: boolean;
}>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();
const titleId = useId();
const panel = ref<HTMLElement | null>(null);
let animations: ReturnType<typeof animateSingleValue>[] = [];
let animationRun = 0;
function stopMotion() {
  animationRun++;
  animations.forEach((animation) => animation.stop());
  animations = [];
}
function reducedMotion() {
  return (
    props.reduceMotion ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}
function beforeEnter(element: Element) {
  if (reducedMotion()) return;
  element.querySelector<HTMLElement>(".modal__backdrop")!.style.opacity = "0";
  const surface = element.querySelector<HTMLElement>(".modal__panel")!;
  surface.style.opacity = "0";
  surface.style.transform = "translateY(18px) scale(0.97)";
}
function transition(element: Element, done: () => void, entering: boolean) {
  stopMotion();
  const surface = element.querySelector<HTMLElement>(".modal__panel")!;
  const backdrop = element.querySelector<HTMLElement>(".modal__backdrop")!;
  if (reducedMotion()) {
    surface.style.opacity = "";
    surface.style.transform = "";
    backdrop.style.opacity = "";
    done();
    return;
  }
  const run = animationRun;
  const start = new DOMMatrixReadOnly(getComputedStyle(surface).transform);
  const targetY = entering ? 0 : 8;
  const targetScale = entering ? 1 : 0.985;
  // 逐帧写入最终样式, 避免 WebKit 在原生动画取消交接时闪回旧遮罩。
  animations = [
    animateSingleValue(
      Number(getComputedStyle(backdrop).opacity),
      entering ? 1 : 0,
      {
        duration: entering ? 0.2 : 0.16,
        ease: "easeOut",
        onUpdate: (value) => {
          backdrop.style.opacity = String(value);
        },
      },
    ),
    animateSingleValue(
      Number(getComputedStyle(surface).opacity),
      entering ? 1 : 0,
      {
        duration: entering ? 0.16 : 0.12,
        ease: "easeOut",
        onUpdate: (value) => {
          surface.style.opacity = String(value);
        },
      },
    ),
    animateSingleValue(0, 1, {
      ...(entering
        ? { type: "spring" as const, stiffness: 430, damping: 32, mass: 0.95 }
        : { duration: 0.16, ease: [0.4, 0, 1, 1] as const }),
      onUpdate: (value) => {
        const y = start.m42 + (targetY - start.m42) * value;
        const scale = start.a + (targetScale - start.a) * value;
        surface.style.transform = `translateY(${y}px) scale(${scale})`;
      },
    }),
  ];
  Promise.all(animations.map((animation) => animation.finished)).then(() => {
    if (run === animationRun) {
      animations = [];
      done();
    }
  });
}
function enter(element: Element, done: () => void) {
  transition(element, done, true);
}
function leave(element: Element, done: () => void) {
  const root = element as HTMLElement;
  root.inert = true;
  root.style.pointerEvents = "none";
  restore();
  root.setAttribute("aria-hidden", "true");
  transition(element, done, false);
}
let previous: HTMLElement | null = null;
function close() {
  emit("update:open", false);
}
function restore() {
  if (previous?.isConnected) previous.focus();
  previous = null;
}
watch(
  () => props.open,
  async (open) => {
    if (open) {
      previous = document.activeElement as HTMLElement | null;
      await nextTick();
      if (props.open)
        panel.value?.querySelector<HTMLElement>("button")?.focus();
    } else {
      await nextTick();
      restore();
    }
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  stopMotion();
  restore();
});
// 切换表单或移除账号后, 将焦点留在弹窗内。
onUpdated(() => {
  if (
    props.open &&
    panel.value &&
    !panel.value.contains(document.activeElement)
  ) {
    panel.value.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
  }
});
function keydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    close();
  }
  if (event.key !== "Tab") return;
  const items = Array.from(
    panel.value?.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]',
    ) ?? [],
  ).filter((item) => !item.closest('[hidden], [aria-hidden="true"]'));
  const first = items[0],
    last = items.at(-1);
  if (!first) {
    event.preventDefault();
    panel.value?.focus();
  } else if (
    event.shiftKey &&
    (document.activeElement === first ||
      !items.includes(document.activeElement as HTMLElement))
  ) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
</script>
<template>
  <Teleport to="body">
    <Transition
      :css="false"
      @before-enter="beforeEnter"
      @enter="enter"
      @leave="leave"
      @enter-cancelled="stopMotion"
      @leave-cancelled="stopMotion"
    >
      <div
        v-if="open"
        class="modal"
        :data-theme="theme"
        :inert="!open"
        :aria-hidden="!open || undefined"
      >
        <div class="modal__backdrop" @click="close"></div>
        <section
          ref="panel"
          class="modal__panel"
          role="dialog"
          aria-modal="true"
          :aria-labelledby="titleId"
          tabindex="-1"
          @keydown="keydown"
        >
          <header class="modal__head">
            <h2 :id="titleId">{{ title }}</h2>
            <button
              type="button"
              class="btn btn--link"
              aria-label="关闭弹窗"
              @click="close"
            >
              <svg class="icon" aria-hidden="true"><use href="#i-x" /></svg>
            </button>
          </header>
          <div class="modal__body"><slot /></div>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>
