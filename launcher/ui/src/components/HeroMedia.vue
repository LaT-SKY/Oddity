<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { media, setHeroVideo, state } from "../core/controller";

const videoA = ref<HTMLVideoElement | null>(null);
const videoB = ref<HTMLVideoElement | null>(null);
const canvas = ref<HTMLCanvasElement | null>(null);
const ready = ref(false);
const active = ref(0);
const playing = computed(() => state.page === "home" && media.looping);
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
let lastFrame = -1;
let switching = false;

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

function paint(timestamp: number) {
  frame = requestAnimationFrame(paint);
  // 素材为 30fps, 避免重复拷贝同一帧。
  const currentFrame = Math.floor((timestamp * 30) / 1000);
  if (currentFrame === lastFrame) return;
  lastFrame = currentFrame;
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
      source.videoWidth / target.clientWidth,
      source.videoHeight / target.clientHeight,
    );
    const width = Math.round(target.clientWidth * pixelRatio);
    const height = Math.round(target.clientHeight * pixelRatio);
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
  if (!media.url) return;
  for (const video of [videoA.value, videoB.value]) {
    if (!video) continue;
    try {
      video.pause();
      video.currentTime = 0;
      video.load();
    } catch {}
  }
  active.value = 0;
  setHeroVideo(videoA.value);
}

function syncPainting() {
  cancelAnimationFrame(frame);
  if (playing.value && context) frame = requestAnimationFrame(paint);
}
watch(playing, syncPainting, { flush: "sync" });
// 只有换素材时才丢弃最后一帧, 页面切换要保留画面直接续播。
watch(
  () => media.videoKey,
  () => {
    ready.value = false;
    active.value = 0;
    setHeroVideo(videoA.value);
  },
  { flush: "sync" },
);
watch(() => media.url, loadVideos, { flush: "post" });
onMounted(() => {
  context = canvas.value!.getContext("2d", { alpha: false });
  setHeroVideo(videoA.value);
  loadVideos();
  syncPainting();
});
onBeforeUnmount(() => {
  cancelAnimationFrame(frame);
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
    :src="media.url || undefined"
    :preload="media.url ? 'auto' : 'none'"
    :autoplay="playing && active === 0"
    crossorigin="anonymous"
    muted
    @ended="switchLoop"
    playsinline
    hidden
    aria-hidden="true"
  ></video>
  <video
    id="hero-video-b"
    ref="videoB"
    class="hero__source"
    :src="media.url || undefined"
    :preload="media.url ? 'auto' : 'none'"
    :autoplay="playing && active === 1"
    crossorigin="anonymous"
    muted
    @ended="switchLoop"
    playsinline
    hidden
    aria-hidden="true"
  ></video>
  <canvas
    id="hero-canvas"
    ref="canvas"
    class="hero__video"
    :class="{ 'is-visible': playing && ready }"
    aria-hidden="true"
  ></canvas>
</template>
