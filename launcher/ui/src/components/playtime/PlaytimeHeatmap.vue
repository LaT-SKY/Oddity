<script setup lang="ts">
import { computed, reactive, ref } from "vue";
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
const days = computed(() => props.heatmap.weeks.flat());
const playedDays = computed(() =>
  days.value
    .filter((day) => !day.future && day.ms > 0)
    .map((day) => ({
      key: day.key,
      label: `${new Date(day.date).getFullYear()}年${formatShortDate(day.date)}`,
      ms: day.ms,
    })),
);
const summary = computed(
  () =>
    `最近 12 个月游玩 ${formatPlaytime(props.totalMs)}, 已游玩 ${props.daysPlayed} 天`,
);
const { reduced } = useControlMotion();
const hovered = ref<PlaytimeDay | null>(null);
// 指针位置单独传递, 移动时只更新提示位置, 不重新渲染 371 个格子。
const pointer = reactive({ x: 0, y: 0 });
let activeCell: HTMLElement | null = null;

// 悬停只切换高亮类, 格子不做缩放动画。
function highlight(cell: HTMLElement | null) {
  if (activeCell === cell) return;
  activeCell?.classList.remove("is-hovered");
  activeCell = cell;
  activeCell?.classList.add("is-hovered");
}

function enter(event: MouseEvent) {
  const target = (event.target as Element | null)?.closest<HTMLElement>(
    ".playtime-heatmap__cell",
  );
  if (!target) {
    reset();
    return;
  }
  if (target === activeCell) {
    move(event);
    return;
  }
  const index = Number(target.dataset.index);
  const day = Number.isInteger(index) ? days.value[index] : undefined;
  if (!day || day.future) {
    reset();
    return;
  }
  highlight(target);
  pointer.x = event.clientX;
  pointer.y = event.clientY;
  hovered.value = day;
}
function move(event: MouseEvent) {
  if (!hovered.value) return;
  pointer.x = event.clientX;
  pointer.y = event.clientY;
}
function reset() {
  highlight(null);
  hovered.value = null;
}
function leave() {
  reset();
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
    <div
      class="playtime-heatmap__grid"
      role="img"
      :aria-label="summary"
      @mouseover="enter"
      @mousemove="move"
      @mouseleave="leave"
    >
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
          :data-index="weekIndex * 7 + dayIndex"
          aria-hidden="true"
        />
      </template>
    </div>
    <section class="playtime-heatmap__sr">
      <h3>每日游玩时长</h3>
      <p v-if="!playedDays.length">暂无游玩记录</p>
      <ul v-else>
        <li v-for="day in playedDays" :key="day.key">
          {{ day.label }}: {{ formatPlaytime(day.ms) }}
        </li>
      </ul>
    </section>
    <FloatingTooltip
      :open="hovered !== null"
      :pointer="pointer"
      :theme="theme"
      :reduce-motion="reduced"
      name="heatmap"
    >
      <template v-if="hovered">
        <strong>{{ formatShortDate(hovered.date) }}</strong>
        <p class="playtime-tooltip__value">
          {{ hovered.ms ? formatPlaytime(hovered.ms) : "没有记录" }}
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
  position: relative;
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
  position: relative;
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
.playtime-heatmap__cell.is-hovered {
  z-index: 1;
  box-shadow: 0 0 0 1.5px var(--bone);
}
.playtime-heatmap__cell.is-level-1,
.playtime-heatmap__legend .is-level-1 {
  background: var(--heat-1);
}
.playtime-heatmap__cell.is-level-2,
.playtime-heatmap__legend .is-level-2 {
  background: var(--heat-2);
}
.playtime-heatmap__cell.is-level-3,
.playtime-heatmap__legend .is-level-3 {
  background: var(--heat-3);
}
.playtime-heatmap__cell.is-level-4,
.playtime-heatmap__legend .is-level-4 {
  background: var(--heat-4);
}
.playtime-heatmap__cell.is-level-5,
.playtime-heatmap__legend .is-level-5 {
  background: var(--heat-5);
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
.playtime-heatmap__sr {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
  border: 0;
}
.playtime-heatmap__legend i {
  width: 9px;
  height: 9px;
  border-radius: 2px;
  background: var(--heat-0);
}
</style>
