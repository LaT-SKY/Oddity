<script setup lang="ts">
import { onBeforeUnmount, reactive, watch } from "vue";
import { motion } from "motion-v";
import { createTimeline } from "animejs";
import { gamepadFamily } from "../core/gamepad";
import type { GamepadHint } from "../types/gamepad";
import GamepadGlyph from "./GamepadGlyph.vue";
const props = defineProps<{
  items: GamepadHint[];
  reduced: boolean;
  theme: string;
}>();
const slots = reactive(
  Array.from({ length: 3 }, () => ({
    hint: null as GamepadHint | null,
    key: "",
    visible: false,
  })),
);
let timeline: ReturnType<typeof createTimeline> | undefined;
watch(
  () => [props.items, props.theme, gamepadFamily.value],
  () => {
    timeline?.cancel();
    const changes = slots.flatMap((slot, index) => {
      const hint = props.items[index] ?? null;
      const key = hint
        ? `${hint.id}/${hint.label}/${hint.buttons.join()}/${gamepadFamily.value}`
        : "";
      return slot.key !== key || (hint && !slot.visible)
        ? [{ slot, hint, key }]
        : [];
    });
    for (const { slot } of changes) slot.visible = false;
    const replace = () => {
      for (const { slot, hint, key } of changes) {
        slot.hint = hint;
        slot.key = key;
        slot.visible = !!hint;
      }
    };
    if (props.reduced) replace();
    else if (changes.length) {
      timeline = createTimeline();
      timeline.call(replace, 110);
    }
  },
  { immediate: true },
);
onBeforeUnmount(() => timeline?.cancel());
</script>
<template>
  <div class="gamepad-hints" :data-theme="theme" aria-label="手柄操作提示">
    <span
      v-for="(slot, index) in slots"
      :key="index"
      class="gamepad-hints__slot"
    >
      <motion.span
        v-if="slot.hint"
        :key="slot.key"
        class="gamepad-hints__item"
        :data-hint="slot.hint.id"
        :initial="{
          opacity: 0,
          filter: reduced ? 'blur(0px)' : 'blur(6px)',
          y: reduced ? 0 : 4,
        }"
        :animate="{
          opacity: slot.visible ? 1 : 0,
          filter: slot.visible || reduced ? 'blur(0px)' : 'blur(6px)',
          y: slot.visible || reduced ? 0 : -4,
        }"
        :transition="{ duration: reduced ? 0 : 0.14, ease: 'easeOut' }"
      >
        <GamepadGlyph
          v-for="button in slot.hint.buttons"
          :key="button"
          :button="button"
        />{{ slot.hint.label }}
      </motion.span>
    </span>
  </div>
</template>
