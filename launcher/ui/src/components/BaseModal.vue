<script setup lang="ts">
import { nextTick, onBeforeUnmount, onUpdated, ref, useId, watch } from "vue";
import { useAnimate } from "motion-v";
import {
  useControlMotion,
  surfaceSpring,
} from "../composables/useControlMotion";
import BaseView from "./BaseView.vue";
import BaseButton from "./BaseButton.vue";
const [, animate] = useAnimate();
const { reduced } = useControlMotion();
let reveal = 0;
const props = defineProps<{
  open: boolean;
  title: string;
  theme?: string;
  reduceMotion?: boolean;
  viewKey?: string | number;
  direction?: 1 | -1;
}>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();
const titleId = useId();
const panel = ref<HTMLElement | null>(null);
const content = ref<HTMLElement | null>(null);
let resizeObserver: ResizeObserver | undefined;
let resizeAnimation: ReturnType<typeof animate> | undefined;
let bodyHeight = 0;
watch(
  content,
  (element) => {
    resizeObserver?.disconnect();
    resizeAnimation?.stop();
    bodyHeight = 0;
    if (!element) return;
    const body = element.parentElement!;
    resizeObserver = new ResizeObserver(() => {
      const style = getComputedStyle(body);
      const height =
        element.offsetHeight +
        Number.parseFloat(style.paddingTop) +
        Number.parseFloat(style.paddingBottom);
      if (Math.abs(height - bodyHeight) < 0.5) return;
      resizeAnimation?.stop();
      const previous = bodyHeight;
      bodyHeight = height;
      if (!previous || reducedMotion()) {
        body.style.height = `${height}px`;
        return;
      }
      resizeAnimation = animate(
        body,
        { height: [Number.parseFloat(body.style.height), height] },
        surfaceSpring,
      );
    });
    resizeObserver.observe(element);
  },
  { flush: "post" },
);
let animations: ReturnType<typeof animate>[] = [];
let animationRun = 0;
function stopMotion() {
  animationRun++;
  animations.forEach((animation) => animation.stop());
  animations = [];
}
function reducedMotion() {
  return props.reduceMotion || reduced.value;
}
function beforeEnter(element: Element) {
  if (reducedMotion()) return;
  element.querySelector<HTMLElement>(".modal__backdrop")!.style.opacity = "0";
  const surface = element.querySelector<HTMLElement>(".modal__panel")!;
  surface.style.opacity = "0";
  reveal = 0;
  surface.style.clipPath = "inset(42% 12% round 24px)";
}
function transition(element: Element, done: () => void, entering: boolean) {
  stopMotion();
  const surface = element.querySelector<HTMLElement>(".modal__panel")!;
  const backdrop = element.querySelector<HTMLElement>(".modal__backdrop")!;
  const heading = surface.querySelector<HTMLElement>(".modal__head")!;
  const body = surface.querySelector<HTMLElement>(".modal__body")!;
  if (reducedMotion()) {
    reveal = entering ? 1 : 0;
    surface.style.opacity = "";
    surface.style.clipPath = "";
    heading.style.opacity = body.style.opacity = "";
    heading.style.transform = body.style.transform = "";
    backdrop.style.opacity = "";
    done();
    return;
  }
  const run = animationRun;
  const start = reveal;
  animations = [
    animate(
      backdrop,
      {
        opacity: [Number(getComputedStyle(backdrop).opacity), entering ? 1 : 0],
      },
      {
        duration: entering ? 0.2 : 0.16,
      },
    ),
    animate(
      surface,
      {
        opacity: [Number(getComputedStyle(surface).opacity), entering ? 1 : 0],
      },
      {
        duration: entering ? 0.12 : 0.16,
      },
    ),
    animate(start, entering ? 1 : 0, {
      ...(entering
        ? surfaceSpring
        : { duration: 0.18, ease: [0.4, 0, 1, 1] as const }),
      onUpdate: (value) => {
        reveal = value;
        const progress = Math.max(0, Math.min(1, value));
        surface.style.clipPath = `inset(${(1 - progress) * 42}% ${(1 - progress) * 12}% round 24px)`;
        const titleProgress = Math.max(
          0,
          Math.min(1, (progress - 0.15) / 0.65),
        );
        const bodyProgress = Math.max(0, Math.min(1, (progress - 0.3) / 0.7));
        heading.style.opacity = String(titleProgress);
        body.style.opacity = String(bodyProgress);
        heading.style.transform = `translateX(${(1 - titleProgress) * 8}px)`;
        body.style.transform = `translateX(${(1 - bodyProgress) * 12}px)`;
      },
    }),
  ];
  Promise.all(animations).then(() => {
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
  resizeObserver?.disconnect();
  resizeAnimation?.stop();
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
  ).filter(
    (item) =>
      item.tabIndex >= 0 && !item.closest('[hidden], [aria-hidden="true"]'),
  );
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
  <Teleport to="#app" defer>
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
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        @keydown="keydown"
        :data-theme="theme"
        :inert="!open"
        :aria-hidden="!open || undefined"
      >
        <div class="modal__backdrop" @click="close"></div>
        <section ref="panel" class="modal__panel" tabindex="-1">
          <header class="modal__head">
            <h2 :id="titleId">{{ title }}</h2>
            <BaseButton
              variant="ghost"
              size="sm"
              icon="i-x"
              icon-only
              aria-label="关闭弹窗"
              @click="close"
            />
          </header>
          <div class="modal__body">
            <div ref="content" class="modal__content">
              <BaseView
                :view-key="viewKey ?? 0"
                :direction="direction"
                :reduce-motion="!!reduceMotion"
                ><slot
              /></BaseView>
            </div>
          </div>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>
