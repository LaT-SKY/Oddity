<script setup lang="ts">
import { state, showPage } from "../../core/controller";
</script>

<template>
  <aside class="rail">
    <nav class="rail__nav" aria-label="主导航">
      <button
        class="rail-item"
        data-page="home"
        aria-label="启动"
        :class="{ 'is-active': state.page === 'home' }"
        :aria-current="state.page === 'home' ? 'page' : undefined"
        @click="showPage('home')"
      >
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-play"></use>
        </svg>
        <span>启动</span>
      </button>
      <button
        class="rail-item"
        data-page="update"
        aria-label="更新"
        :class="{ 'is-active': state.page === 'update' }"
        :aria-current="state.page === 'update' ? 'page' : undefined"
        @click="showPage('update')"
      >
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-download-simple"></use>
        </svg>
        <span>更新</span>
      </button>
      <button
        class="rail-item"
        data-page="bulletin"
        aria-label="公告"
        :class="{ 'is-active': state.page === 'bulletin' }"
        :aria-current="state.page === 'bulletin' ? 'page' : undefined"
        @click="showPage('bulletin')"
      >
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-newspaper-clipping"></use>
        </svg>
        <span>公告</span>
      </button>
      <button
        class="rail-item"
        data-page="settings"
        aria-label="设置"
        :class="{ 'is-active': state.page === 'settings' }"
        :aria-current="state.page === 'settings' ? 'page' : undefined"
        @click="showPage('settings')"
      >
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-gear-six"></use>
        </svg>
        <span>设置</span>
      </button>
    </nav>
  </aside>
</template>
