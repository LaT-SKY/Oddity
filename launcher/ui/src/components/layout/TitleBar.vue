<script setup lang="ts">
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
    <div class="titlebar__right">
      <button
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
      </button>
      <div class="win-controls">
        <button
          class="win-btn"
          data-win="minimize"
          @click="windowAction('minimize')"
          aria-label="最小化"
          title="最小化"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-minus"></use>
          </svg>
        </button>
        <button
          class="win-btn"
          data-win="maximize"
          @click="windowAction('maximize')"
          aria-label="最大化"
          title="最大化"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-square"></use>
          </svg>
        </button>
        <button
          class="win-btn win-btn--close"
          data-win="close"
          @click="windowAction('close')"
          aria-label="关闭"
          title="关闭"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-x"></use>
          </svg>
        </button>
      </div>
    </div>
  </header>
</template>
