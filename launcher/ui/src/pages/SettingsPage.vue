<script setup lang="ts">
import { computed } from "vue";
import {
  state,
  ui,
  form,
  applyTheme,
  applyReduceMotion,
  applyHeroMedia,
  queueSettingsSave,
  redetect,
  settingsPathsLocked,
} from "../core/controller";
import TaygedoSettings from "../components/TaygedoSettings.vue";
import BaseSelect from "../components/BaseSelect.vue";
import DirectoryField from "../components/DirectoryField.vue";
import LogExport from "../components/LogExport.vue";

const themeOptions = [
  { label: "亮色（默认）", value: "light" },
  { label: "暗色", value: "dark" },
];
const distributionOptions = [
  { label: "国服", value: "cn" },
  { label: "国际服", value: "global" },
];

const heroMediaOptions = [
  { label: "动态视频", value: "video" },
  { label: "静态照片", value: "photo" },
];

const launchModeOptions = [
  {
    label: "后台启动 (推荐)",
    value: "background",
    hint: "等待官方资源检查, 自动登录并启动游戏, 隐藏官方主窗口.",
  },
  {
    label: "前台启动",
    value: "official",
    hint: "显示官方窗口, 请从该窗口点击启动游戏.",
  },
  {
    label: "直接启动 (开发中)",
    value: "direct",
    hint: "跳过官方登录, 开发中.",
  },
];
const launchModeHint = computed(
  () =>
    launchModeOptions.find((option) => option.value === form.launchMode)?.hint,
);
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
          <svg class="icon" aria-hidden="true">
            <use href="#i-arrows-clockwise" /></svg
          ><span class="btn__text">重新检测</span>
        </button>
      </div>
    </header>

    <form
      class="settings"
      id="settings-form"
      :data-save-state="ui.saveState"
      @submit.prevent
    >
      <section class="settings-section">
        <h3>外观</h3>
        <div class="field-grid">
          <div class="field">
            <span>主题</span>
            <BaseSelect
              id="set-theme"
              v-model="form.theme"
              :options="themeOptions"
              @change="
                applyTheme(form.theme);
                queueSettingsSave(0);
              "
            />
          </div>
          <div class="field">
            <span>主页主视觉</span>
            <BaseSelect
              id="set-hero-media"
              v-model="form.heroMedia"
              :options="heroMediaOptions"
              @change="
                applyHeroMedia(form.heroMedia);
                queueSettingsSave(0);
              "
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
              queueSettingsSave(0);
            "
            type="checkbox"
          />
          <i class="switch" aria-hidden="true"></i>
        </label>
      </section>

      <section class="settings-section">
        <h3>安装与运行</h3>
        <p v-if="settingsPathsLocked" class="settings-lock" role="status">
          游戏资源任务进行中，暂时无法修改游戏地区和安装目录。
        </p>
        <div class="field-grid">
          <div class="field">
            <span>游戏地区</span>
            <BaseSelect
              id="set-distribution"
              v-model="form.distribution"
              :options="distributionOptions"
              :disabled="settingsPathsLocked"
              @change="queueSettingsSave(0)"
            />
          </div>
          <DirectoryField
            id="set-install"
            v-model="form.installRoot"
            class="field field--wide"
            label="国服安装目录"
            placeholder="/path/to/Neverness To Everness"
            :disabled="settingsPathsLocked"
            @input="queueSettingsSave()"
            @change="queueSettingsSave(0)"
          />
          <DirectoryField
            id="set-global-install"
            v-model="form.globalInstallRoot"
            class="field field--wide"
            label="国际服安装目录"
            placeholder="~/Games/Neverness To Everness Global"
            :disabled="settingsPathsLocked"
            @input="queueSettingsSave()"
            @change="queueSettingsSave(0)"
          />
          <DirectoryField
            id="set-prefix"
            v-model="form.prefix"
            class="field"
            label="Wine prefix"
            placeholder="~/.local/share/oddity/prefix"
            @input="queueSettingsSave()"
            @change="queueSettingsSave(0)"
          />
          <DirectoryField
            id="set-proton"
            v-model="form.protonPath"
            class="field field--wide"
            label="Proton 路径"
            placeholder="~/.local/share/Steam/compatibilitytools.d/GE-Proton..."
            @input="queueSettingsSave()"
            @change="queueSettingsSave(0)"
          />
          <div class="field">
            <span>启动方式</span>
            <BaseSelect
              id="set-mode"
              v-model="form.launchMode"
              :options="launchModeOptions"
              @change="queueSettingsSave(0)"
            /><small class="field__hint">{{ launchModeHint }}</small>
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
            @change="queueSettingsSave(0)"
          />
          <i class="switch" aria-hidden="true"></i>
        </label>
        <label class="switch-row">
          <span
            ><strong>游戏退出后打开启动器</strong
            ><small>检测到游戏进程结束后，自动显示 Oddity 主界面。</small></span
          >
          <input
            id="set-show-after-game-exit"
            v-model="form.showLauncherAfterGameExit"
            type="checkbox"
            @change="queueSettingsSave(0)"
          />
          <i class="switch" aria-hidden="true"></i>
        </label>
      </section>
      <p v-if="form.distribution === 'global'" class="field__hint">
        塔吉多签到和实时数据仅适用于国服. 国际服请使用官方账号服务.
      </p>
      <TaygedoSettings
        v-if="state.page === 'settings' && form.distribution === 'cn'"
      />
      <LogExport />
    </form>

    <section class="diagnostics">
      <div class="panel-label">运行环境</div>
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
