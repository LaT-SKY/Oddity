<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { motion, useReducedMotion } from "motion-v";
import { createTimeline } from "animejs";
import { gamepad, gamepadTarget, gamepadViewport } from "../core/gamepad";
import { form, state } from "../core/controller";
import { playerExpanded, sceneEntering } from "../composables/useSceneMotion";
import GamepadHints from "./GamepadHints.vue";
import type { GamepadHint } from "../types/gamepad";
import GamepadGlyph from "./GamepadGlyph.vue";
const prefersReduced = useReducedMotion();
const reduced = computed(() => form.reduceMotion || prefersReduced.value);
const box = ref<{ x: number; y: number; width: number; height: number } | null>(
  null,
);
const shape = ref({ scaleX: 1, scaleY: 1 });
const detail = ref(true);
const selectOpen = ref(false);
const browsing = ref(false);
const primaryLabel = ref("");
const clip = ref("inset(0px)");
let frame = 0;
let resize: ResizeObserver | undefined;
let mutations: MutationObserver | undefined;
let ancestors: HTMLElement[] = [];
let timeline: ReturnType<typeof createTimeline> | undefined;
const spring = computed(() =>
  reduced.value
    ? { duration: 0 }
    : { type: "spring" as const, stiffness: 410, damping: 25, mass: 0.8 },
);
let measuredOwner: HTMLElement | null = null;
function measure() {
  frame = 0;
  const owner = gamepadTarget.value;
  browsing.value = owner?.hasAttribute("data-pad-browsing") ?? false;
  primaryLabel.value =
    document
      .querySelector(".page.is-active [data-pad-primary]")
      ?.getAttribute("data-pad-action") ?? "";
  selectOpen.value =
    owner?.getAttribute("role") === "combobox" &&
    owner.getAttribute("aria-expanded") === "true";
  const option = selectOpen.value
    ? document.getElementById(
        owner?.getAttribute("aria-activedescendant") || "",
      )
    : null;
  let target = option ?? owner;
  if (target && !option) {
    const row = target.closest<HTMLElement>(".setting-row");
    if (
      row &&
      row.querySelectorAll("button,input:not([hidden]),textarea").length === 1
    )
      target = row;
  }
  if (
    gamepad.active &&
    target?.isConnected &&
    !target.closest('[inert],[hidden],[aria-hidden="true"]')
  ) {
    const rect = target.getBoundingClientRect();
    const viewport = gamepadViewport(target);
    clip.value = `inset(${Math.max(0, viewport.top)}px 0 ${Math.max(0, innerHeight - viewport.bottom)}px 0)`;
    if (
      rect.width &&
      rect.height &&
      rect.bottom > viewport.top &&
      rect.top < viewport.bottom
    ) {
      const next = {
        x: rect.left - 5,
        y: rect.top - 4,
        width: rect.width + 10,
        height: rect.height + 8,
      };
      if (measuredOwner !== owner) {
        measuredOwner = owner ?? null;
        timeline?.cancel();
        const horizontal = box.value
          ? Math.abs(next.x - box.value.x) > Math.abs(next.y - box.value.y)
          : false;
        const distance = box.value
          ? Math.hypot(next.x - box.value.x, next.y - box.value.y)
          : 0;
        const amount = reduced.value ? 0 : distance > 250 ? 0.025 : 0.055;
        shape.value = {
          scaleX: horizontal ? 1 + amount : 1 - amount / 2,
          scaleY: horizontal ? 1 - amount / 2 : 1 + amount,
        };
        detail.value = false;
        timeline = createTimeline();
        timeline.call(
          () => {
            detail.value = true;
          },
          reduced.value ? 0 : 30,
        );
        timeline.call(
          () => {
            shape.value = { scaleX: 1, scaleY: 1 };
          },
          reduced.value ? 0 : 85,
        );
      }
      if (
        !box.value ||
        Object.entries(next).some(
          ([key, value]) =>
            Math.abs(box.value![key as keyof typeof next] - value) > 0.25,
        )
      )
        box.value = next;
    } else box.value = null;
  } else box.value = null;
  // 过渡中的目标仍逐帧跟随, 静止后等待布局事件。
  if (
    selectOpen.value ||
    ancestors.some((element) =>
      element
        .getAnimations()
        .some((animation) => animation.playState === "running"),
    )
  )
    schedule();
}
function schedule() {
  if (!frame) frame = requestAnimationFrame(measure);
}
watch(
  () => [gamepad.active, gamepadTarget.value, state.page],
  () => {
    resize?.disconnect();
    mutations?.disconnect();
    cancelAnimationFrame(frame);
    frame = 0;
    ancestors = [];
    const owner = gamepadTarget.value;
    if (!gamepad.active || !owner) {
      box.value = null;
      return;
    }
    resize = new ResizeObserver(schedule);
    mutations = new MutationObserver(schedule);
    const attributes = [
      "style",
      "class",
      "hidden",
      "inert",
      "aria-expanded",
      "aria-activedescendant",
      "data-pad-browsing",
      "data-pad-action",
    ];
    for (
      let element: HTMLElement | null = owner;
      element && element !== document.body;
      element = element.parentElement
    ) {
      ancestors.push(element);
      resize.observe(element);
      mutations.observe(element, {
        attributes: true,
        attributeFilter: attributes,
      });
    }
    const content = owner.closest(".page,.modal,.notification-stack") ?? owner;
    mutations.observe(content, {
      attributes: true,
      attributeFilter: attributes,
      childList: true,
      characterData: true,
      subtree: true,
    });
    schedule();
  },
  { immediate: true, flush: "post" },
);
document.addEventListener("scroll", schedule, true);
window.addEventListener("resize", schedule);
onBeforeUnmount(() => {
  cancelAnimationFrame(frame);
  resize?.disconnect();
  mutations?.disconnect();
  document.removeEventListener("scroll", schedule, true);
  window.removeEventListener("resize", schedule);
  timeline?.cancel();
});
const local = computed(
  () =>
    gamepad.notification ||
    !!gamepad.editing ||
    selectOpen.value ||
    !!gamepadTarget.value?.closest(".modal,[data-pad-modal]"),
);
const hints = computed<GamepadHint[]>(() => {
  if (gamepad.windowControls)
    return [
      {
        id: "confirm",
        buttons: ["confirm"],
        label: gamepadTarget.value?.getAttribute("aria-label") || "操作",
      },
      { id: "back", buttons: ["back"], label: "返回" },
    ];
  if (state.page === "home" && !local.value) {
    const launching = state.game === "launching";
    const running = state.game === "running";
    const closing = state.game === "closing";
    const launch = launching
      ? state.launch?.cancelling
        ? { id: "pause", label: "正在暂停" }
        : { id: "pause", label: "暂停启动" }
      : running
        ? { id: "close", label: "关闭游戏" }
        : closing
          ? { id: "close", label: "正在关闭" }
          : { id: "launch", label: "启动" };
    return [
      { ...launch, buttons: ["west"] },
      {
        id: "player",
        buttons: ["north"],
        label: playerExpanded.value ? "关闭玩家信息" : "玩家信息",
      },
      { id: "page", buttons: ["lb", "rb"], label: "切页" },
    ];
  }
  if (selectOpen.value)
    return [
      { id: "move", buttons: ["move"], label: "选择" },
      { id: "confirm", buttons: ["confirm"], label: "确认" },
      { id: "back", buttons: ["back"], label: "取消" },
    ];
  if (browsing.value)
    return [
      { id: "move", buttons: ["move"], label: "浏览" },
      { id: "back", buttons: ["back"], label: "结束浏览" },
    ];
  const back = {
    id: "back",
    buttons: ["back"] as const,
    label: local.value
      ? "关闭"
      : gamepad.navigation
        ? "返回"
        : ["settings", "bulletin", "game-mode"].includes(state.page)
          ? "返回分类"
          : "返回",
  };
  const result: GamepadHint[] = gamepadTarget.value?.matches(
    'input[type="range"],input[type="number"]',
  )
    ? [
        { id: "move", buttons: ["move"], label: "调整" },
        { ...back, buttons: [...back.buttons] },
      ]
    : [
        {
          id: "confirm",
          buttons: ["confirm"],
          label: gamepad.editing
            ? "输入"
            : gamepad.navigation && !local.value
              ? "进入"
              : "操作",
        },
        { ...back, buttons: [...back.buttons] },
      ];
  if (!local.value && primaryLabel.value)
    result.push({ id: "primary", buttons: ["r3"], label: primaryLabel.value });
  return result;
});
</script>
<template>
  <Teleport to="body">
    <div
      class="gamepad-select-layer"
      :style="{ clipPath: clip }"
      aria-hidden="true"
    >
      <motion.div
        v-if="box"
        class="gamepad-select"
        :data-theme="form.theme"
        :initial="false"
        :animate="box"
        :transition="spring"
        aria-hidden="true"
      >
        <motion.div
          class="gamepad-select__surface"
          :animate="{
            scaleX: gamepad.pressing ? 0.978 : shape.scaleX,
            scaleY: gamepad.pressing ? 0.965 : shape.scaleY,
            y: gamepad.pressing ? 2 : 0,
          }"
          :transition="
            reduced
              ? { duration: 0 }
              : { type: 'spring', stiffness: 500, damping: 18, mass: 0.65 }
          "
        />
        <motion.span
          v-if="
            !browsing &&
            !gamepadTarget?.classList.contains('control-button') &&
            gamepadTarget?.id !== 'launch-btn' &&
            !gamepadTarget?.classList.contains('game-records__capsule')
          "
          class="gamepad-select__key"
          :class="{ 'gamepad-select__key--window': gamepad.windowControls }"
          :animate="{
            opacity: detail ? 1 : 0.2,
            scale: gamepad.pressing ? 0.85 : detail ? 1 : 0.8,
            y: detail ? 0 : 4,
          }"
          :transition="spring"
          ><GamepadGlyph button="confirm"
        /></motion.span>
      </motion.div>
    </div>
    <GamepadHints
      v-if="gamepad.active"
      :items="hints"
      :reduced="!!reduced"
      :defer-enter="sceneEntering"
    />
  </Teleport>
</template>
