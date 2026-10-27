<script setup lang="ts">
import { motion } from "motion-v";
import NavigationRail from "./NavigationRail.vue";
import { ui, showPage, windowAction, dragWindow } from "../../core/controller";
</script>

<template>
  <header class="titlebar" id="titlebar" @mousedown="dragWindow">
    <div class="titlebar__brand" title="Oddity">
      <img
        src="assets/app-icon.svg"
        alt=""
        class="brand-mark"
        draggable="false"
      />
      <span class="brand-word">ODDITY</span>
    </div>
    <NavigationRail />
    <div class="titlebar__right">
      <motion.button
        :while-press="{ scale: 0.88 }"
        class="setup-chip"
        id="setup-chip"
        @click="showPage('settings')"
        type="button"
        :hidden="!ui.setupNeeded"
        :data-state="ui.setupNeeded ? 'warn' : 'ok'"
        :title="ui.setupNeeded ? '启动环境未就绪，打开设置' : ''"
      >
        <i class="status-dot status-dot--warn"></i>
        <span>需要配置</span>
      </motion.button>
      <div class="win-controls">
        <motion.button
          :while-press="{ scale: 0.88 }"
          class="win-btn"
          data-win="minimize"
          @click="windowAction('minimize')"
          aria-label="最小化"
          title="最小化"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-minus"></use>
          </svg>
        </motion.button>
        <motion.button
          :while-press="{ scale: 0.88 }"
          class="win-btn"
          data-win="maximize"
          @click="windowAction('maximize')"
          aria-label="最大化"
          title="最大化"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-square"></use>
          </svg>
        </motion.button>
        <motion.button
          :while-press="{ scale: 0.88 }"
          class="win-btn win-btn--close"
          data-win="close"
          @click="windowAction('close')"
          aria-label="关闭"
          title="关闭"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-x"></use>
          </svg>
        </motion.button>
      </div>
    </div>
  </header>
</template>
