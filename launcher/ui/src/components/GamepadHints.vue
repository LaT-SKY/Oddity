<script setup lang="ts">
import { onBeforeUnmount, reactive, watch } from "vue";
import { motion } from "motion-v";
import { createTimeline } from "animejs";
import { gamepadGlyph } from "../core/gamepad";
import type { GamepadHint } from "../types/gamepad";
import GamepadGlyph from "./GamepadGlyph.vue";
const props = defineProps<{
  items: GamepadHint[];
  reduced: boolean;
}>();
const slots = reactive(
  Array.from({ length: 3 }, () => ({
    hint: null as GamepadHint | null,
    key: "",
    visible: false,
    symbols: [] as string[],
  })),
);
const timelines: (ReturnType<typeof createTimeline> | undefined)[] = [];
const requested = ["", "", ""];
watch(
  () =>
    [
      props.items.map((hint) => {
        const symbols = hint.buttons.map(gamepadGlyph);
        return {
          hint: { ...hint, buttons: [...hint.buttons] },
          symbols,
          key: `${hint.id}/${hint.label}/${symbols.join()}`,
        };
      }),
      props.reduced,
    ] as const,
  ([items, reduced]) => {
    slots.forEach((slot, index) => {
      const next = items[index];
      const key = next?.key ?? "";
      if (
        requested[index] === key &&
        (!reduced || (slot.key === key && slot.visible === !!next))
      )
        return;
      requested[index] = key;
      timelines[index]?.cancel();
      if (slot.key === key) {
        slot.visible = !!next;
        return;
      }
      const replace = () => {
        slot.hint = next?.hint ?? null;
        slot.symbols = next?.symbols ?? [];
        slot.key = key;
        slot.visible = !!next;
      };
      if (reduced) {
        replace();
        return;
      }
      const entering = !slot.hint;
      slot.visible = false;
      if (entering) {
        slot.hint = next?.hint ?? null;
        slot.symbols = next?.symbols ?? [];
        slot.key = key;
      }
      const timeline = createTimeline();
      timelines[index] = timeline;
      timeline.call(replace, entering ? 16 : 140);
    });
  },
  { immediate: true },
);
onBeforeUnmount(() => timelines.forEach((timeline) => timeline?.cancel()));
</script>
<template>
  <div
    class="gamepad-hints"
    :data-reduced="reduced || undefined"
    aria-label="手柄操作提示"
  >
    <span
      v-for="(slot, index) in slots"
      :key="index"
      v-memo="[slot.key, slot.visible, reduced]"
      class="gamepad-hints__slot"
    >
      <motion.span
        v-if="slot.hint"
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
        <span class="gamepad-hints__buttons">
          <GamepadGlyph
            v-for="(button, index) in slot.hint.buttons"
            :key="button"
            :button="button"
            :symbol="slot.symbols[index]"
          />
        </span>
        <span class="gamepad-hints__label">{{ slot.hint.label }}</span>
      </motion.span>
    </span>
  </div>
</template>
