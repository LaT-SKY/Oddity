<script setup lang="ts">
import { motion } from "motion-v";
import {
  state,
  launchView,
  updateView,
  homeDownloadNotice,
  showPage,
  handleLaunchAction,
} from "../core/controller";
import { playerExpanded } from "../composables/useSceneMotion";
import HomeGameRecords from "../components/HomeGameRecords.vue";
import { gamepad } from "../core/gamepad";
import GamepadGlyph from "../components/GamepadGlyph.vue";
</script>

<template>
  <section
    class="page page--home"
    :class="{ 'is-active': state.page === 'home' }"
    id="page-home"
    :inert="state.page !== 'home'"
    aria-labelledby="home-title"
  >
    <div class="hero">
      <div class="hero__inner">
        <HomeGameRecords />
        <motion.div
          class="hero__content"
          :initial="{ opacity: 0, y: 22 }"
          :animate="{
            opacity: state.page !== 'home' ? 0.25 : playerExpanded ? 0.65 : 1,
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
            <span class="hero__cn">{{
              state.bootstrap?.settings.distribution === "global"
                ? "NTE"
                : "异环"
            }}</span>
          </h1>
          <div class="hero__actions">
            <motion.button
              :while-hover="{ scale: 1.025, y: -2 }"
              :while-press="{ scale: 0.96, y: 1 }"
              :transition="{ type: 'spring', stiffness: 460, damping: 26 }"
              class="btn btn--primary btn--trailing"
              id="launch-btn"
              @click="handleLaunchAction"
              type="button"
              :aria-busy="launchView.busy"
              :disabled="launchView.busy"
              :class="{
                'is-busy': launchView.busy,
                'is-running': state.game === 'running',
              }"
            >
              <span class="btn__text">{{ launchView.label }}</span>
              <span class="btn__icon"
                ><GamepadGlyph v-if="gamepad.active" button="west" /><svg
                  v-else
                  class="icon"
                  viewBox="0 0 256 256"
                  aria-hidden="true"
                >
                  <use :href="'#' + launchView.icon"></use></svg
              ></span>
            </motion.button>
          </div>
          <button
            class="hero__status"
            id="home-status"
            @click="showPage('update')"
            type="button"
            :hidden="!homeDownloadNotice.visible && !updateView.showHomeStatus"
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
