<script setup lang="ts">
import { computed } from "vue";
import { gamepadFamily, confirmButton, backButton } from "../core/gamepad";
import type { GamepadButton } from "../types/gamepad";
const props = defineProps<{ button: GamepadButton }>();
const glyph = computed(() => {
  const family = gamepadFamily.value;
  const prefix =
    family === "dualsense" || family === "ds4"
      ? "ps"
      : family === "switch"
        ? "ns"
        : "xbox";
  const face =
    prefix === "ps"
      ? ["cross", "circle", "square", "triangle"]
      : prefix === "ns"
        ? ["b", "a", "y", "x"]
        : ["a", "b", "x", "y"];
  const button = props.button;
  if (["confirm", "back", "west", "north"].includes(button))
    return `${prefix}-${face[button === "confirm" ? confirmButton.value : button === "back" ? backButton.value : button === "west" ? 2 : 3]}`;
  return `${prefix}-${button}`;
});
</script>
<template>
  <svg class="gamepad-glyph" viewBox="0 0 64 64" aria-hidden="true">
    <use :href="`/assets/gamepad/prompts.svg#${glyph}`" />
  </svg>
</template>
