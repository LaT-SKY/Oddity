<script setup lang="ts">
import { ref } from "vue";
import { motion } from "motion-v";
import { useControlMotion } from "../composables/useControlMotion";
defineProps<{ src: string }>();
const emit = defineEmits<{ error: [] }>();
const loaded = ref(false);
const { reduced } = useControlMotion();
</script>
<template>
  <div class="bulletin-media">
    <div class="bulletin-media__frame">
      <motion.span
        v-if="!loaded"
        class="bulletin-media__placeholder"
        aria-hidden="true"
        :animate="{ opacity: reduced ? 0.5 : [0.35, 0.65, 0.35] }"
        :transition="{ duration: 1.5, repeat: reduced ? 0 : Infinity }"
      />
      <motion.img
        :src="src"
        alt=""
        loading="lazy"
        decoding="async"
        :initial="{ opacity: 0 }"
        :animate="{ opacity: loaded ? 1 : 0 }"
        :transition="{ duration: reduced ? 0 : 0.18 }"
        @load="loaded = true"
        @error="emit('error')"
      />
    </div>
  </div>
</template>
