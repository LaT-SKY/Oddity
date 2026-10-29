<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from "vue";
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

interface HoverLayer {
  x: number;
  y: number;
  size: number;
  level: number;
}

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
const [, animate] = useAnimate();
const { reduced } = useControlMotion();
const hovered = ref<PlaytimeDay | null>(null);
// 指针位置单独传递, 移动时只更新提示位置, 不重新渲染 371 个格子。
const pointer = reactive({ x: 0, y: 0 });
let activeCell: HTMLElement | null = null;

// 格子本身不参与动画, 悬停放大交给两个覆盖层, 只让合成层变化。
const grid = ref<HTMLElement | null>(null);
let geometry: { cell: number; gap: number; left: number } | null = null;
const layerA = ref<HoverLayer | null>(null);
const layerB = ref<HoverLayer | null>(null);
const elementA = ref<HTMLElement | null>(null);
const elementB = ref<HTMLElement | null>(null);
const slots = [
  { layer: layerA, element: elementA },
  { layer: layerB, element: elementB },
] as const;
const animations: Array<ReturnType<typeof animate> | undefined> = [
  undefined,
  undefined,
];
const timers = [0, 0];
let activeSlot = 1;

function measureGrid() {
  const element = grid.value;
  if (!element) return null;
  const width = element.clientWidth;
  if (width <= 0) return null;
  const gap = 3;
  const label = 18;
  // 54 列之间有 53 个间隙, 格子从标签列之后的一个间隙开始。
  geometry = { cell: (width - label - gap * 53) / 53, gap, left: label + gap };
  return geometry;
}

function grow(slot: number, layer: HoverLayer) {
  const target = slots[slot];
  window.clearTimeout(timers[slot]);
  timers[slot] = 0;
  animations[slot]?.stop();
  target.layer.value = layer;
  const run = () => {
    const element = target.element.value;
    if (!element) return;
    animations[slot] = animate(
      element,
      { x: layer.x, y: layer.y, scale: 1 },
      { duration: 0 },
    );
    animations[slot] = animate(
      element,
      { x: layer.x, y: layer.y, scale: 1.45 },
      reduced.value
        ? { duration: 0 }
        : { type: "spring", stiffness: 520, damping: 28 },
    );
  };
  if (target.element.value) run();
  else void nextTick(run);
}

function shrink(slot: number) {
  const target = slots[slot];
  const layer = target.layer.value;
  if (!layer) return;
  const element = target.element.value;
  if (element) {
    animations[slot]?.stop();
    animations[slot] = animate(
      element,
      { x: layer.x, y: layer.y, scale: 1 },
      reduced.value ? { duration: 0 } : { duration: 0.12 },
    );
  }
  window.clearTimeout(timers[slot]);
  timers[slot] = window.setTimeout(
    () => {
      timers[slot] = 0;
      target.layer.value = null;
    },
    reduced.value ? 0 : 130,
  );
}

function showLayer(index: number) {
  const day = days.value[index];
  if (!day) return;
  const box = geometry ?? measureGrid();
  const slot = activeSlot === 0 ? 1 : 0;
  shrink(activeSlot);
  grow(slot, {
    x: box ? box.left + Math.floor(index / 7) * (box.cell + box.gap) : 0,
    y: box ? (index % 7) * (box.cell + box.gap) : 0,
    size: box?.cell ?? 0,
    level: day.level,
  });
  activeSlot = slot;
}

function enter(event: MouseEvent) {
  const target = (event.target as Element | null)?.closest<HTMLElement>(
    ".playtime-heatmap__cell",
  );
  if (!target) {
    leave();
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
  activeCell = target;
  pointer.x = event.clientX;
  pointer.y = event.clientY;
  hovered.value = day;
  showLayer(index);
}
function move(event: MouseEvent) {
  if (!hovered.value) return;
  pointer.x = event.clientX;
  pointer.y = event.clientY;
}
function reset() {
  shrink(activeSlot);
  activeCell = null;
  hovered.value = null;
}
function leave() {
  reset();
}
let observer: ResizeObserver | undefined;
watch(grid, (element) => {
  observer?.disconnect();
  observer = undefined;
  if (!element || typeof ResizeObserver === "undefined") return;
  // 尺寸变化后下一次悬停重新测量格子间距。
  observer = new ResizeObserver(() => {
    geometry = null;
  });
  observer.observe(element);
});
onBeforeUnmount(() => {
  observer?.disconnect();
  observer = undefined;
  timers.forEach((timer) => window.clearTimeout(timer));
  animations.forEach((animation) => animation?.stop());
});
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
      ref="grid"
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
      <span
        v-if="layerA"
        ref="elementA"
        class="playtime-heatmap__cell playtime-heatmap__hover"
        :class="`is-level-${layerA.level}`"
        :style="{ width: `${layerA.size}px`, height: `${layerA.size}px` }"
        aria-hidden="true"
      />
      <span
        v-if="layerB"
        ref="elementB"
        class="playtime-heatmap__cell playtime-heatmap__hover"
        :class="`is-level-${layerB.level}`"
        :style="{ width: `${layerB.size}px`, height: `${layerB.size}px` }"
        aria-hidden="true"
      />
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
.playtime-heatmap__hover {
  position: absolute;
  top: 0;
  left: 0;
  z-index: 2;
  aspect-ratio: auto;
  pointer-events: none;
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
