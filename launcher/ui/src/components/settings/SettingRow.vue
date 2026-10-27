<script setup lang="ts">
import { useId } from "vue";
withDefaults(
  defineProps<{
    label: string;
    description?: string;
    for?: string;
    stacked?: boolean;
  }>(),
  { stacked: false },
);
const descriptionId = useId();
defineSlots<{
  default(props: { describedby: string | undefined }): unknown;
  detail(): unknown;
}>();
</script>
<template>
  <div class="setting-row" :class="{ 'setting-row--stacked': stacked }">
    <div class="setting-row__copy">
      <label v-if="$props.for" :for="$props.for" class="setting-row__label">{{
        label
      }}</label>
      <span v-else class="setting-row__label">{{ label }}</span>
      <p
        v-if="description"
        :id="descriptionId"
        class="setting-row__description"
      >
        {{ description }}
      </p>
    </div>
    <div class="setting-row__control">
      <slot :describedby="description ? descriptionId : undefined" />
    </div>
    <div v-if="$slots.detail" class="setting-row__detail">
      <slot name="detail" />
    </div>
  </div>
</template>
