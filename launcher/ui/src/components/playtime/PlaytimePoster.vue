<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { motion } from "motion-v";
import { save as saveDialog } from "../../core/filePicker";
import BaseButton from "../../components/BaseButton.vue";
import BaseModal from "../../components/BaseModal.vue";
import { invoke, isDesktop } from "../../core/api";
import { formatPlaytime, type PlaytimeStats } from "../../core/playtime";
import { notify } from "../../core/notifications";
import { useControlMotion } from "../../composables/useControlMotion";

const props = defineProps<{
  open: boolean;
  stats: PlaytimeStats;
  theme?: string;
  reduceMotion?: boolean;
}>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();
const { reduced } = useControlMotion();
const reducedMotion = computed(
  () => Boolean(props.reduceMotion) || reduced.value,
);

const generating = ref(false);
const saving = ref(false);
const posterUrl = ref("");
const savedPath = ref("");
const error = ref("");
let posterBlob: Blob | null = null;

// 海报内容只取决于记录本身, 相同记录的再次打开直接复用上一次的结果。
interface PosterCache {
  signature: string;
  blob: Blob;
  url: string;
}
let posterCache: PosterCache | null = null;
// 每次打开或重新生成都会推进轮次, 异步步骤返回后据此丢弃过期结果。
let posterRun = 0;

watch(
  () => props.open,
  async (open) => {
    if (!open) {
      posterRun++;
      generating.value = false;
      posterUrl.value = "";
      savedPath.value = "";
      error.value = "";
      return;
    }
    await nextTick();
    // 先让弹窗和骨架画出一帧, 再开始绘制海报。
    await nextPaint();
    await generatePoster();
  },
  { immediate: true },
);

function nextPaint() {
  return new Promise<void>((resolve) => {
    if (typeof requestAnimationFrame !== "function") {
      resolve();
      return;
    }
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

function posterSignature(stats: PlaytimeStats) {
  const today = new Date();
  return JSON.stringify([
    stats.totalMs,
    stats.sessionCount,
    stats.daysPlayed,
    stats.currentStreak,
    stats.longestStreak,
    stats.longestSessionMs,
    stats.heatmap.start,
    stats.heatmap.weeks.map((week) => week.map((day) => day.ms)),
    stats.months.map((month) => [month.ms, month.sessions]),
    // 海报底部绘制当天日期, 跨日打开需要重新生成。
    `${today.getFullYear()}-${today.getMonth() + 1}-${today.getDate()}`,
  ]);
}

async function generatePoster() {
  if (generating.value || !props.open) return;
  const run = ++posterRun;
  generating.value = true;
  posterUrl.value = "";
  posterBlob = null;
  savedPath.value = "";
  error.value = "";
  const stale = (signature: string) =>
    run !== posterRun ||
    !props.open ||
    signature !== posterSignature(props.stats);
  try {
    let signature = posterSignature(props.stats);
    while (true) {
      if (posterCache?.signature === signature) {
        posterBlob = posterCache.blob;
        posterUrl.value = posterCache.url;
        return;
      }
      await document.fonts?.ready;
      const wallpaper = await loadWallpaper();
      if (stale(signature)) return;
      const canvas = document.createElement("canvas");
      canvas.width = 1080;
      canvas.height = 1440;
      drawPoster(canvas.getContext("2d"), props.stats, wallpaper);
      const blob = await canvasToBlob(canvas);
      if (stale(signature)) return;
      // 绘制期间记录发生变化时, 用最新数据重画。
      if (posterSignature(props.stats) !== signature) {
        signature = posterSignature(props.stats);
        continue;
      }
      const url = URL.createObjectURL(blob);
      // 等图片解码完成再撤下骨架, 避免预览位置闪一帧空白。
      await decodePoster(url);
      if (stale(signature)) {
        URL.revokeObjectURL(url);
        if (!props.open || run !== posterRun) return;
        signature = posterSignature(props.stats);
        continue;
      }
      if (posterCache) URL.revokeObjectURL(posterCache.url);
      posterCache = { signature, blob, url };
      posterBlob = blob;
      posterUrl.value = url;
      return;
    }
  } catch (cause) {
    // 旧轮次的失败不应显示在新一轮的骨架或预览旁。
    if (run === posterRun && props.open)
      error.value = `生成海报失败: ${String(cause)}`;
  } finally {
    if (run === posterRun) generating.value = false;
  }
}

async function decodePoster(url: string) {
  const image = new Image();
  image.src = url;
  try {
    await image.decode?.();
  } catch {
    // 解码失败时仍然展示, 由预览图的加载状态兜底。
  }
}

function canvasToBlob(canvas: HTMLCanvasElement) {
  return new Promise<Blob>((resolve, reject) => {
    canvas.toBlob(
      (blob) => (blob ? resolve(blob) : reject(new Error("无法编码海报"))),
      "image/png",
    );
  });
}

function blobToBase64(blob: Blob) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const result = String(reader.result ?? "");
      resolve(result.slice(result.indexOf(",") + 1));
    };
    reader.onerror = () =>
      reject(reader.error ?? new Error("无法读取海报数据"));
    reader.readAsDataURL(blob);
  });
}

onBeforeUnmount(() => {
  if (posterCache) URL.revokeObjectURL(posterCache.url);
  posterCache = null;
});

function close() {
  if (saving.value) return;
  emit("update:open", false);
}

async function savePoster() {
  if (!posterUrl.value || saving.value) return;
  saving.value = true;
  error.value = "";
  try {
    if (!isDesktop()) {
      const link = document.createElement("a");
      link.href = posterUrl.value;
      link.download = posterName();
      link.style.display = "none";
      document.body.append(link);
      link.click();
      link.remove();
      notify("预览模式: 已下载海报.", "ok", 3400, {
        key: "playtime-poster",
      });
      return;
    }
    const destination = await saveDialog({
      title: "保存游玩记录海报",
      defaultPath: posterName(),
      filters: [{ name: "PNG 图片", extensions: ["png"] }],
    });
    if (!destination) return;
    if (!posterBlob) throw new Error("海报尚未生成");
    const data = await blobToBase64(posterBlob);
    savedPath.value = await invoke("playtime_export_poster", {
      destination,
      data,
    });
    notify("海报已保存.", "ok", 3400, { key: "playtime-poster" });
  } catch (cause) {
    error.value = `保存海报失败: ${String(cause)}`;
    notify(error.value, "error", 6500, { key: "playtime-poster" });
  } finally {
    saving.value = false;
  }
}

function posterName() {
  const date = new Date();
  return `异环游玩记录-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}.png`;
}

const imageRequests = new Map<string, Promise<HTMLImageElement | null>>();
let wallpaperRequest: Promise<HTMLImageElement | null> | null = null;

function loadImage(src: string, anonymous = false) {
  const key = `${anonymous ? "cors" : "same"}:${src}`;
  const existing = imageRequests.get(key);
  if (existing) return existing;
  const request = new Promise<HTMLImageElement | null>((resolve) => {
    const image = new Image();
    if (anonymous) image.crossOrigin = "anonymous";
    image.onload = () => resolve(image);
    image.onerror = () => resolve(null);
    image.src = src;
  });
  imageRequests.set(key, request);
  return request;
}

function loadWallpaper() {
  if (wallpaperRequest) return wallpaperRequest;
  wallpaperRequest = (async () => {
    if (isDesktop()) {
      try {
        const media = await invoke("launcher_background");
        if (media.image) {
          const official = await loadImage(media.image, true);
          if (official) return official;
        }
      } catch {}
    }
    return loadImage(
      new URL("assets/hero-keyart.jpg", window.location.href).href,
    );
  })();
  return wallpaperRequest;
}

function drawImageCover(
  ctx: CanvasRenderingContext2D,
  image: HTMLImageElement,
  width: number,
  height: number,
  focusX = 0.5,
  focusY = 0.5,
) {
  const scale = Math.max(
    width / image.naturalWidth,
    height / image.naturalHeight,
  );
  const drawWidth = image.naturalWidth * scale;
  const drawHeight = image.naturalHeight * scale;
  ctx.drawImage(
    image,
    (width - drawWidth) * focusX,
    (height - drawHeight) * focusY,
    drawWidth,
    drawHeight,
  );
}

function drawPoster(
  context: CanvasRenderingContext2D | null,
  stats: PlaytimeStats,
  wallpaper: HTMLImageElement | null,
) {
  if (!context) throw new Error("无法创建画布");
  const ctx = context;
  const width = 1080;
  const height = 1440;
  const padding = 96;
  const contentWidth = width - padding * 2;
  const font =
    '"Space Grotesk", "Noto Sans CJK SC", "Noto Sans SC", sans-serif';

  const heroHeight = 600;
  ctx.fillStyle = "#0b1016";
  ctx.fillRect(0, 0, width, height);
  if (wallpaper) {
    drawImageCover(ctx, wallpaper, width, heroHeight);
    const topShade = ctx.createLinearGradient(0, 0, 0, 180);
    topShade.addColorStop(0, "rgba(4, 8, 12, 0.52)");
    topShade.addColorStop(1, "rgba(4, 8, 12, 0)");
    ctx.fillStyle = topShade;
    ctx.fillRect(0, 0, width, 180);
    const side = ctx.createLinearGradient(0, 0, width * 0.78, 0);
    side.addColorStop(0, "rgba(4, 8, 12, 0.74)");
    side.addColorStop(0.58, "rgba(4, 8, 12, 0.36)");
    side.addColorStop(1, "rgba(4, 8, 12, 0)");
    ctx.fillStyle = side;
    ctx.fillRect(0, 0, width * 0.78, heroHeight);
    const fade = ctx.createLinearGradient(0, heroHeight - 150, 0, heroHeight);
    fade.addColorStop(0, "rgba(11, 16, 22, 0)");
    fade.addColorStop(0.68, "rgba(11, 16, 22, 0.82)");
    fade.addColorStop(1, "#0b1016");
    ctx.fillStyle = fade;
    ctx.fillRect(0, heroHeight - 150, width, 150);
  } else {
    const glow = ctx.createRadialGradient(820, 120, 20, 820, 120, 760);
    glow.addColorStop(0, "rgba(89, 121, 151, 0.34)");
    glow.addColorStop(1, "rgba(11, 16, 22, 0)");
    ctx.fillStyle = glow;
    ctx.fillRect(0, 0, width, heroHeight);
  }

  ctx.textBaseline = "alphabetic";
  ctx.save();
  ctx.shadowColor = "rgba(0, 0, 0, 0.48)";
  ctx.shadowBlur = 20;
  ctx.shadowOffsetY = 2;
  ctx.fillStyle = "#f2f6fb";
  ctx.font = `600 22px ${font}`;
  ctx.fillText("O D D I T Y", padding, 92);
  ctx.fillStyle = "#8294a7";
  ctx.font = `500 20px ${font}`;
  ctx.textAlign = "right";
  ctx.fillText("NTE / PLAY REPORT", width - padding, 92);
  ctx.textAlign = "left";

  ctx.fillStyle = "#9cabbc";
  ctx.font = `500 22px ${font}`;
  ctx.fillText(rangeLabel(stats), padding, 168);
  ctx.fillStyle = "#edf2f8";
  ctx.font = `700 72px ${font}`;
  ctx.fillText("游玩记录", padding, 260);

  ctx.fillStyle = "#9cabbc";
  ctx.font = `500 22px ${font}`;
  ctx.fillText("累计游玩", padding, 340);
  ctx.fillStyle = "#edf2f8";
  ctx.font = `600 76px ${font}`;
  ctx.fillText(formatPlaytime(stats.totalMs), padding, 426);
  ctx.restore();

  const metrics = [
    { label: "游玩天数", value: `${stats.daysPlayed} 天` },
    { label: "最近连续", value: `${stats.currentStreak} 天` },
    { label: "最长连续", value: `${stats.longestStreak} 天` },
    { label: "单次最长", value: formatPlaytime(stats.longestSessionMs) },
    { label: "游玩场次", value: `${stats.sessionCount} 次` },
  ];
  roundedRect(ctx, padding, 630, contentWidth, 104, 16);
  ctx.fillStyle = "rgba(5, 11, 17, 0.42)";
  ctx.fill();
  ctx.strokeStyle = "rgba(227, 236, 248, 0.12)";
  ctx.lineWidth = 1;
  ctx.stroke();
  const metricInset = 28;
  const metricWidth = (contentWidth - metricInset * 2) / metrics.length;
  metrics.forEach((metric, index) => {
    const x = padding + metricInset + metricWidth * index;
    ctx.fillStyle = "#9cabbc";
    ctx.font = `500 20px ${font}`;
    ctx.fillText(metric.label, x, 668);
    ctx.fillStyle = "#f2f6fb";
    ctx.font = `600 26px ${font}`;
    ctx.fillText(metric.value, x, 708);
  });

  roundedRect(ctx, padding, 760, contentWidth, 240, 16);
  ctx.fillStyle = "rgba(5, 11, 17, 0.46)";
  ctx.fill();
  ctx.strokeStyle = "rgba(227, 236, 248, 0.12)";
  ctx.stroke();
  ctx.fillStyle = "#edf2f8";
  ctx.font = `600 24px ${font}`;
  ctx.fillText("最近 12 个月", padding + 28, 800);
  ctx.fillStyle = "#b3c0cf";
  ctx.font = `500 20px ${font}`;
  ctx.textAlign = "right";
  ctx.fillText(`已游玩 ${heatmapDays(stats)} 天`, width - padding - 28, 800);
  ctx.textAlign = "left";

  const cell = 12;
  const gap = 4;
  const gridX = padding + 34;
  const gridY = 850;
  const weekdays = ["一", "", "三", "", "五", "", "日"];
  ctx.font = `500 14px ${font}`;
  ctx.textAlign = "center";
  ctx.fillStyle = "#8294a7";
  weekdays.forEach((label, index) => {
    if (label)
      ctx.fillText(label, padding + 14, gridY + index * (cell + gap) + 11);
  });
  ctx.textAlign = "left";
  stats.heatmap.weeks.forEach((week, column) => {
    week.forEach((day, row) => {
      const x = gridX + column * (cell + gap);
      const y = gridY + row * (cell + gap);
      if (day.future) return;
      ctx.fillStyle = heatColor(day.level);
      roundedRect(ctx, x, y, cell, cell, 3);
      ctx.fill();
    });
  });
  stats.heatmap.months.forEach((month) => {
    const x = gridX + month.column * (cell + gap);
    ctx.fillStyle = "#8294a7";
    ctx.font = `500 14px ${font}`;
    ctx.fillText(month.label, x, gridY - 12);
  });

  ctx.fillStyle = "#edf2f8";
  ctx.font = `600 24px ${font}`;
  ctx.fillText("月度时长", padding, 1035);
  drawMonthlyChart(ctx, stats, padding, 1053, contentWidth, 250, font);

  ctx.strokeStyle = "rgba(227, 236, 248, 0.14)";
  ctx.beginPath();
  ctx.moveTo(padding, 1335);
  ctx.lineTo(width - padding, 1335);
  ctx.stroke();
  ctx.fillStyle = "#d5e0ec";
  ctx.font = `600 20px ${font}`;
  ctx.fillText("Oddity", padding, 1380);
  ctx.fillStyle = "#8294a7";
  ctx.textAlign = "right";
  ctx.fillText(
    `${new Date().getFullYear()}.${pad(new Date().getMonth() + 1)}.${pad(new Date().getDate())}`,
    width - padding,
    1380,
  );
  ctx.textAlign = "left";

  if (stats.active && stats.activeMs > 0) {
    ctx.fillStyle = "#d5e0ec";
    ctx.font = `500 18px ${font}`;
    ctx.textAlign = "right";
    ctx.fillText("正在游玩", width - padding, 168);
    ctx.textAlign = "left";
  }
}

function drawMonthlyChart(
  ctx: CanvasRenderingContext2D,
  stats: PlaytimeStats,
  x: number,
  y: number,
  width: number,
  height: number,
  font: string,
) {
  if (!stats.months.length) return;
  const left = x + 76;
  const right = x + width - 24;
  const top = y + 28;
  const bottom = y + height - 40;
  const plotWidth = right - left;
  const plotHeight = bottom - top;
  const max = Math.max(1, ...stats.months.map((month) => month.ms));
  const hour = 60 * 60_000;
  const ceiling = Math.max(hour, Math.ceil(max / hour) * hour);
  ctx.save();
  roundedRect(ctx, x, y, width, height, 14);
  ctx.fillStyle = "rgba(5, 11, 17, 0.46)";
  ctx.fill();
  ctx.strokeStyle = "rgba(227, 236, 248, 0.12)";
  ctx.lineWidth = 1;
  ctx.stroke();
  ctx.textAlign = "right";
  ctx.font = `500 12px ${font}`;
  for (const fraction of [0, 0.5, 1]) {
    const lineY = bottom - plotHeight * fraction;
    ctx.strokeStyle = "rgba(227, 236, 248, 0.1)";
    ctx.beginPath();
    ctx.moveTo(left, lineY);
    ctx.lineTo(right, lineY);
    ctx.stroke();
    ctx.fillStyle = "#8294a7";
    ctx.fillText(
      fraction ? formatPlaytime(ceiling * fraction) : "0",
      left - 12,
      lineY + 4,
    );
  }
  const step =
    stats.months.length > 1 ? plotWidth / (stats.months.length - 1) : 0;
  const points = stats.months.map((month, index) => ({
    month,
    x: left + step * index,
    y: bottom - plotHeight * (month.ms / ceiling),
  }));
  const first = points[0];
  const last = points[points.length - 1];
  const gradient = ctx.createLinearGradient(0, top, 0, bottom);
  gradient.addColorStop(0, "rgba(213, 224, 236, 0.2)");
  gradient.addColorStop(1, "rgba(213, 224, 236, 0)");
  ctx.beginPath();
  ctx.moveTo(first.x, bottom);
  for (const point of points) ctx.lineTo(point.x, point.y);
  ctx.lineTo(last.x, bottom);
  ctx.closePath();
  ctx.fillStyle = gradient;
  ctx.fill();
  ctx.beginPath();
  points.forEach((point, index) => {
    if (index) ctx.lineTo(point.x, point.y);
    else ctx.moveTo(point.x, point.y);
  });
  ctx.strokeStyle = "#d5e0ec";
  ctx.lineWidth = 2;
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  ctx.stroke();
  for (const point of points) {
    ctx.beginPath();
    ctx.arc(point.x, point.y, 3.5, 0, Math.PI * 2);
    ctx.fillStyle = point === last ? "#d5e0ec" : "#10151c";
    ctx.fill();
    ctx.strokeStyle = "#d5e0ec";
    ctx.lineWidth = 2;
    ctx.stroke();
  }
  ctx.textAlign = "center";
  ctx.fillStyle = "#8294a7";
  ctx.font = `500 12px ${font}`;
  for (const point of points) {
    ctx.fillText(point.month.label, point.x, bottom + 24);
  }
  ctx.restore();
}

function heatmapDays(stats: PlaytimeStats) {
  return stats.heatmap.weeks.flat().filter((day) => !day.future && day.ms > 0)
    .length;
}

function heatColor(level: number) {
  return ["#1c2530", "#2a3b4c", "#405c73", "#5d819b", "#8eafc7", "#d5e0ec"][
    Math.max(0, Math.min(5, level))
  ];
}

function rangeLabel(stats: PlaytimeStats) {
  const start = new Date(stats.heatmap.start);
  const end = new Date(Math.min(stats.heatmap.end, Date.now()));
  return `${start.getFullYear()}.${pad(start.getMonth() + 1)} - ${end.getFullYear()}.${pad(end.getMonth() + 1)}`;
}

function roundedRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  const r = Math.min(radius, width / 2, height / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + width, y, x + width, y + height, r);
  ctx.arcTo(x + width, y + height, x, y + height, r);
  ctx.arcTo(x, y + height, x, y, r);
  ctx.arcTo(x, y, x + width, y, r);
  ctx.closePath();
}

function pad(value: number) {
  return String(value).padStart(2, "0");
}
</script>

<template>
  <BaseModal
    :open="open"
    title="游玩记录海报"
    :theme="theme"
    :reduce-motion="reduceMotion"
    @update:open="close"
  >
    <div class="playtime-poster">
      <div
        v-if="generating"
        class="playtime-poster__skeleton"
        role="status"
        aria-label="正在生成海报"
      >
        <motion.div
          class="playtime-poster__skeleton-body"
          aria-hidden="true"
          :animate="{ opacity: reducedMotion ? 0.6 : [0.45, 0.85, 0.45] }"
          :transition="{
            duration: 1.6,
            repeat: reducedMotion ? 0 : Infinity,
          }"
        >
          <span class="playtime-poster__skeleton-hero" />
          <span class="playtime-poster__skeleton-metrics" />
          <span class="playtime-poster__skeleton-heatmap" />
          <span class="playtime-poster__skeleton-chart" />
        </motion.div>
      </div>
      <img
        v-else-if="posterUrl"
        class="playtime-poster__image"
        :src="posterUrl"
        alt="游玩记录海报预览"
      />
      <p v-if="error" class="playtime-poster__error" role="alert">
        {{ error }}
      </p>
      <p v-if="savedPath" class="playtime-poster__saved" role="status">
        已保存至 {{ savedPath }}
      </p>
      <div class="playtime-poster__actions">
        <BaseButton variant="ghost" :disabled="saving" @click="close"
          >取消</BaseButton
        >
        <BaseButton
          icon="i-download-simple"
          :busy="saving"
          :disabled="!posterUrl || generating"
          @click="savePoster"
          >保存图片</BaseButton
        >
      </div>
    </div>
  </BaseModal>
</template>

<style scoped>
.playtime-poster {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.playtime-poster__image {
  display: block;
  width: 100%;
  border-radius: var(--r-md);
  background: #10151c;
}
.playtime-poster__saved {
  margin: 0;
  color: var(--muted);
  font-size: var(--text-sm);
}
.playtime-poster__skeleton {
  position: relative;
  width: 100%;
  aspect-ratio: 1080 / 1440;
  border-radius: var(--r-md);
  background: #10151c;
  overflow: hidden;
}
.playtime-poster__skeleton-body {
  position: absolute;
  inset: 0;
}
.playtime-poster__skeleton-hero,
.playtime-poster__skeleton-metrics,
.playtime-poster__skeleton-heatmap,
.playtime-poster__skeleton-chart {
  position: absolute;
  border: 1px solid rgb(255 255 255 / 9%);
  border-radius: 10px;
  background: rgb(255 255 255 / 6%);
}
.playtime-poster__skeleton-hero {
  inset: 0 0 auto;
  height: 41.7%;
  border: 0;
  border-radius: 0;
  background: rgb(255 255 255 / 10%);
}
.playtime-poster__skeleton-metrics {
  top: 43.75%;
  left: 8.9%;
  right: 8.9%;
  height: 7.2%;
}
.playtime-poster__skeleton-heatmap {
  top: 52.8%;
  left: 8.9%;
  right: 8.9%;
  height: 16.6%;
}
.playtime-poster__skeleton-chart {
  top: 71.9%;
  left: 8.9%;
  right: 8.9%;
  height: 18.6%;
}
.playtime-poster__error {
  margin: 0;
  color: var(--danger);
  font-size: var(--text-sm);
  overflow-wrap: anywhere;
}
.playtime-poster__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
