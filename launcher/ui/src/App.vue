<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { init, ui, form, dispose } from "./core/controller";
import TitleBar from "./components/layout/TitleBar.vue";
import NavigationRail from "./components/layout/NavigationRail.vue";
import HomePage from "./pages/HomePage.vue";
import UpdatePage from "./pages/UpdatePage.vue";
import BulletinPage from "./pages/BulletinPage.vue";
import SettingsPage from "./pages/SettingsPage.vue";
import IconSprite from "./components/icons/IconSprite.vue";
const root = ref<HTMLElement | null>(null);
onMounted(() => {
  // 先提交隐藏的入场布局, 再由状态开启过渡。
  void root.value?.getBoundingClientRect();
  void init();
});
onBeforeUnmount(dispose);
</script>

<template>
  <IconSprite />
  <div
    ref="root"
    class="app"
    id="app"
    :data-theme="form.theme"
    :class="{
      'is-intro': !ui.introStarted,
      'is-intro-done': ui.introDone,
      'is-maximized': ui.maximized,
      'reduce-motion': form.reduceMotion,
    }"
  >
    <div class="grain" aria-hidden="true"></div>
    <TitleBar />

    <div class="shell">
      <NavigationRail />

      <main class="main">
        <div
          class="scroll-edge scroll-edge--top"
          :class="{ 'is-visible': ui.scrollEdge.top }"
          aria-hidden="true"
        ></div>

        <HomePage />

        <UpdatePage />

        <BulletinPage />

        <SettingsPage />

        <div
          class="scroll-edge scroll-edge--bottom"
          :class="{ 'is-visible': ui.scrollEdge.bottom }"
          aria-hidden="true"
        ></div>
      </main>
    </div>
  </div>

  <div
    class="toast"
    id="toast"
    role="status"
    aria-live="polite"
    :data-theme="form.theme"
    :class="{
      'is-visible': ui.toast.visible,
      'is-error': ui.toast.kind === 'error',
    }"
  >
    {{ ui.toast.message }}
  </div>
</template>
