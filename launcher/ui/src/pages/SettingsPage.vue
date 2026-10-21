<script setup lang="ts">
import {
  state,
  ui,
  form,
  applyTheme,
  applyReduceMotion,
  applyHeroMedia,
  saveSettings,
  redetect,
} from "../core/controller";
import BaseSelect from "../components/BaseSelect.vue";

const themeOptions = [
  { label: "亮色（默认）", value: "light" },
  { label: "暗色", value: "dark" },
];

const heroMediaOptions = [
  { label: "动态视频", value: "video" },
  { label: "静态照片", value: "photo" },
];

const launchModeOptions = [
  {
    label: "官方后台桥接（推荐：自动登录，隐藏启动器窗口）",
    value: "background",
  },
  {
    label: "官方启动器桥接（显示启动器窗口，自动登录）",
    value: "official",
  },
  {
    label: "研究直启（无启动器自动登录，仅供调试）",
    value: "direct",
  },
];
</script>

<template>
  <section
    class="page"
    id="page-settings"
    :class="{ 'is-active': state.page === 'settings' }"
    aria-labelledby="settings-title"
  >
    <header class="page-head">
      <div class="page-head__copy">
        <h2 id="settings-title">设置</h2>
      </div>
      <div class="page-head__actions">
        <button
          class="btn btn--link"
          id="settings-redetect"
          :disabled="ui.redetecting"
          :class="{ 'is-busy': ui.redetecting }"
          @click="redetect"
        >
          <span class="btn__text">重新检测</span>
        </button>
        <button
          class="btn btn--primary"
          type="submit"
          form="settings-form"
          id="settings-save"
          :disabled="ui.saving"
          :class="{ 'is-busy': ui.saving }"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-floppy-disk"></use>
          </svg>
          <span class="btn__text">保存设置</span>
        </button>
      </div>
    </header>

    <form class="settings" id="settings-form" @submit.prevent="saveSettings">
      <section class="settings-section">
        <h3>外观</h3>
        <div class="field-grid">
          <div class="field">
            <span>主题</span>
            <BaseSelect
              id="set-theme"
              v-model="form.theme"
              :options="themeOptions"
              @change="applyTheme(form.theme)"
            />
          </div>
          <div class="field">
            <span>主页主视觉</span>
            <BaseSelect
              id="set-hero-media"
              v-model="form.heroMedia"
              :options="heroMediaOptions"
              @change="applyHeroMedia(form.heroMedia)"
            />
          </div>
        </div>
        <label class="switch-row">
          <span
            ><strong>减少动效</strong
            ><small>关闭入场与页面过渡，只保留状态反馈。</small></span
          >
          <input
            id="set-motion"
            v-model="form.reduceMotion"
            @change="
              applyReduceMotion(form.reduceMotion);
              applyHeroMedia(form.heroMedia);
            "
            type="checkbox"
          />
          <i class="switch" aria-hidden="true"></i>
        </label>
      </section>

      <section class="settings-section">
        <h3>安装与运行</h3>
        <div class="field-grid">
          <label class="field field--wide">
            <span>游戏安装目录</span>
            <input
              id="set-install"
              v-model="form.installRoot"
              type="text"
              spellcheck="false"
              autocomplete="off"
              placeholder="/path/to/Neverness To Everness"
            />
          </label>
          <label class="field">
            <span>官方启动器桥接目录</span>
            <input
              id="set-bridge"
              v-model="form.bridgeRoot"
              type="text"
              spellcheck="false"
              autocomplete="off"
              placeholder="~/.local/share/oddity/winroot"
            />
          </label>
          <label class="field">
            <span>Wine prefix</span>
            <input
              id="set-prefix"
              v-model="form.prefix"
              type="text"
              spellcheck="false"
              autocomplete="off"
              placeholder="~/.local/share/oddity/prefix"
            />
          </label>
          <label class="field field--wide">
            <span>Proton 路径</span>
            <input
              id="set-proton"
              v-model="form.protonPath"
              type="text"
              spellcheck="false"
              autocomplete="off"
              placeholder="~/.local/share/Steam/compatibilitytools.d/GE-Proton..."
            />
          </label>
          <div class="field">
            <span>启动方式</span>
            <BaseSelect
              id="set-mode"
              v-model="form.launchMode"
              :options="launchModeOptions"
            />
          </div>
        </div>
        <label class="switch-row">
          <span
            ><strong>关闭窗口时最小化到托盘</strong
            ><small>关闭主窗口时保持后台运行。</small></span
          >
          <input
            id="set-minimize-to-tray"
            v-model="form.minimizeToTrayOnClose"
            type="checkbox"
          />
          <i class="switch" aria-hidden="true"></i>
        </label>
      </section>
    </form>

    <section class="diagnostics">
      <div class="panel-label">DIAGNOSTICS</div>
      <div id="settings-checks" class="checks">
        <div
          v-for="(item, index) in state.bootstrap?.checks ?? []"
          :key="index"
          class="check-row"
        >
          <i class="check-dot" :class="item.ok ? 'is-ok' : 'is-bad'"></i>
          <div class="check-copy">
            <strong>{{ item.label }}</strong
            ><span>{{ item.detail }}</span>
          </div>
        </div>
      </div>
    </section>
  </section>
</template>
