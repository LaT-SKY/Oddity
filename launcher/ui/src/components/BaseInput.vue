<script setup lang="ts">
defineOptions({ inheritAttrs: false });
withDefaults(defineProps<{ invalid?: boolean; mono?: boolean }>(), {
  invalid: false,
  mono: false,
});
const model = defineModel<string>({ required: true });
const emit = defineEmits<{ input: [event: Event]; change: [event: Event] }>();
</script>
<template>
  <input
    v-bind="$attrs"
    v-model="model"
    class="control-input"
    :class="{ 'control-input--mono': mono }"
    :aria-invalid="invalid || undefined"
    @input="emit('input', $event)"
    @change="emit('change', $event)"
  />
</template>
