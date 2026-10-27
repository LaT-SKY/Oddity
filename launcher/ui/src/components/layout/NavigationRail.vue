<script setup lang="ts">
import { motion } from "motion-v";
import { state, showPage } from "../../core/controller";
</script>

<template>
  <aside class="rail">
    <nav class="rail__nav" aria-label="主导航">
      <motion.button
        :while-hover="{ y: -2 }"
        :while-press="{ scale: 0.88 }"
        :transition="{ type: 'spring', stiffness: 460, damping: 25 }"
        class="rail-item"
        data-page="home"
        aria-label="启动"
        title="启动"
        :class="{ 'is-active': state.page === 'home' }"
        :aria-current="state.page === 'home' ? 'page' : undefined"
        @click="showPage('home')"
      >
        <motion.span
          v-if="state.page === 'home'"
          layout-id="navigation-focus"
          class="nav-focus"
          :transition="{ type: 'spring', stiffness: 420, damping: 32 }"
        />
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-play"></use>
        </svg>
      </motion.button>
      <motion.button
        :while-hover="{ y: -2 }"
        :while-press="{ scale: 0.88 }"
        :transition="{ type: 'spring', stiffness: 460, damping: 25 }"
        class="rail-item"
        data-page="update"
        aria-label="更新"
        title="更新"
        :class="{ 'is-active': state.page === 'update' }"
        :aria-current="state.page === 'update' ? 'page' : undefined"
        @click="showPage('update')"
      >
        <motion.span
          v-if="state.page === 'update'"
          layout-id="navigation-focus"
          class="nav-focus"
          :transition="{ type: 'spring', stiffness: 420, damping: 32 }"
        />
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-download-simple"></use>
        </svg>
      </motion.button>
      <motion.button
        :while-hover="{ y: -2 }"
        :while-press="{ scale: 0.88 }"
        :transition="{ type: 'spring', stiffness: 460, damping: 25 }"
        class="rail-item"
        data-page="bulletin"
        aria-label="公告"
        title="公告"
        :class="{ 'is-active': state.page === 'bulletin' }"
        :aria-current="state.page === 'bulletin' ? 'page' : undefined"
        @click="showPage('bulletin')"
      >
        <motion.span
          v-if="state.page === 'bulletin'"
          layout-id="navigation-focus"
          class="nav-focus"
          :transition="{ type: 'spring', stiffness: 420, damping: 32 }"
        />
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-newspaper-clipping"></use>
        </svg>
      </motion.button>
      <motion.button
        :while-hover="{ y: -2 }"
        :while-press="{ scale: 0.88 }"
        :transition="{ type: 'spring', stiffness: 460, damping: 25 }"
        class="rail-item"
        data-page="settings"
        aria-label="设置"
        title="设置"
        :class="{ 'is-active': state.page === 'settings' }"
        :aria-current="state.page === 'settings' ? 'page' : undefined"
        @click="showPage('settings')"
      >
        <motion.span
          v-if="state.page === 'settings'"
          layout-id="navigation-focus"
          class="nav-focus"
          :transition="{ type: 'spring', stiffness: 420, damping: 32 }"
        />
        <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-gear-six"></use>
        </svg>
      </motion.button>
    </nav>
  </aside>
</template>
