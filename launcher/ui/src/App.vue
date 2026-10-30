<script setup lang="ts">
import {
  onBeforeUnmount,
  onMounted,
  watch,
  ref,
  nextTick,
  useTemplateRef,
} from "vue";
import { MotionConfig, motion } from "motion-v";
import { init, ui, form, state, showPage, dispose } from "./core/controller";
import { useSceneMotion, playerExpanded } from "./composables/useSceneMotion";
import { useScenePalette } from "./composables/useScenePalette";
import { navigationScrubbing } from "./composables/navigationGesture";
import NotificationCenter from "./components/NotificationCenter.vue";
import TitleBar from "./components/layout/TitleBar.vue";
import HeroMedia from "./components/HeroMedia.vue";
import HomePage from "./pages/HomePage.vue";
import UpdatePage from "./pages/UpdatePage.vue";
import BulletinPage from "./pages/BulletinPage.vue";
import GameModePage from "./pages/GameModePage.vue";
import PlaytimePage from "./pages/PlaytimePage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import IconSprite from "./components/icons/IconSprite.vue";
import { gamepad, startGamepad } from "./core/gamepad";
import GamepadOverlay from "./components/GamepadOverlay.vue";
import GamepadFilePicker from "./components/GamepadFilePicker.vue";
import GamepadKeyboard from "./components/GamepadKeyboard.vue";
let stopGamepad: (() => void) | undefined;
const scene = useTemplateRef<HTMLElement>("scene");
const drawer = useTemplateRef<HTMLElement>("drawer");
const shade = useTemplateRef<HTMLElement>("shade");
const { opened } = useSceneMotion(scene, drawer, shade);
useScenePalette();
const content = ref<HTMLElement | null>(null);
watch(
  () => state.page,
  async (page, previous) => {
    playerExpanded.value = false;
    const scrubbing = navigationScrubbing.value;
    await nextTick();
    if (scrubbing || navigationScrubbing.value || gamepad.active) return;
    if (page !== "home") {
      content.value
        ?.querySelector<HTMLElement>(".page.is-active h2")
        ?.focus({ preventScroll: true });
    } else if (previous !== "home") {
      document.querySelector<HTMLElement>(`[data-page="${previous}"]`)?.focus();
    }
  },
);
function escape(event: KeyboardEvent) {
  if (event.defaultPrevented || document.querySelector(".modal")) return;
  if (event.key === "Escape" && opened.value) {
    event.preventDefault();
    showPage("home");
  }
}
onMounted(() => {
  void init();
  stopGamepad = startGamepad();
  document.addEventListener("keydown", escape);
});
onBeforeUnmount(() => {
  stopGamepad?.();
  document.removeEventListener("keydown", escape);
  dispose();
});
</script>

<template>
  <IconSprite />
  <MotionConfig :reduced-motion="form.reduceMotion ? 'always' : 'user'">
    <div
      class="app"
      id="app"
      :data-theme="form.theme"
      :class="{
        'is-intro': !ui.introStarted,
        'is-intro-done': ui.introDone,
        'is-maximized': ui.maximized,
        'reduce-motion': form.reduceMotion,
        'has-drawer': opened,
        'gamepad-mode': gamepad.active,
      }"
    >
      <div ref="scene" class="scene" aria-hidden="true">
        <HeroMedia />
        <div class="hero__fade"></div>
      </div>
      <HomePage />
      <div ref="shade" class="scene-shade" aria-hidden="true"></div>
      <TitleBar />
      <button
        v-if="opened"
        class="drawer-dismiss"
        aria-label="返回主页"
        @click="showPage('home')"
      ></button>
      <div
        ref="drawer"
        class="drawer"
        :inert="!opened"
        :aria-hidden="!opened || undefined"
      >
        <motion.button
          class="drawer-close"
          type="button"
          aria-label="关闭页面"
          :while-hover="{ scale: 1.08 }"
          :while-press="{ scale: 0.9 }"
          @click="showPage('home')"
        >
          <svg class="icon" aria-hidden="true"><use href="#i-x" /></svg>
        </motion.button>
        <main ref="content" class="main">
          <UpdatePage />
          <GameModePage />
          <BulletinPage />
          <PlaytimePage />
          <SettingsPage />
        </main>
      </div>
      <NotificationCenter />
      <GamepadFilePicker />
      <GamepadKeyboard />
      <GamepadOverlay />
    </div>
  </MotionConfig>
</template>
