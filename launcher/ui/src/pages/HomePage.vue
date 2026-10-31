<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { AnimatePresence, motion, useReducedMotion } from "motion-v";
import { createTimeline } from "animejs";
import {
  state,
  form,
  launchView,
  updateView,
  homeDownloadNotice,
  showPage,
  handleLaunchAction,
  pauseLaunch,
} from "../core/controller";
import { playerExpanded } from "../composables/useSceneMotion";
import HomeGameRecords from "../components/HomeGameRecords.vue";
import { gamepad } from "../core/gamepad";
import GamepadGlyph from "../components/GamepadGlyph.vue";
import BaseButton from "../components/BaseButton.vue";
const isActive = computed(() => state.page === "home");
const prefersReduced = useReducedMotion();
const reduced = computed(() => form.reduceMotion || prefersReduced.value);
const gameName = computed(() =>
  state.bootstrap?.settings.distribution === "global" ? "NTE" : "异环",
);

// 文案切换沿用 GamepadHints 的 Anime.js 编排: 先模糊退出, 再换字。
const label = computed(() => launchView.value.label);
const displayedLabel = ref(label.value);
const labelVisible = ref(true);
let labelTimeline: ReturnType<typeof createTimeline> | undefined;
watch(label, (next) => {
  labelTimeline?.cancel();
  if (next === displayedLabel.value) {
    labelVisible.value = true;
    return;
  }
  if (reduced.value) {
    displayedLabel.value = next;
    labelVisible.value = true;
    return;
  }
  labelVisible.value = false;
  labelTimeline = createTimeline().call(() => {
    displayedLabel.value = next;
    labelVisible.value = true;
  }, 140);
});
onBeforeUnmount(() => labelTimeline?.cancel());

function handleMainLaunchClick() {
  if (launchView.value.disabled) return;
  handleLaunchAction();
}
</script>

<template>
  <section
    class="page page--home"
    :class="{ 'is-active': isActive }"
    id="page-home"
    :inert="!isActive"
    aria-labelledby="home-title"
  >
    <div class="hero">
      <div class="hero__inner">
        <HomeGameRecords />
        <motion.div
          class="hero__content"
          :initial="{ opacity: 0, y: 22 }"
          :animate="{
            opacity: !isActive ? 0.25 : playerExpanded ? 0.65 : 1,
            y: 0,
          }"
          :transition="{
            type: 'spring',
            stiffness: 180,
            damping: 25,
            delay: 0.1,
          }"
        >
          <h1 class="hero__title" id="home-title">
            <span class="hero__cn">{{ gameName }}</span>
          </h1>
          <div class="hero__actions">
            <div
              class="hero__launch"
              id="launch-region"
              :data-pad-region="state.game === 'launching' ? '' : undefined"
              :tabindex="state.game === 'launching' ? -1 : undefined"
              :aria-label="
                state.game === 'launching' ? launchView.label : undefined
              "
            >
              <motion.button
                :while-hover="
                  launchView.disabled ? undefined : { scale: 1.025, y: -2 }
                "
                :while-press="
                  launchView.disabled ? undefined : { scale: 0.96, y: 1 }
                "
                :transition="{ type: 'spring', stiffness: 460, damping: 26 }"
                class="btn btn--primary btn--trailing hero__launch-main"
                id="launch-btn"
                @click="handleMainLaunchClick"
                type="button"
                :aria-busy="launchView.busy"
                :aria-disabled="launchView.disabled || undefined"
                :aria-label="launchView.label"
                :class="{
                  'is-busy': launchView.disabled && state.game !== 'launching',
                  'is-launching': state.game === 'launching',
                  'is-running': state.game === 'running',
                }"
              >
                <motion.span
                  class="btn__text"
                  :initial="false"
                  :animate="{
                    opacity: labelVisible ? 1 : 0,
                    filter: labelVisible || reduced ? 'blur(0px)' : 'blur(6px)',
                    y: labelVisible || reduced ? 0 : -4,
                  }"
                  :transition="{
                    duration: reduced ? 0 : 0.14,
                    ease: 'easeOut',
                  }"
                  >{{ displayedLabel }}</motion.span
                >
                <span v-if="state.game !== 'launching'" class="btn__icon"
                  ><GamepadGlyph v-if="gamepad.active" button="west" /><svg
                    v-else
                    class="icon"
                    viewBox="0 0 256 256"
                    aria-hidden="true"
                  >
                    <use :href="'#' + launchView.icon"></use></svg
                ></span>
              </motion.button>
              <AnimatePresence>
                <motion.div
                  v-if="launchView.pauseAvailable"
                  class="hero__launch-sub"
                  :initial="{ opacity: 0, scale: 0.78 }"
                  :animate="{ opacity: 1, scale: 1 }"
                  :exit="{ opacity: 0, scale: 0.78 }"
                  :transition="{
                    duration: reduced ? 0 : 0.16,
                    ease: 'easeOut',
                  }"
                >
                  <BaseButton
                    id="launch-pause-btn"
                    class="hero__launch-pause"
                    variant="secondary"
                    size="sm"
                    icon="i-pause"
                    icon-only
                    data-pad-skip
                    :aria-label="launchView.pausing ? '正在暂停' : '暂停启动'"
                    :busy="launchView.pausing"
                    @click="pauseLaunch"
                  />
                </motion.div>
              </AnimatePresence>
            </div>
          </div>
          <button
            class="hero__status"
            id="home-status"
            @click="showPage('update')"
            type="button"
            :hidden="
              state.game === 'launching' ||
              (!homeDownloadNotice.visible && !updateView.showHomeStatus)
            "
            :data-state="
              homeDownloadNotice.visible
                ? homeDownloadNotice.kind
                : updateView.homeStatusKind
            "
          >
            <i class="status-dot status-dot--warn"></i>
            <span id="home-status-text">{{
              homeDownloadNotice.visible
                ? homeDownloadNotice.text
                : updateView.homeStatusText
            }}</span>
          </button>
        </motion.div>
      </div>
    </div>
  </section>
</template>
