<script setup lang="ts">
import { useId } from "vue";
import { motion, AnimatePresence } from "motion-v";
import { useControlMotion } from "../composables/useControlMotion";
defineProps<{ title: string; disabled?: boolean }>();
const open = defineModel<boolean>({ default: false });
const id = useId();
const { control, surface } = useControlMotion();
</script>
<template>
  <div class="control-disclosure">
    <button
      type="button"
      class="control-disclosure__trigger"
      :disabled="disabled"
      :aria-expanded="open"
      :aria-controls="id"
      @click="open = !open"
    >
      <span>{{ title }}</span
      ><motion.svg
        class="icon"
        viewBox="0 0 256 256"
        aria-hidden="true"
        :animate="{ rotate: open ? 180 : 0 }"
        :transition="control"
        ><use href="#i-caret-down"
      /></motion.svg>
    </button>
    <AnimatePresence :initial="false">
      <motion.div
        v-if="open"
        :id="id"
        class="control-disclosure__clip"
        :initial="{ height: 0, opacity: 0 }"
        :animate="{ height: 'auto', opacity: 1 }"
        :exit="{ height: 0, opacity: 0 }"
        :transition="surface"
      >
        <div class="control-disclosure__body"><slot /></div>
      </motion.div>
    </AnimatePresence>
  </div>
</template>
