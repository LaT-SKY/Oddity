<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { motion } from "motion-v";
import BaseButton from "../components/BaseButton.vue";
import PlaytimeHeatmap from "../components/playtime/PlaytimeHeatmap.vue";
import PlaytimeMonthlyChart from "../components/playtime/PlaytimeMonthlyChart.vue";
import PlaytimePoster from "../components/playtime/PlaytimePoster.vue";
import {
  form,
  loadPlaytime,
  playtime,
  showPage,
  state,
} from "../core/controller";
import {
  buildPlaytimeStats,
  formatClock,
  formatPlaytime,
  playModeLabel,
} from "../core/playtime";
import { useControlMotion } from "../composables/useControlMotion";

const { surface } = useControlMotion();
const now = ref(Date.now());
const posterOpen = ref(false);
let timer: number | undefined;
const stats = computed(() =>
  buildPlaytimeStats(
    playtime.history ?? { sessions: [], active: null },
    now.value,
  ),
);
const heatmapDays = computed(
  () =>
    stats.value.heatmap.weeks.flat().filter((day) => !day.future && day.ms > 0)
      .length,
);
const heatmapTotal = computed(() =>
  stats.value.heatmap.weeks.flat().reduce((total, day) => total + day.ms, 0),
);
const hasRecords = computed(() => stats.value.sessionCount > 0);
const empty = computed(
  () => playtime.loaded && !playtime.loading && !hasRecords.value,
);
const summary = computed(() => [
  { label: "累计游玩", value: formatPlaytime(stats.value.totalMs) },
  { label: "游玩天数", value: `${stats.value.daysPlayed} 天` },
  { label: "最近连续", value: `${stats.value.currentStreak} 天` },
  { label: "最长连续", value: `${stats.value.longestStreak} 天` },
  { label: "单次最长", value: formatPlaytime(stats.value.longestSessionMs) },
]);

function openPoster() {
  if (hasRecords.value) posterOpen.value = true;
}
function refresh() {
  now.value = Date.now();
  void loadPlaytime();
}
watch(
  () => state.page,
  (page) => {
    if (page !== "records") return;
    now.value = Date.now();
    void loadPlaytime();
  },
);
function startTimer() {
  if (timer !== undefined) return;
  now.value = Date.now();
  timer = window.setInterval(() => {
    now.value = Date.now();
  }, 1000);
}
function stopTimer() {
  if (timer === undefined) return;
  window.clearInterval(timer);
  timer = undefined;
}
watch(
  () => state.page === "records" && Boolean(stats.value.active),
  (running) => (running ? startTimer() : stopTimer()),
  { immediate: true },
);
onBeforeUnmount(stopTimer);
</script>

<template>
  <motion.section
    id="page-records"
    class="page playtime-page"
    :class="{ 'is-active': state.page === 'records' }"
    :inert="state.page !== 'records'"
    :aria-hidden="state.page !== 'records' || undefined"
    :initial="false"
    :animate="{
      opacity: state.page === 'records' ? 1 : 0,
      x: state.page === 'records' ? 0 : 22,
      visibility: state.page === 'records' ? 'visible' : 'hidden',
    }"
    :transition="surface"
    aria-labelledby="records-title"
    :aria-busy="playtime.loading"
  >
    <header class="page-head playtime-head">
      <div class="page-head__copy">
        <h2 id="records-title" tabindex="-1">游玩记录</h2>
        <p>计时从游戏进程出现开始, 到游戏退出结束. 记录仅保存在本机.</p>
      </div>
      <div class="page-head__actions">
        <BaseButton
          icon="i-download-simple"
          :busy="playtime.loading"
          :disabled="!hasRecords"
          @click="openPoster"
          >生成海报</BaseButton
        >
      </div>
    </header>

    <div class="playtime-content">
      <div v-if="playtime.error" class="playtime-error" role="status">
        <span>{{ playtime.error }}</span>
        <BaseButton variant="ghost" size="sm" @click="refresh">重试</BaseButton>
      </div>

      <section
        v-if="stats.active && stats.activeMs > 0"
        class="playtime-active"
        aria-live="polite"
      >
        <span class="playtime-active__dot" aria-hidden="true"></span>
        <span>正在游玩</span>
        <strong>{{ formatClock(stats.activeMs) }}</strong>
        <small>{{ playModeLabel(stats.active.mode) }}</small>
      </section>

      <section
        v-if="hasRecords || playtime.loading"
        class="playtime-summary panel"
        aria-label="游玩统计"
      >
        <div
          v-for="item in summary"
          :key="item.label"
          class="playtime-summary__item"
        >
          <span>{{ item.label }}</span>
          <strong>{{ item.value }}</strong>
        </div>
      </section>

      <template v-if="hasRecords">
        <section class="playtime-activity panel">
          <header class="playtime-section-head">
            <h3>最近 12 个月</h3>
            <span>已游玩 {{ heatmapDays }} 天</span>
          </header>
          <PlaytimeHeatmap
            :heatmap="stats.heatmap"
            :days-played="heatmapDays"
            :total-ms="heatmapTotal"
            :theme="form.theme"
          />
        </section>

        <section class="playtime-months panel">
          <header class="playtime-section-head">
            <h3>月度时长</h3>
            <span>{{ stats.months.length }} 个月</span>
          </header>
          <PlaytimeMonthlyChart :months="stats.months" :theme="form.theme" />
        </section>
      </template>

      <section v-else-if="empty" class="playtime-empty panel">
        <svg class="icon" aria-hidden="true"><use href="#i-clock" /></svg>
        <h3>还没有游玩记录</h3>
        <p>通过 Oddity 启动游戏后, 这里会记录每次游玩时间.</p>
        <BaseButton variant="ghost" @click="showPage('home')"
          >返回启动页</BaseButton
        >
      </section>
    </div>

    <PlaytimePoster
      v-model:open="posterOpen"
      :stats="stats"
      :theme="form.theme"
      :reduce-motion="form.reduceMotion"
    />
  </motion.section>
</template>

<style scoped>
.playtime-page {
  gap: 18px;
}
.playtime-head {
  align-items: flex-start;
}
.playtime-content {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding-bottom: 24px;
}
.playtime-error {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  border: 1px solid var(--danger-line);
  border-radius: var(--r-control);
  background: var(--danger-fill);
  color: var(--danger);
  font-size: var(--text-sm);
}
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
.playtime-summary {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  padding: 18px 24px;
}
.playtime-summary__item {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
  padding: 0 18px;
  border-left: 1px solid var(--line);
}
.playtime-summary__item:first-child {
  padding-left: 0;
  border-left: 0;
}
.playtime-summary__item span {
  color: var(--muted);
  font-size: var(--text-xs);
}
.playtime-summary__item strong {
  color: var(--bone);
  font-size: var(--text-xl);
  font-weight: 500;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.playtime-section-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
}
.playtime-section-head h3 {
  margin: 0;
  color: var(--bone);
  font-size: var(--text-md);
  font-weight: 600;
}
.playtime-section-head span {
  color: var(--faint);
  font-size: var(--text-xs);
}
.playtime-activity {
  gap: 16px;
}
.playtime-months {
  gap: 14px;
}
.playtime-months .playtime-chart {
  margin-top: 2px;
}
.playtime-empty {
  align-items: center;
  justify-content: center;
  gap: 12px;
  min-height: 280px;
  text-align: center;
}
.playtime-empty .icon {
  width: 28px;
  height: 28px;
  color: var(--faint);
}
.playtime-empty h3 {
  margin: 0;
  color: var(--bone);
  font-size: var(--text-lg);
  font-weight: 600;
}
.playtime-empty p {
  margin: 0;
  color: var(--muted);
  font-size: var(--text-sm);
}
@media (max-width: 960px) {
  .playtime-summary {
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 14px 0;
  }
  .playtime-summary__item:nth-child(4) {
    padding-left: 0;
    border-left: 0;
  }
}
</style>
