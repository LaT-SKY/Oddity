<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { formatClock, playModeLabel } from "../../core/playtime";
import type { ActivePlaytime } from "../../types/launcher";

const props = defineProps<{ active: ActivePlaytime }>();
const emit = defineEmits<{ refresh: [] }>();
const now = ref(Date.now());
let timer: number | undefined;
let refreshedAt = now.value;

// 秒表只驱动自身, 汇总与图表每 30 秒才需要跟随一次。
onMounted(() => {
  now.value = Date.now();
  refreshedAt = now.value;
  timer = window.setInterval(() => {
    const current = Date.now();
    now.value = current;
    if (current - refreshedAt < 30_000) return;
    refreshedAt = current;
    emit("refresh");
  }, 1000);
});
onBeforeUnmount(() => {
  if (timer === undefined) return;
  window.clearInterval(timer);
  timer = undefined;
});
const elapsed = computed(() => Math.max(0, now.value - props.active.startedAt));
</script>

<template>
  <section
    v-if="elapsed > 0"
    class="playtime-active"
    role="timer"
    aria-label="当前游玩时长"
  >
    <span class="playtime-active__dot" aria-hidden="true"></span>
    <span>正在游玩</span>
    <strong>{{ formatClock(elapsed) }}</strong>
    <small>{{ playModeLabel(active.mode) }}</small>
  </section>
</template>

<style scoped>
.playtime-active {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 12px 16px;
  border: 1px solid var(--line);
  border-radius: var(--r-control);
  background: var(--surface-2);
  color: var(--text-soft);
  font-size: var(--text-sm);
}
.playtime-active__dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--signal-strong);
  box-shadow: 0 0 0 4px var(--signal-dim);
}
.playtime-active strong {
  font-variant-numeric: tabular-nums;
  font-size: var(--text-lg);
  font-weight: 500;
  color: var(--bone);
}
.playtime-active small {
  margin-left: auto;
  color: var(--faint);
}
</style>
