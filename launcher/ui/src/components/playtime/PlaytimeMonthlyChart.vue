<script setup lang="ts">
import { computed, ref, useId } from "vue";
import { motion } from "motion-v";
import FloatingTooltip from "./FloatingTooltip.vue";
import { useControlMotion } from "../../composables/useControlMotion";
import { formatPlaytime, type PlaytimeMonth } from "../../core/playtime";

const props = defineProps<{
  months: PlaytimeMonth[];
  theme?: string;
}>();
const width = 720;
const height = 220;
const left = 64;
const right = 18;
const top = 18;
const bottom = 30;
const plotWidth = width - left - right;
const plotHeight = height - top - bottom;
const gradientId = `playtime-chart-${useId()}`;
const chart = ref<SVGSVGElement | null>(null);
const { reduced } = useControlMotion();
const hovered = ref<{ x: number; y: number } | null>(null);
const activeIndex = ref(Math.max(0, props.months.length - 1));
const max = computed(() =>
  Math.max(1, ...props.months.map((month) => month.ms)),
);
const ceiling = computed(() => {
  const hour = 60 * 60_000;
  return Math.max(hour, Math.ceil(max.value / hour) * hour);
});
const points = computed(() =>
  props.months.map((month, index) => {
    const x =
      props.months.length > 1
        ? left + (index / (props.months.length - 1)) * plotWidth
        : left + plotWidth / 2;
    const y = top + plotHeight * (1 - month.ms / ceiling.value);
    return { month, x, y };
  }),
);
const initial = computed(
  () => points.value.at(-1) ?? { month: null, x: left, y: top + plotHeight },
);
const activePoint = computed(
  () => points.value[activeIndex.value] ?? initial.value,
);
const activeMonth = computed(() => activePoint.value.month);
const line = computed(() =>
  points.value
    .map((point, index) => `${index ? "L" : "M"}${point.x} ${point.y}`)
    .join(" "),
);
const area = computed(() => {
  const first = points.value[0];
  const last = points.value.at(-1);
  if (!first || !last) return "";
  return `M ${first.x} ${top + plotHeight} L ${points.value
    .map((point) => `${point.x} ${point.y}`)
    .join(" L ")} L ${last.x} ${top + plotHeight} Z`;
});
const grid = [0, 0.5, 1];

function hover(event: MouseEvent) {
  const element = chart.value;
  if (!element || !points.value.length) return;
  const bounds = element.getBoundingClientRect();
  const x = ((event.clientX - bounds.left) / bounds.width) * width;
  const step = plotWidth / Math.max(1, points.value.length - 1);
  const index = Math.max(
    0,
    Math.min(points.value.length - 1, Math.round((x - left) / step)),
  );
  activeIndex.value = index;
  hovered.value = { x: event.clientX, y: event.clientY };
}
function leave() {
  hovered.value = null;
  activeIndex.value = Math.max(0, points.value.length - 1);
}
</script>

<template>
  <div class="playtime-chart">
    <svg
      ref="chart"
      :viewBox="`0 0 ${width} ${height}`"
      role="group"
      aria-label="最近 12 个月月度游玩时长折线图"
    >
      <defs>
        <linearGradient :id="gradientId" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" class="playtime-chart__area-start" />
          <stop offset="1" class="playtime-chart__area-end" />
        </linearGradient>
      </defs>
      <rect
        x="0.5"
        y="0.5"
        :width="width - 1"
        :height="height - 1"
        rx="12"
        class="playtime-chart__plot"
      />
      <g v-for="fraction in grid" :key="fraction">
        <line
          class="playtime-chart__grid"
          :x1="left"
          :x2="width - right"
          :y1="top + plotHeight * (1 - fraction)"
          :y2="top + plotHeight * (1 - fraction)"
        />
        <text
          class="playtime-chart__axis"
          :x="left - 10"
          :y="top + plotHeight * (1 - fraction) + 4"
          text-anchor="end"
        >
          {{ fraction ? formatPlaytime(ceiling * fraction) : "0" }}
        </text>
      </g>
      <path
        class="playtime-chart__area"
        :d="area"
        :fill="`url(#${gradientId})`"
      />
      <path class="playtime-chart__line" :d="line" />
      <g v-for="point in points" :key="point.month.key">
        <circle
          class="playtime-chart__dot"
          :class="{ 'is-current': point === points.at(-1) }"
          :cx="point.x"
          :cy="point.y"
          r="3"
        />
      </g>
      <motion.line
        class="playtime-chart__guide"
        :initial="{ x1: initial.x, x2: initial.x }"
        :animate="{ x1: activePoint.x, x2: activePoint.x }"
        :y1="top"
        :y2="top + plotHeight"
        :transition="
          reduced
            ? { duration: 0 }
            : { type: 'spring', stiffness: 800, damping: 42, mass: 0.5 }
        "
      />
      <motion.circle
        class="playtime-chart__cursor"
        :initial="{ cx: initial.x, cy: initial.y }"
        :animate="{ cx: activePoint.x, cy: activePoint.y }"
        :r="7"
        :transition="
          reduced
            ? { duration: 0 }
            : { type: 'spring', stiffness: 800, damping: 42, mass: 0.5 }
        "
      />
      <rect
        class="playtime-chart__hit"
        :x="left"
        :y="top"
        :width="plotWidth"
        :height="plotHeight"
        @mouseenter="hover"
        @mousemove="hover"
        @mouseleave="leave"
      />
      <text
        v-for="point in points"
        :key="`label-${point.month.key}`"
        class="playtime-chart__axis playtime-chart__month"
        :x="point.x"
        :y="height - 10"
        text-anchor="middle"
      >
        {{ point.month.label }}
      </text>
    </svg>
    <FloatingTooltip
      :open="hovered !== null && activeMonth !== null"
      :x="hovered?.x ?? 0"
      :y="hovered?.y ?? 0"
      :theme="theme"
      :reduce-motion="reduced"
      name="chart"
    >
      <template v-if="activeMonth">
        <strong>{{ activeMonth.label }}</strong>
        <p class="playtime-tooltip__value">
          {{ formatPlaytime(activeMonth.ms) }}
        </p>
        <small class="playtime-tooltip__meta">
          {{ activeMonth.sessions }} 次游玩
        </small>
      </template>
    </FloatingTooltip>
  </div>
</template>

<style scoped>
.playtime-chart {
  position: relative;
  min-width: 0;
}
.playtime-chart svg {
  display: block;
  width: 100%;
  height: auto;
  overflow: visible;
}
.playtime-chart__plot {
  fill: var(--surface-2);
  stroke: var(--line);
}
.playtime-chart__grid {
  stroke: var(--line);
  stroke-width: 1;
  vector-effect: non-scaling-stroke;
}
.playtime-chart__axis {
  fill: var(--faint);
  font-family: var(--font);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
}
.playtime-chart__month {
  font-size: 10px;
}
.playtime-chart__area-start {
  stop-color: var(--signal-strong);
  stop-opacity: 0.2;
}
.playtime-chart__area-end {
  stop-color: var(--signal-strong);
  stop-opacity: 0;
}
.playtime-chart__area {
  stroke: none;
}
.playtime-chart__line {
  fill: none;
  stroke: var(--signal-strong);
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
  vector-effect: non-scaling-stroke;
}
.playtime-chart__dot {
  fill: var(--surface-solid);
  stroke: var(--signal-strong);
  stroke-width: 2;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}
.playtime-chart__dot.is-current {
  fill: var(--signal-strong);
}
.playtime-chart__guide {
  stroke: var(--line-strong);
  stroke-width: 1;
  stroke-dasharray: 3 4;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}
.playtime-chart__cursor {
  fill: #10151c;
  stroke: var(--surface-solid);
  stroke-width: 3;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}
.playtime-chart__hit {
  fill: transparent;
  cursor: crosshair;
}
</style>
