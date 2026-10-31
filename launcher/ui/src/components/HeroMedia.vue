<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useReducedMotion } from "motion-v";
import { useDocumentVisibility } from "@vueuse/core";
import {
  applyHeroMedia,
  media,
  setHeroVideo,
  state,
  form,
  ui,
} from "../core/controller";

const videoA = ref<HTMLVideoElement | null>(null);
const videoB = ref<HTMLVideoElement | null>(null);
const canvas = ref<HTMLCanvasElement | null>(null);
const ready = ref(false);
const reduced = useReducedMotion();
const visibility = useDocumentVisibility();
const videoEnabled = computed(
  () =>
    state.heroMedia === "video" &&
    !form.reduceMotion &&
    !reduced.value &&
    visibility.value === "visible" &&
    ui.windowVisible,
);
const active = ref(0);
const playing = computed(
  () => videoEnabled.value && state.page === "home" && media.looping,
);
const currentVideo = computed(() =>
  active.value === 0 ? videoA.value : videoB.value,
);
const standbyVideo = computed(() =>
  active.value === 0 ? videoB.value : videoA.value,
);
const artStyle = computed(() =>
  media.image ? { backgroundImage: `url("${media.image}")` } : undefined,
);
let context: CanvasRenderingContext2D | null = null;
let frame = 0;
let painting: HTMLVideoElement | null = null;
let resize: ResizeObserver | undefined;
let cssWidth = 0;
let cssHeight = 0;
let switching = false;
let resumeTime = 0;

// 两个 video 交替播放, 在结尾前切到已经预载到首帧的备播元素。
// 单元素 loop 会在回跳时等待解码器 seek, 看起来像少了几帧。
function switchLoop() {
  if (switching) return;
  const outgoing = currentVideo.value;
  const incoming = standbyVideo.value;
  if (!outgoing || !incoming) return;
  switching = true;
  try {
    if (incoming.readyState < 2) {
      try {
        outgoing.currentTime = 0;
      } catch {}
      const retry = outgoing.play();
      if (retry && typeof retry.catch === "function") retry.catch(() => {});
      return;
    }
    incoming.pause();
    try {
      incoming.currentTime = 0;
    } catch {}
    const attempt = incoming.play();
    if (attempt && typeof attempt.catch === "function") attempt.catch(() => {});
    active.value = active.value === 0 ? 1 : 0;
    setHeroVideo(incoming);
    outgoing.pause();
    try {
      outgoing.currentTime = 0;
    } catch {}
  } finally {
    switching = false;
  }
}

function paint() {
  if (painting) frame = painting.requestVideoFrameCallback(paint);
  const source = currentVideo.value;
  const target = canvas.value;
  // 回跳与缓冲期间保留上一帧, 不暴露解码器的空画面。
  if (
    source &&
    target &&
    context &&
    !source.paused &&
    !source.seeking &&
    source.readyState >= 2 &&
    source.videoWidth &&
    source.videoHeight
  ) {
    // 画布分辨率不超过源画面, 避免高 DPI 下的无效放大。
    const pixelRatio = Math.min(
      window.devicePixelRatio,
      source.videoWidth / cssWidth,
      source.videoHeight / cssHeight,
    );
    const width = Math.round(cssWidth * pixelRatio);
    const height = Math.round(cssHeight * pixelRatio);
    if (width > 0 && height > 0) {
      if (target.width !== width || target.height !== height) {
        target.width = width;
        target.height = height;
      }
      // 直接按源画面比例裁切, 不经过原生视频层的 object-fit 缩放。
      const scale = Math.max(
        width / source.videoWidth,
        height / source.videoHeight,
      );
      const cropWidth = width / scale;
      const cropHeight = height / scale;
      try {
        context.drawImage(
          source,
          (source.videoWidth - cropWidth) * media.cropX,
          (source.videoHeight - cropHeight) * media.cropY,
          cropWidth,
          cropHeight,
          0,
          0,
          width,
          height,
        );
      } catch (error) {
        if (error instanceof DOMException && error.name === "InvalidStateError")
          return;
        throw error;
      }
      ready.value = true;
      if (
        Number.isFinite(source.duration) &&
        source.duration > media.loopCut &&
        source.currentTime >= source.duration - media.loopCut
      )
        switchLoop();
    }
  }
}

function loadVideos() {
  for (const video of [videoA.value, videoB.value]) {
    if (!video) continue;
    video.pause();
    video.load();
  }
  active.value = 0;
  setHeroVideo(videoA.value);
}

function syncPainting() {
  if (painting) painting.cancelVideoFrameCallback(frame);
  if (playing.value && !context && canvas.value)
    context = canvas.value.getContext("2d", { alpha: false });
  painting = playing.value && context ? currentVideo.value : null;
  if (painting) frame = painting.requestVideoFrameCallback(paint);
}
// 离页后释放解码器仍保留播放位置, 重新解码时从同一帧续播.
watch(
  videoEnabled,
  (enabled) => {
    if (!enabled)
      resumeTime =
        state.page === "home" ? 0 : (currentVideo.value?.currentTime ?? 0);
  },
  { flush: "sync" },
);
function restorePosition(event: Event) {
  const video = event.currentTarget as HTMLVideoElement;
  if (video === currentVideo.value && resumeTime > 0) {
    video.currentTime = resumeTime;
    resumeTime = 0;
  }
}
watch(playing, syncPainting, { flush: "sync" });
watch(currentVideo, syncPainting, { flush: "post" });
// 只有换素材时才丢弃最后一帧, 页面切换要保留画面直接续播。
watch(
  () => media.videoKey,
  () => {
    ready.value = false;
    if (canvas.value) {
      canvas.value.width = 0;
      canvas.value.height = 0;
    }
    resumeTime = 0;
    active.value = 0;
    setHeroVideo(videoA.value);
  },
  { flush: "sync" },
);
watch(
  [() => media.url, videoEnabled],
  () => {
    loadVideos();
    applyHeroMedia(state.heroMedia);
  },
  { flush: "post" },
);
onMounted(() => {
  const target = canvas.value!;
  resize = new ResizeObserver(([entry]) => {
    if (!entry) return;
    cssWidth = entry.contentRect.width;
    cssHeight = entry.contentRect.height;
  });
  resize.observe(target);
  setHeroVideo(videoA.value);
  loadVideos();
  syncPainting();
});
onBeforeUnmount(() => {
  if (painting) painting.cancelVideoFrameCallback(frame);
  resize?.disconnect();
  setHeroVideo(null);
});
</script>

<template>
  <div
    class="hero__art"
    :class="{
      'hero__art--video': state.heroMedia === 'video',
      'hero__art--official': media.officialImage,
      'hero__art--pending': !media.initialized,
    }"
    :style="artStyle"
    aria-hidden="true"
  ></div>
  <video
    id="hero-video"
    ref="videoA"
    class="hero__source"
    :src="videoEnabled ? media.url || undefined : undefined"
    :preload="videoEnabled && media.url ? 'auto' : 'none'"
    :autoplay="playing && active === 0"
    crossorigin="anonymous"
    muted
    @loadedmetadata="restorePosition"
    @ended="switchLoop"
    playsinline
    hidden
    aria-hidden="true"
  ></video>
  <video
    id="hero-video-b"
    ref="videoB"
    class="hero__source"
    :src="videoEnabled ? media.url || undefined : undefined"
    :preload="videoEnabled && media.url ? 'auto' : 'none'"
    :autoplay="playing && active === 1"
    crossorigin="anonymous"
    muted
    @loadedmetadata="restorePosition"
    @ended="switchLoop"
    playsinline
    hidden
    aria-hidden="true"
  ></video>
  <canvas
    id="hero-canvas"
    ref="canvas"
    width="0"
    height="0"
    class="hero__video"
    :class="{
      'is-visible':
        state.heroMedia === 'video' && ready && !form.reduceMotion && !reduced,
    }"
    aria-hidden="true"
  ></canvas>
</template>
