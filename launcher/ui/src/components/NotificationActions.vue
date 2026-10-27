<script setup lang="ts">
import { ref } from "vue";
import { motion } from "motion-v";
import type { NotificationItem } from "../core/notifications";
import { runNotificationAction } from "../core/notifications";
defineProps<{ item: NotificationItem; reduced: boolean }>();
const hovered = ref(false);
const focused = ref(false);
</script>

<template>
  <motion.div v-if="item.primary" class="notification-actions" layout>
    <motion.button
      class="notification-action notification-action--primary"
      type="button"
      :aria-label="item.primary.label"
      :disabled="item.busy"
      :while-press="reduced ? {} : { scale: 0.94 }"
      @pointerenter="hovered = true"
      @pointerleave="hovered = false"
      @focus="focused = true"
      @blur="focused = false"
      @click="runNotificationAction(item.id, 'primary')"
    >
      <svg class="icon" aria-hidden="true">
        <use :href="`#${item.primary.icon}`" />
      </svg>
      <motion.span
        class="notification-action-label"
        aria-hidden="true"
        :initial="false"
        :animate="{
          width: hovered || focused ? 'auto' : 0,
          opacity: hovered || focused ? 1 : 0,
          marginLeft: hovered || focused ? 7 : 0,
        }"
        :transition="
          reduced
            ? { duration: 0 }
            : {
                type: 'spring',
                stiffness: 430,
                damping: 30,
                opacity: { duration: 0.12 },
              }
        "
      >
        <span>{{ item.primary.label }}</span>
      </motion.span>
    </motion.button>
    <motion.button
      v-if="item.secondary"
      class="notification-action notification-action--ghost"
      layout="position"
      type="button"
      :disabled="item.busy"
      :while-press="reduced ? {} : { scale: 0.95 }"
      @click="runNotificationAction(item.id, 'secondary')"
      >{{ item.secondary.label }}</motion.button
    >
  </motion.div>
</template>
