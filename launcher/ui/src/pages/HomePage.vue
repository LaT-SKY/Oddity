<script setup lang="ts">
import {
  state,
  launchView,
  updateView,
  showPage,
  checkUpdate,
  handleLaunchAction,
} from "../core/controller";
import HeroMedia from "../components/HeroMedia.vue";
</script>

<template>
  <section
    class="page page--home"
    :class="{ 'is-active': state.page === 'home' }"
    id="page-home"
    aria-labelledby="home-title"
  >
    <div class="hero">
      <div class="hero__inner">
        <HeroMedia />
        <div class="hero__fade" aria-hidden="true"></div>
        <div class="hero__content">
          <h1 class="hero__title" id="home-title">
            <span class="hero__cn">异环</span>
            <span class="hero__en">NEVERNESS TO EVERNESS</span>
          </h1>
          <p class="hero__tagline">一切正常，就是异常。</p>
          <div class="hero__actions">
            <button
              class="btn btn--primary"
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
                ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
                  <use :href="'#' + launchView.icon"></use></svg
              ></span>
            </button>
            <button
              class="btn btn--link"
              id="check-btn"
              :disabled="state.checking"
              :class="{ 'is-busy': state.checking }"
              @click="
                showPage('update');
                checkUpdate(false);
              "
            >
              <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
                <use href="#i-arrows-clockwise"></use>
              </svg>
              <span class="btn__text">检查更新</span>
            </button>
          </div>
          <button
            class="hero__status"
            id="home-status"
            @click="showPage('update')"
            type="button"
            :hidden="!updateView.showHomeStatus"
            :data-state="updateView.homeStatusKind"
          >
            <i class="status-dot status-dot--warn"></i>
            <span id="home-status-text">{{ updateView.homeStatusText }}</span>
          </button>
        </div>
      </div>
    </div>
  </section>
</template>
