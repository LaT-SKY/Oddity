<script setup lang="ts">
import { useId } from "vue";
const props = defineProps<{
  title: string;
  description?: string;
  id?: string;
}>();
const generatedId = useId();
</script>
<template>
  <section class="setting-group" :aria-labelledby="props.id ?? generatedId">
    <header class="setting-group__head">
      <div>
        <h3 :id="props.id ?? generatedId">{{ title }}</h3>
        <p v-if="description">{{ description }}</p>
      </div>
      <slot name="action" />
    </header>
    <div class="setting-group__items"><slot /></div>
  </section>
</template>
