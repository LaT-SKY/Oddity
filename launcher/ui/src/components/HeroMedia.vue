<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { heroEnded, media, setHeroVideo, state } from "../core/controller";

const video = ref<HTMLVideoElement | null>(null);
const canvas = ref<HTMLCanvasElement | null>(null);
const ready = ref(false);
const playing = computed(() => state.page === "home" && media.looping);
let context: CanvasRenderingContext2D | null = null;
let frame = 0;
let lastFrame = -1;

function paint(timestamp: number) {
  frame = requestAnimationFrame(paint);
  // 素材为 30fps, 避免重复拷贝同一帧。
  const currentFrame = Math.floor((timestamp * 30) / 1000);
  if (currentFrame === lastFrame) return;
  lastFrame = currentFrame;
  const source = video.value;
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
          (source.videoWidth - cropWidth) * 0.54,
          (source.videoHeight - cropHeight) * 0.38,
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
      // 素材尾部有 1 秒静帧垫, 在进入尾垫时回跳, 避免等待解码器排空。
      if (
        Number.isFinite(source.duration) &&
        source.duration > 1 &&
        source.currentTime >= source.duration - 1
      )
        heroEnded();
    }
  }
}

function syncPainting() {
  cancelAnimationFrame(frame);
  ready.value = false;
  if (playing.value && context) frame = requestAnimationFrame(paint);
}
watch(playing, syncPainting, { flush: "sync" });
onMounted(() => {
  context = canvas.value!.getContext("2d", { alpha: false });
  setHeroVideo(video.value);
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
    :class="{ 'hero__art--video': state.heroMedia === 'video' }"
    aria-hidden="true"
  ></div>
  <video
    id="hero-video"
    ref="video"
    :src="media.url || undefined"
    :preload="media.url ? 'auto' : 'none'"
    :autoplay="playing"
    muted
    @ended="heroEnded"
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
