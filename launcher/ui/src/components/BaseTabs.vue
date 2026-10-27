<script setup lang="ts" generic="T extends string">
import { useId } from "vue";
import { motion } from "motion-v";
import { useControlMotion } from "../composables/useControlMotion";
const props = defineProps<{
  modelValue: T;
  options: readonly { value: T; label: string; disabled?: boolean }[];
  label: string;
  disabled?: boolean;
  controls?: string;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: T];
  change: [value: T];
}>();
const uid = useId();
const { control } = useControlMotion();
function choose(value: T) {
  if (value !== props.modelValue) {
    emit("update:modelValue", value);
    emit("change", value);
  }
}
function keydown(event: KeyboardEvent) {
  if (
    props.disabled ||
    !["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)
  )
    return;
  event.preventDefault();
  const enabled = props.options.filter((option) => !option.disabled);
  if (!enabled.length) return;
  const current = enabled.findIndex(
    (option) => option.value === props.modelValue,
  );
  const index =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? enabled.length - 1
        : (current + (event.key === "ArrowLeft" ? -1 : 1) + enabled.length) %
          enabled.length;
  choose(enabled[index].value);
  const group = event.currentTarget as HTMLElement;
  group
    .querySelectorAll<HTMLButtonElement>("button:not(:disabled)")
    [index]?.focus();
}
</script>
<template>
  <div
    class="control-tabs"
    role="tablist"
    :aria-label="label"
    @keydown="keydown"
  >
    <button
      v-for="option in options"
      :key="option.value"
      type="button"
      role="tab"
      class="control-tabs__tab"
      :disabled="disabled || option.disabled"
      :aria-selected="modelValue === option.value"
      :aria-controls="controls"
      :tabindex="modelValue === option.value ? 0 : -1"
      @click="choose(option.value)"
    >
      <motion.span
        v-if="modelValue === option.value"
        class="control-tabs__selection"
        :layout-id="`${uid}-selection`"
        :transition="control"
      />
      <span class="control-tabs__label">{{ option.label }}</span>
    </button>
  </div>
</template>
