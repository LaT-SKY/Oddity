<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import BaseButton from "../../components/BaseButton.vue";
import BaseModal from "../../components/BaseModal.vue";
import { invoke, isTauri } from "../../core/api";
import { formatPlaytime, type PlaytimeStats } from "../../core/playtime";
import { notify } from "../../core/notifications";

const props = defineProps<{
  open: boolean;
  stats: PlaytimeStats;
  theme?: string;
  reduceMotion?: boolean;
}>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();

const generating = ref(false);
const saving = ref(false);
const posterUrl = ref("");
const savedPath = ref("");
const error = ref("");

watch(
  () => props.open,
  async (open) => {
    if (!open) {
      posterUrl.value = "";
      savedPath.value = "";
      error.value = "";
      return;
    }
    await nextTick();
    await generatePoster();
  },
  { immediate: true },
);

async function generatePoster() {
  if (generating.value) return;
  generating.value = true;
  posterUrl.value = "";
  savedPath.value = "";
  error.value = "";
  try {
    await document.fonts?.ready;
    const wallpaper = await loadWallpaper();
    const canvas = document.createElement("canvas");
    canvas.width = 1080;
    canvas.height = 1440;
    drawPoster(canvas.getContext("2d"), props.stats, wallpaper);
    if (!props.open) return;
    posterUrl.value = canvas.toDataURL("image/png");
  } catch (cause) {
    error.value = `生成海报失败: ${String(cause)}`;
  } finally {
    generating.value = false;
  }
}

function close() {
  if (saving.value) return;
  emit("update:open", false);
}

async function savePoster() {
  if (!posterUrl.value || saving.value) return;
  saving.value = true;
  error.value = "";
  try {
    if (!isTauri()) {
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
    const data = posterUrl.value.split(",", 2)[1] ?? "";
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
    if (isTauri()) {
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
  const padding = 72;
  const contentWidth = width - padding * 2;
  const font =
    '"Space Grotesk", "Noto Sans CJK SC", "Noto Sans SC", sans-serif';

  ctx.fillStyle = "#080d13";
  ctx.fillRect(0, 0, width, height);
  if (wallpaper) {
    drawImageCover(ctx, wallpaper, width, height, 0.62, 0.46);
    const overlay = ctx.createLinearGradient(0, 0, 0, height);
    overlay.addColorStop(0, "rgba(5, 9, 14, 0.18)");
    overlay.addColorStop(0.3, "rgba(5, 9, 14, 0.26)");
    overlay.addColorStop(0.48, "rgba(5, 9, 14, 0.56)");
    overlay.addColorStop(0.64, "rgba(7, 12, 18, 0.9)");
    overlay.addColorStop(1, "rgba(7, 12, 18, 0.99)");
    ctx.fillStyle = overlay;
    ctx.fillRect(0, 0, width, height);
    const side = ctx.createLinearGradient(0, 0, width * 0.7, 0);
    side.addColorStop(0, "rgba(5, 9, 14, 0.34)");
    side.addColorStop(1, "rgba(5, 9, 14, 0)");
    ctx.fillStyle = side;
    ctx.fillRect(0, 0, width * 0.7, height);
  } else {
    const glow = ctx.createRadialGradient(880, 120, 20, 880, 120, 700);
    glow.addColorStop(0, "rgba(89, 121, 151, 0.34)");
    glow.addColorStop(1, "rgba(16, 21, 28, 0)");
    ctx.fillStyle = glow;
    ctx.fillRect(0, 0, width, height);
  }

  ctx.textBaseline = "alphabetic";
  ctx.fillStyle = "#d5e0ec";
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

  const metrics = [
    { label: "游玩天数", value: `${stats.daysPlayed} 天` },
    { label: "最近连续", value: `${stats.currentStreak} 天` },
    { label: "最长连续", value: `${stats.longestStreak} 天` },
    { label: "单次最长", value: formatPlaytime(stats.longestSessionMs) },
    { label: "游玩场次", value: `${stats.sessionCount} 次` },
  ];
  roundedRect(ctx, padding, 486, contentWidth, 118, 16);
  ctx.fillStyle = "rgba(5, 11, 17, 0.42)";
  ctx.fill();
  ctx.strokeStyle = "rgba(227, 236, 248, 0.12)";
  ctx.lineWidth = 1;
  ctx.stroke();
  const metricWidth = contentWidth / metrics.length;
  metrics.forEach((metric, index) => {
    const x = padding + metricWidth * index;
    ctx.fillStyle = "#9cabbc";
    ctx.font = `500 20px ${font}`;
    ctx.fillText(metric.label, x, 528);
    ctx.fillStyle = "#f2f6fb";
    ctx.font = `600 26px ${font}`;
    ctx.fillText(metric.value, x, 572);
  });

  roundedRect(ctx, padding, 640, contentWidth, 252, 16);
  ctx.fillStyle = "rgba(5, 11, 17, 0.46)";
  ctx.fill();
  ctx.strokeStyle = "rgba(227, 236, 248, 0.12)";
  ctx.stroke();
  ctx.fillStyle = "#edf2f8";
  ctx.font = `600 24px ${font}`;
  ctx.fillText("最近 12 个月", padding + 28, 682);
  ctx.fillStyle = "#b3c0cf";
  ctx.font = `500 20px ${font}`;
  ctx.textAlign = "right";
  ctx.fillText(`已游玩 ${heatmapDays(stats)} 天`, width - padding - 28, 682);
  ctx.textAlign = "left";

  const cell = 13;
  const gap = 4;
  const gridX = padding + 34;
  const gridY = 736;
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
  ctx.fillText("月度时长", padding, 930);
  drawMonthlyChart(ctx, stats, padding, 948, contentWidth, 270, font);

  ctx.strokeStyle = "rgba(227, 236, 248, 0.14)";
  ctx.beginPath();
  ctx.moveTo(padding, 1265);
  ctx.lineTo(width - padding, 1265);
  ctx.stroke();
  ctx.fillStyle = "#d5e0ec";
  ctx.font = `600 20px ${font}`;
  ctx.fillText("Oddity", padding, 1315);
  ctx.fillStyle = "#8294a7";
  ctx.textAlign = "right";
  ctx.fillText(
    `${new Date().getFullYear()}.${pad(new Date().getMonth() + 1)}.${pad(new Date().getDate())}`,
    width - padding,
    1315,
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
      <div v-if="generating" class="playtime-poster__status" role="status">
        正在生成海报...
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
.playtime-poster__status,
.playtime-poster__saved {
  margin: 0;
  color: var(--muted);
  font-size: var(--text-sm);
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
