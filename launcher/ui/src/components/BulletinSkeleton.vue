<script setup lang="ts">
import { motion } from "motion-v";
import { useControlMotion } from "../composables/useControlMotion";
const { reduced } = useControlMotion();
</script>
<template>
  <div class="bulletin-skeleton" role="status" aria-label="正在读取公告">
    <motion.div
      v-for="index in 3"
      :key="index"
      class="bulletin-skeleton__entry"
      aria-hidden="true"
      :animate="{ opacity: reduced ? 0.5 : [0.35, 0.7, 0.35] }"
      :transition="{
        duration: 1.5,
        delay: (index - 1) * 0.12,
        repeat: reduced ? 0 : Infinity,
      }"
    >
      <span v-if="index === 1" class="bulletin-skeleton__image" /><span
        class="bulletin-skeleton__copy"
        ><i /><i /><i /><i
      /></span>
    </motion.div>
  </div>
</template>
