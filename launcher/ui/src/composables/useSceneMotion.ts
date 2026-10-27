import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type ShallowRef,
} from "vue";
import { useReducedMotion } from "motion-v";
import { createTimeline, spring, utils } from "animejs";
import { form, state } from "../core/controller";

export const playerExpanded = ref(false);

export function useSceneMotion(
  scene: Readonly<ShallowRef<HTMLElement | null>>,
  drawer: Readonly<ShallowRef<HTMLElement | null>>,
  shade: Readonly<ShallowRef<HTMLElement | null>>,
) {
  const prefersReduced = useReducedMotion();
  const reduced = computed(() => form.reduceMotion || prefersReduced.value);
  const opened = computed(() => state.page !== "home");
  let timeline: ReturnType<typeof createTimeline> | undefined;
  let mounted = false;

  function arrange() {
    if (!mounted || !scene.value || !drawer.value || !shade.value) return;
    timeline?.cancel();
    // 只让编排层写入空间属性, Motion 负责内部组件的弹簧与布局。
    const entering = opened.value;
    const focus = entering || playerExpanded.value;
    const travel: { duration: number } | { ease: ReturnType<typeof spring> } =
      reduced.value
        ? { duration: 0 }
        : {
            ease: spring({
              bounce: entering ? 0.18 : 0.12,
              duration: entering ? 520 : 440,
            }),
          };
    timeline = createTimeline();
    timeline
      .add(drawer.value, { x: entering ? "0%" : "110%", ...travel }, 0)
      .add(
        scene.value,
        {
          x: entering ? "-7%" : "0%",
          filter: focus ? "blur(5px)" : "blur(0px)",
          ...travel,
        },
        0,
      )
      .add(
        shade.value,
        {
          opacity: entering ? 0.48 : focus ? 0.26 : 0,
          duration: reduced.value ? 0 : 320,
          ease: "out(3)",
        },
        0,
      );
  }
  watch([opened, playerExpanded, reduced], arrange, { flush: "post" });
  onMounted(() => {
    mounted = true;
    utils.set(drawer.value!, { x: "110%" });
    arrange();
  });
  onBeforeUnmount(() => {
    mounted = false;
    timeline?.revert();
  });
  return { opened, reduced };
}
