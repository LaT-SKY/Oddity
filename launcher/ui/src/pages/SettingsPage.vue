<script setup lang="ts">
import {
  state,
  ui,
  displayVersion,
  form,
  applyTheme,
  applyReduceMotion,
  applyHeroMedia,
  saveSettings,
  redetect,
} from "../core/controller";
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
        <div class="eyebrow">
          <span class="eyebrow__line"></span>SETTINGS / 设置
        </div>
        <h2 id="settings-title">把环境调到顺手</h2>
        <p>路径、Proton 与启动方式。保存后立即生效。</p>
      </div>
      <div class="page-head__actions">
        <button
          class="btn btn--ghost"
          id="settings-redetect"
          :disabled="ui.redetecting"
          :class="{ 'is-busy': ui.redetecting }"
          @click="redetect"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-arrows-clockwise"></use>
          </svg>
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
          <span class="btn__text">保存设置</span>
        </button>
      </div>
    </header>

    <form class="settings" id="settings-form" @submit.prevent="saveSettings">
      <section class="settings-section">
        <h3>外观</h3>
        <div class="field-grid">
          <label class="field">
            <span>主题</span>
            <select
              id="set-theme"
              v-model="form.theme"
              @change="applyTheme(form.theme)"
            >
              <option value="light">亮色（默认）</option>
              <option value="dark">暗色</option>
            </select>
          </label>
          <label class="field">
            <span>主页主视觉</span>
            <select
              id="set-hero-media"
              v-model="form.heroMedia"
              @change="applyHeroMedia(form.heroMedia)"
            >
              <option value="video">动态视频</option>
              <option value="photo">静态照片</option>
            </select>
          </label>
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
          <label class="field">
            <span>启动方式</span>
            <select id="set-mode" v-model="form.launchMode">
              <option value="background">
                官方后台桥接（推荐：自动登录，隐藏启动器窗口）
              </option>
              <option value="official">
                官方启动器桥接（显示启动器窗口，自动登录）
              </option>
              <option value="direct">
                研究直启（无启动器自动登录，仅供调试）
              </option>
            </select>
          </label>
          <p class="settings-hint settings-note">
            Wine prefix / 桥接目录沿用历史路径（软链到
            ~/.local/share/oddity），以保持官方 OneSDK 自动登录票据有效。
          </p>
        </div>
      </section>

      <div class="settings-actions">
        <span class="settings-hint"
          >Oddity <span id="app-version">v{{ displayVersion }}</span> · 设置写入
          ~/.config/oddity/settings.json · Ctrl+S 保存</span
        >
      </div>
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
