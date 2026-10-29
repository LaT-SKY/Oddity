<script setup lang="ts">
import { computed } from "vue";
import { gamepadGlyph } from "../core/gamepad";
import type { GamepadButton } from "../types/gamepad";
const props = defineProps<{ button: GamepadButton; symbol?: string }>();
const glyph = computed(() => props.symbol ?? gamepadGlyph(props.button));
</script>
<template>
  <svg class="gamepad-glyph" viewBox="0 0 64 64" aria-hidden="true">
    <use :href="`/assets/gamepad/prompts.svg#${glyph}`" />
  </svg>
</template>
