<script setup lang="ts">
import { motion } from "motion-v";
import { useControlMotion } from "../composables/useControlMotion";
defineProps<{
  modelValue: boolean;
  disabled?: boolean;
  id?: string;
  label?: string;
  describedby?: string;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  change: [event: Event];
}>();
const { control } = useControlMotion();
</script>
<template>
  <span class="control-switch">
    <input
      :id="id"
      type="checkbox"
      :checked="modelValue"
      :disabled="disabled"
      :aria-label="label"
      :aria-describedby="describedby"
      @change="
        emit('update:modelValue', ($event.target as HTMLInputElement).checked);
        emit('change', $event);
      "
    />
    <motion.span
      class="control-switch__track"
      :initial="false"
      aria-hidden="true"
      :animate="{
        backgroundColor: modelValue
          ? 'var(--switch-on-bg)'
          : 'var(--switch-bg)',
      }"
      :transition="control"
    >
      <motion.span
        class="control-switch__thumb"
        :initial="false"
        :animate="{
          x: modelValue ? 18 : 0,
          backgroundColor: modelValue
            ? 'var(--switch-on-knob)'
            : 'var(--switch-knob)',
        }"
        :transition="control"
      />
    </motion.span>
  </span>
</template>
