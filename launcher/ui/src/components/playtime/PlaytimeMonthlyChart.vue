<script setup lang="ts">
import {
  computed,
  onBeforeUnmount,
  onMounted,
  reactive,
  ref,
  useId,
} from "vue";
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
const open = ref(false);
const browsing = ref(false);
const pointer = reactive({ x: 0, y: 0 });
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
const cursorTransition = computed(() =>
  reduced.value
    ? { duration: 0 }
    : { type: "spring" as const, stiffness: 800, damping: 42, mass: 0.5 },
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
// 图表尺寸在悬停期间不变, 每次进入只测量一次, 指针移动不再触发布局。
let bounds: DOMRect | null = null;
const scale = ref(1);
// 与 SVG 的 non-scaling-stroke 一致: 圆点半径随图缩放, 描边固定 3px。
const cursorSize = computed(() => 14 * scale.value + 3);

function updateScale() {
  const box = chart.value?.getBoundingClientRect() ?? null;
  if (box && box.width > 0) scale.value = box.width / width;
  return box;
}
function measure() {
  bounds = updateScale();
  return bounds;
}
let observer: ResizeObserver | undefined;
onMounted(() => {
  updateScale();
  const element = chart.value;
  if (!element || typeof ResizeObserver === "undefined") return;
  // 容器宽度变化时重新换算游标坐标。
  observer = new ResizeObserver(() => {
    updateScale();
  });
  observer.observe(element);
});
onBeforeUnmount(() => {
  observer?.disconnect();
  observer = undefined;
});
function hover(event: MouseEvent) {
  if (!points.value.length) return;
  const box = bounds ?? measure();
  if (!box || box.width <= 0) return;
  const x = ((event.clientX - box.left) / box.width) * width;
  const step = plotWidth / Math.max(1, points.value.length - 1);
  const index = Math.max(
    0,
    Math.min(points.value.length - 1, Math.round((x - left) / step)),
  );
  pointer.x = event.clientX;
  pointer.y = event.clientY;
  open.value = true;
  activeIndex.value = index;
}
function enter(event: MouseEvent) {
  measure();
  hover(event);
}
function leave() {
  browsing.value = false;
  bounds = null;
  open.value = false;
  activeIndex.value = Math.max(0, points.value.length - 1);
}
function keydown(event: KeyboardEvent) {
  if (event.key === "Escape" && browsing.value) {
    event.preventDefault();
    event.stopPropagation();
    leave();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    browsing.value = !browsing.value;
    if (!browsing.value) {
      leave();
      return;
    }
  } else if (
    browsing.value &&
    ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)
  ) {
    event.preventDefault();
    activeIndex.value = Math.max(
      0,
      Math.min(
        points.value.length - 1,
        activeIndex.value +
          (["ArrowLeft", "ArrowUp"].includes(event.key) ? -1 : 1),
      ),
    );
  } else return;
  const box = bounds ?? measure();
  if (box) {
    pointer.x = box.left + (activePoint.value.x / width) * box.width;
    pointer.y = box.top + (activePoint.value.y / height) * box.height;
    open.value = true;
  }
}
</script>

<template>
  <div
    class="playtime-chart"
    tabindex="0"
    data-pad-region
    :data-pad-browsing="browsing || undefined"
    role="group"
    aria-label="月度游玩时长, 确认后使用方向键浏览"
    :aria-describedby="browsing ? `${gradientId}-description` : undefined"
    @keydown="keydown"
    @blur="leave"
  >
    <svg
      ref="chart"
      class="playtime-chart__frame"
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
      <rect
        class="playtime-chart__hit"
        :x="left"
        :y="top"
        :width="plotWidth"
        :height="plotHeight"
        @mouseenter="enter"
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
    <!-- 游标用独立合成层承载, 悬停时只移动图层, 不重绘折线图。 -->
    <div class="playtime-chart__cursor-layer" aria-hidden="true">
      <motion.div
        class="playtime-chart__guide"
        :style="{ top: `${top * scale}px`, height: `${plotHeight * scale}px` }"
        :initial="{ x: initial.x * scale }"
        :animate="{ x: activePoint.x * scale }"
        :transition="cursorTransition"
      />
      <motion.div
        class="playtime-chart__cursor"
        :style="{
          width: `${cursorSize}px`,
          height: `${cursorSize}px`,
          marginLeft: `${-cursorSize / 2}px`,
          marginTop: `${-cursorSize / 2}px`,
        }"
        :initial="{ x: initial.x * scale, y: initial.y * scale }"
        :animate="{ x: activePoint.x * scale, y: activePoint.y * scale }"
        :transition="cursorTransition"
      />
    </div>
    <p
      :id="`${gradientId}-description`"
      class="playtime-chart__sr"
      role="status"
      aria-live="polite"
      aria-atomic="true"
    >
      <template v-if="browsing && activeMonth"
        >{{ activeMonth.label }}, {{ formatPlaytime(activeMonth.ms) }},
        {{ activeMonth.sessions }} 次游玩</template
      >
    </p>
    <FloatingTooltip
      :open="open && activeMonth !== null"
      :pointer="pointer"
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
.playtime-chart__sr {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip-path: inset(50%);
  white-space: nowrap;
}
.playtime-chart {
  position: relative;
  min-width: 0;
}
.playtime-chart__frame {
  display: block;
  width: 100%;
  height: auto;
  overflow: visible;
}
.playtime-chart__overlay {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
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
.playtime-chart__cursor-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
}
.playtime-chart__guide {
  position: absolute;
  top: 0;
  left: 0;
  width: 1px;
  background-image: repeating-linear-gradient(
    to bottom,
    var(--line-strong) 0 3px,
    transparent 3px 7px
  );
  will-change: transform;
}
.playtime-chart__cursor {
  position: absolute;
  top: 0;
  left: 0;
  box-sizing: border-box;
  border: 3px solid var(--surface-solid);
  border-radius: 50%;
  background: #10151c;
  will-change: transform;
}
.playtime-chart__hit {
  fill: transparent;
  cursor: crosshair;
}
</style>
