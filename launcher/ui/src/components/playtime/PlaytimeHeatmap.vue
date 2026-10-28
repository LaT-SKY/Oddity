<script setup lang="ts">
import { computed, ref } from "vue";
import { useAnimate } from "motion-v";
import {
  formatPlaytime,
  formatShortDate,
  type PlaytimeDay,
  type PlaytimeHeatmap,
} from "../../core/playtime";
import { useControlMotion } from "../../composables/useControlMotion";
import FloatingTooltip from "./FloatingTooltip.vue";

const props = defineProps<{
  heatmap: PlaytimeHeatmap;
  daysPlayed: number;
  totalMs: number;
  theme?: string;
}>();

const weekdays = ["一", "", "三", "", "五", "", "日"];
const summary = computed(
  () =>
    `最近 12 个月游玩 ${formatPlaytime(props.totalMs)}, 已游玩 ${props.daysPlayed} 天`,
);
const [, animate] = useAnimate();
const { reduced } = useControlMotion();
const hovered = ref<{ day: PlaytimeDay; x: number; y: number } | null>(null);
let activeCell: HTMLElement | null = null;

function enter(day: PlaytimeDay, event: MouseEvent) {
  if (day.future) return;
  const cell = event.currentTarget as HTMLElement | null;
  if (!cell) return;
  if (activeCell && activeCell !== cell)
    animate(
      activeCell,
      { scale: 1 },
      reduced.value ? { duration: 0 } : { duration: 0.12 },
    );
  activeCell = cell;
  cell.style.zIndex = "2";
  animate(
    cell,
    { scale: 1.45 },
    reduced.value
      ? { duration: 0 }
      : { type: "spring", stiffness: 520, damping: 28 },
  );
  hovered.value = { day, x: event.clientX, y: event.clientY };
}
function move(event: MouseEvent) {
  if (!hovered.value) return;
  hovered.value = {
    ...hovered.value,
    x: event.clientX,
    y: event.clientY,
  };
}
function leave() {
  if (activeCell) {
    animate(
      activeCell,
      { scale: 1 },
      reduced.value ? { duration: 0 } : { duration: 0.12 },
    );
    activeCell.style.zIndex = "";
  }
  activeCell = null;
  hovered.value = null;
}
</script>

<template>
  <div class="playtime-heatmap">
    <div class="playtime-heatmap__months" aria-hidden="true">
      <span
        v-for="label in heatmap.months"
        :key="`${label.column}-${label.label}`"
        :style="{ gridColumn: `${label.column + 1} / span 4` }"
        >{{ label.label }}</span
      >
    </div>
    <div class="playtime-heatmap__grid" role="img" :aria-label="summary">
      <span
        v-for="(day, index) in weekdays"
        :key="`weekday-${index}`"
        class="playtime-heatmap__weekday"
        :style="{ gridRow: index + 1 }"
        aria-hidden="true"
        >{{ day }}</span
      >
      <template v-for="(week, weekIndex) in heatmap.weeks" :key="weekIndex">
        <span
          v-for="(day, dayIndex) in week"
          :key="day.key"
          class="playtime-heatmap__cell"
          :class="[`is-level-${day.level}`, { 'is-future': day.future }]"
          :style="{ gridColumn: weekIndex + 2, gridRow: dayIndex + 1 }"
          aria-hidden="true"
          @mouseenter="enter(day, $event)"
          @mousemove="move"
          @mouseleave="leave"
        />
      </template>
    </div>
    <FloatingTooltip
      :open="hovered !== null"
      :x="hovered?.x ?? 0"
      :y="hovered?.y ?? 0"
      :theme="theme"
      :reduce-motion="reduced"
      name="heatmap"
    >
      <template v-if="hovered">
        <strong>{{ formatShortDate(hovered.day.date) }}</strong>
        <p class="playtime-tooltip__value">
          {{ hovered.day.ms ? formatPlaytime(hovered.day.ms) : "没有记录" }}
        </p>
      </template>
    </FloatingTooltip>
    <div class="playtime-heatmap__legend" aria-hidden="true">
      <span>少</span>
      <i v-for="level in 5" :key="level" :class="`is-level-${level}`" />
      <span>多</span>
    </div>
  </div>
</template>

<style scoped>
.playtime-heatmap {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}
.playtime-heatmap__months {
  display: grid;
  grid-template-columns: repeat(53, minmax(0, 1fr));
  gap: 3px;
  margin-left: 21px;
  min-height: 14px;
  overflow: hidden;
  color: var(--faint);
  font-size: var(--text-2xs);
  line-height: 14px;
}
.playtime-heatmap__months span {
  white-space: nowrap;
}
.playtime-heatmap__grid {
  display: grid;
  grid-template-columns: 18px repeat(53, minmax(0, 1fr));
  grid-template-rows: repeat(7, auto);
  gap: 3px;
  min-width: 0;
}
.playtime-heatmap__weekday {
  display: flex;
  align-items: center;
  color: var(--faint);
  font-size: var(--text-2xs);
  line-height: 1;
}
.playtime-heatmap__cell {
  position: relative;
  width: 100%;
  aspect-ratio: 1;
  border-radius: 2px;
  background: var(--heat-0);
}
.playtime-heatmap__cell.is-level-1,
.playtime-heatmap__legend .is-level-1 {
  background: color-mix(in srgb, var(--signal-strong) 18%, transparent);
}
.playtime-heatmap__cell.is-level-2,
.playtime-heatmap__legend .is-level-2 {
  background: color-mix(in srgb, var(--signal-strong) 34%, transparent);
}
.playtime-heatmap__cell.is-level-3,
.playtime-heatmap__legend .is-level-3 {
  background: color-mix(in srgb, var(--signal-strong) 54%, transparent);
}
.playtime-heatmap__cell.is-level-4,
.playtime-heatmap__legend .is-level-4 {
  background: color-mix(in srgb, var(--signal-strong) 76%, transparent);
}
.playtime-heatmap__cell.is-level-5,
.playtime-heatmap__legend .is-level-5 {
  background: var(--signal-strong);
}
.playtime-heatmap__cell.is-future {
  background: transparent;
}
.playtime-heatmap__legend {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
  color: var(--faint);
  font-size: var(--text-2xs);
}
.playtime-heatmap__legend i {
  width: 9px;
  height: 9px;
  border-radius: 2px;
  background: var(--heat-0);
}
</style>
