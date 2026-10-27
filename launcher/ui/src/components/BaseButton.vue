<script setup lang="ts">
import { motion, AnimatePresence } from "motion-v";
import { useControlMotion } from "../composables/useControlMotion";
withDefaults(
  defineProps<{
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "sm" | "md";
    icon?: string;
    iconOnly?: boolean;
    busy?: boolean;
    disabled?: boolean;
    type?: "button" | "submit" | "reset";
  }>(),
  { variant: "secondary", size: "md", type: "button" },
);
const { control, reduced } = useControlMotion();
</script>
<template>
  <motion.button
    class="control-button"
    :class="[
      `control-button--${variant}`,
      `control-button--${size}`,
      { 'control-button--icon': iconOnly },
    ]"
    :type="type"
    :disabled="disabled || busy"
    :aria-busy="busy || undefined"
    :while-press="disabled || busy || reduced ? undefined : { scale: 0.96 }"
    :transition="control"
  >
    <span v-if="icon || busy" class="control-button__symbol">
      <AnimatePresence mode="wait">
        <motion.svg
          v-if="busy"
          key="busy"
          class="icon"
          viewBox="0 0 256 256"
          aria-hidden="true"
          :initial="{ opacity: 0 }"
          :animate="{ opacity: 1, rotate: reduced ? 0 : 360 }"
          :exit="{ opacity: 0 }"
          :transition="{
            opacity: { duration: 0.1 },
            rotate: {
              duration: 1,
              repeat: reduced ? 0 : Infinity,
              ease: 'linear',
            },
          }"
        >
          <use href="#i-arrows-clockwise" />
        </motion.svg>
        <motion.svg
          v-else
          key="icon"
          class="icon"
          viewBox="0 0 256 256"
          aria-hidden="true"
          :initial="false"
          :animate="{ opacity: 1 }"
          :exit="{ opacity: 0 }"
          ><use :href="`#${icon}`"
        /></motion.svg>
      </AnimatePresence>
    </span>
    <span v-if="!iconOnly" class="control-button__label"><slot /></span>
  </motion.button>
</template>
