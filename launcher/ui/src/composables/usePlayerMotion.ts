import {
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type Ref,
  type ShallowRef,
} from "vue";
import { createTimeline, spring, utils } from "animejs";

const compactWidth = 204;
const expandedWidth = 280;
const headerHeight = 44;

export function usePlayerMotion(
  open: Ref<boolean>,
  reduced: Ref<boolean>,
  surface: Readonly<ShallowRef<HTMLElement | null>>,
  details: Readonly<ShallowRef<HTMLElement | null>>,
) {
  const detailsReady = ref(false);
  const summaryReady = ref(true);
  const moving = ref(false);
  let timeline: ReturnType<typeof createTimeline> | undefined;
  let observer: ResizeObserver | undefined;
  let generation = 0;
  let mounted = false;
  const height = () => headerHeight + (details.value?.scrollHeight ?? 0) + 2;

  function fit() {
    if (!surface.value || !open.value || moving.value) return;
    timeline?.cancel();
    timeline = createTimeline().add(surface.value, {
      height: height(),
      ...(reduced.value
        ? { duration: 0 }
        : { ease: spring({ bounce: 0.08, duration: 260 }) }),
    });
  }

  async function arrange() {
    const run = ++generation;
    timeline?.cancel();
    detailsReady.value = false;
    summaryReady.value = reduced.value;
    moving.value = !reduced.value;
    await nextTick();
    if (!mounted || run !== generation || !surface.value) return;
    observer?.disconnect();
    if (details.value) observer?.observe(details.value);
    const expandedHeight = height();
    if (reduced.value) {
      utils.set(surface.value, {
        width: open.value ? expandedWidth : compactWidth,
        height: open.value ? expandedHeight : headerHeight,
        x: 0,
        y: 0,
        borderRadius: open.value ? 20 : 14,
      });
      detailsReady.value = open.value;
      summaryReady.value = true;
      moving.value = false;
      return;
    }
    const entering = open.value;
    const reveal = () => {
      if (run !== generation) return;
      detailsReady.value = entering;
      summaryReady.value = true;
    };
    // 外壳改变真实尺寸, 文字独立显露, 不参与缩放。
    timeline = createTimeline({
      onComplete: () => {
        if (run !== generation) return;
        moving.value = false;
        reveal();
        if (entering && Math.abs(surface.value!.offsetHeight - height()) > 1)
          fit();
      },
    });
    if (entering) {
      timeline
        .add(
          surface.value,
          {
            width: 44,
            height: 44,
            borderRadius: 22,
            x: -128,
            y: 0,
            duration: 120,
            ease: "inOut(2)",
          },
          0,
        )
        .add(
          surface.value,
          {
            width: expandedWidth,
            height: expandedHeight,
            borderRadius: 20,
            x: -48,
            y: 54,
            duration: 180,
            ease: "out(2)",
          },
          120,
        )
        .add(
          surface.value,
          { x: 0, y: 0, ease: spring({ bounce: 0.18, duration: 240 }) },
          230,
        )
        .call(reveal, 250);
    } else {
      timeline
        .add(
          surface.value,
          { x: -48, y: 40, duration: 110, ease: "inOut(2)" },
          0,
        )
        .add(
          surface.value,
          {
            width: 44,
            height: 44,
            borderRadius: 22,
            x: -128,
            y: 0,
            duration: 150,
            ease: "inOut(2)",
          },
          80,
        )
        .add(
          surface.value,
          {
            width: compactWidth,
            height: headerHeight,
            borderRadius: 14,
            x: 0,
            y: 0,
            ease: spring({ bounce: 0.1, duration: 230 }),
          },
          230,
        )
        .call(reveal, 340);
    }
  }
  watch([open, reduced], arrange, { flush: "post" });
  onMounted(() => {
    mounted = true;
    observer = new ResizeObserver(fit);
  });
  onBeforeUnmount(() => {
    mounted = false;
    generation++;
    observer?.disconnect();
    timeline?.cancel();
  });
  return { detailsReady, summaryReady, moving };
}
