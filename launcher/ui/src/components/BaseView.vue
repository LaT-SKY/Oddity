<script setup lang="ts">
import { onBeforeUnmount } from "vue";
import { useAnimate } from "motion-v";
import {
  useControlMotion,
  surfaceSpring,
} from "../composables/useControlMotion";
const props = withDefaults(
  defineProps<{
    viewKey: string | number;
    direction?: 1 | -1;
    reduceMotion?: boolean;
  }>(),
  { direction: 1 },
);
const { reduced } = useControlMotion();
const [, animate] = useAnimate();
const animations = new Map<Element, ReturnType<typeof animate>>();
function cancel(element: Element) {
  animations.get(element)?.stop();
  animations.delete(element);
}
function beforeEnter(element: Element) {
  const view = element as HTMLFieldSetElement;
  view.style.opacity = props.reduceMotion || reduced.value ? "1" : "0";
  view.style.transform = `translateX(${props.reduceMotion || reduced.value ? 0 : props.direction * 18}px)`;
}
function transition(element: Element, done: () => void, entering: boolean) {
  cancel(element);
  const view = element as HTMLFieldSetElement;
  if (!entering) {
    view.style.width = `${view.offsetWidth}px`;
    view.style.position = "absolute";
    view.style.top = "0";
    view.inert = true;
    view.disabled = true;
    view.setAttribute("aria-hidden", "true");
    // 离场视图不再占用新表单的控件标识。
    view
      .querySelectorAll("[id]")
      .forEach((element) => element.removeAttribute("id"));
  }
  if (props.reduceMotion || reduced.value) {
    view.style.opacity = entering ? "1" : "0";
    view.style.transform = "none";
    done();
    return;
  }
  const opacity = Number(getComputedStyle(view).opacity);
  const fromX = new DOMMatrixReadOnly(getComputedStyle(view).transform).m41;
  const targetX = entering ? 0 : -props.direction * 18;
  const playback = animate(
    view,
    {
      transform: [`translateX(${fromX}px)`, `translateX(${targetX}px)`],
      opacity: [opacity, entering ? 1 : 0],
    },
    surfaceSpring,
  );
  animations.set(element, playback);
  void playback.then(() => {
    if (animations.get(element) === playback) {
      animations.delete(element);
      done();
    }
  });
}
onBeforeUnmount(() => {
  animations.forEach((animation) => animation.stop());
});
</script>
<template>
  <div class="view-stack">
    <Transition
      :css="false"
      @before-enter="beforeEnter"
      @enter="(element, done) => transition(element, done, true)"
      @leave="(element, done) => transition(element, done, false)"
      @enter-cancelled="cancel"
      @leave-cancelled="cancel"
    >
      <fieldset :key="viewKey" class="view-stack__page"><slot /></fieldset>
    </Transition>
  </div>
</template>
