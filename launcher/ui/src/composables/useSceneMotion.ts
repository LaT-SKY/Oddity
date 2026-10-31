import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type ShallowRef,
} from "vue";
import { useReducedMotion } from "motion-v";
import { spring } from "animejs";
import { form, state } from "../core/controller";
export const playerExpanded = ref(false);
export const sceneEntering = ref(false);

export function useSceneMotion(
  scene: Readonly<ShallowRef<HTMLElement | null>>,
  drawer: Readonly<ShallowRef<HTMLElement | null>>,
  shade: Readonly<ShallowRef<HTMLElement | null>>,
) {
  const prefersReduced = useReducedMotion();
  const reduced = computed(() => form.reduceMotion || prefersReduced.value);
  const opened = computed(() => state.page !== "home");
  const enterSpring = spring({ bounce: 0.18, duration: 520 });
  const leaveSpring = spring({ bounce: 0.12, duration: 440 });
  let animations: Animation[] = [];
  let mounted = false;
  let previousOpened = false;
  let run = 0;
  function arrange() {
    if (!mounted || !scene.value || !drawer.value || !shade.value) return;
    const sceneElement = scene.value,
      drawerElement = drawer.value,
      shadeElement = shade.value;
    // 一次读取当前呈现位置, 反向时由该位置接续同一条弹簧曲线.
    const sceneStyle = getComputedStyle(sceneElement),
      drawerStyle = getComputedStyle(drawerElement);
    const sceneX =
      (new DOMMatrixReadOnly(sceneStyle.transform).m41 /
        Math.max(1, sceneElement.clientWidth)) *
      100;
    const drawerX =
      (new DOMMatrixReadOnly(drawerStyle.transform).m41 /
        Math.max(1, drawerElement.clientWidth)) *
      100;
    const blur = Number.parseFloat(
      /blur\(([-\d.]+)px\)/.exec(sceneStyle.filter)?.[1] ?? "0",
    );
    const opacity = Number(getComputedStyle(shadeElement).opacity);
    animations.forEach((animation) => animation.cancel());
    const current = ++run;
    const entering = opened.value;
    sceneEntering.value =
      entering && !reduced.value && (!previousOpened || sceneEntering.value);
    previousOpened = entering;
    const focus = entering || playerExpanded.value;
    const targetScene = entering ? -7 : 0,
      targetDrawer = entering ? 0 : 110;
    const targetBlur = focus ? 5 : 0,
      targetOpacity = entering ? 0.34 : focus ? 0.26 : 0;
    if (reduced.value) {
      sceneElement.style.transform = `translateX(${targetScene}%)`;
      sceneElement.style.filter = `blur(${targetBlur}px)`;
      drawerElement.style.transform = `translateX(${targetDrawer}%)`;
      shadeElement.style.opacity = String(targetOpacity);
      sceneEntering.value = false;
      animations = [];
      return;
    }
    const travel = entering ? enterSpring : leaveSpring;
    const steps = Math.ceil(travel.duration / 3);
    const sceneFrames: Keyframe[] = [],
      drawerFrames: Keyframe[] = [];
    for (let i = 0; i <= steps; i++) {
      const offset = i / steps,
        progress = travel.ease(offset);
      sceneFrames.push({
        offset,
        transform: `translateX(${sceneX + (targetScene - sceneX) * progress}%)`,
        filter: `blur(${Math.max(0, blur + (targetBlur - blur) * progress)}px)`,
      });
      drawerFrames.push({
        offset,
        transform: `translateX(${drawerX + (targetDrawer - drawerX) * progress}%)`,
      });
    }
    const options: KeyframeAnimationOptions = {
      duration: travel.duration,
      fill: "both",
      easing: "linear",
    };
    const shadeFrames = Array.from({ length: 108 }, (_, i) => {
      const offset = i / 107,
        p = 1 - (1 - offset) ** 3;
      return { offset, opacity: opacity + (targetOpacity - opacity) * p };
    });
    animations = [
      drawerElement.animate(drawerFrames, options),
      sceneElement.animate(sceneFrames, options),
      shadeElement.animate(shadeFrames, {
        duration: 320,
        fill: "both",
        easing: "linear",
      }),
    ];
    void Promise.all(animations.map((animation) => animation.finished)).then(
      () => {
        if (current !== run) return;
        animations.forEach((animation) => {
          animation.commitStyles();
          animation.cancel();
        });
        animations = [];
        sceneEntering.value = false;
      },
      () => {},
    );
  }
  watch([opened, playerExpanded, reduced], arrange, { flush: "post" });
  onMounted(() => {
    mounted = true;
    drawer.value!.style.transform = "translateX(110%)";
    // 首页初始位置已由样式给出, 避免为零位移启动合成与同步布局.
    if (opened.value || playerExpanded.value) arrange();
  });
  onBeforeUnmount(() => {
    mounted = false;
    run++;
    sceneEntering.value = false;
    animations.forEach((animation) => animation.cancel());
  });
  return { opened, reduced };
}
