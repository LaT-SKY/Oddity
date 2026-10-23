<script setup lang="ts">
import { computed } from "vue";
import BaseSelect from "../components/BaseSelect.vue";
import {
  state,
  form,
  downloadBusy,
  downloadView,
  startGameDownload,
  cancelGameDownload,
  checkUpdate,
  updateView,
  formatSize,
} from "../core/controller";
import type { DownloadPhase } from "../types/launcher";

const distributionOptions = [
  { label: "国服", value: "cn" },
  { label: "国际服", value: "global" },
];
const currentRoot = computed({
  get: () =>
    form.distribution === "cn" ? form.installRoot : form.globalInstallRoot,
  set: (value: string) => {
    if (form.distribution === "cn") form.installRoot = value;
    else form.globalInstallRoot = value;
  },
});

const statusMatches = computed(
  () => state.download?.distribution === form.distribution,
);
const active = computed(() => downloadBusy());
const cancellable = computed(() =>
  ["checking", "launcher", "game"].includes(state.download?.phase || ""),
);
const phase = computed<DownloadPhase>(() =>
  statusMatches.value && state.download ? state.download.phase : "idle",
);
const installed = computed(
  () => statusMatches.value && Boolean(state.download?.installed),
);
const downloading = computed(
  () => phase.value === "launcher" || phase.value === "game",
);
const needsFullRepair = computed(
  () =>
    statusMatches.value &&
    phase.value !== "ready" &&
    Boolean(state.download?.full || state.download?.unverifiedFiles),
);

const resolvedRoot = computed(() => {
  const data = state.bootstrap;
  const root = currentRoot.value.trim();
  const saved =
    form.distribution === "cn"
      ? data?.settings.installRoot
      : data?.settings.globalInstallRoot;
  return data?.settings.distribution === form.distribution &&
    root === (saved || data.installRoot)
    ? data.installRoot
    : root;
});

// 检查结果只对后端实际使用的地区和目录有效。
const stale = computed(
  () =>
    state.download &&
    state.download.phase !== "idle" &&
    (state.download.distribution !== form.distribution ||
      state.download.installRoot.replace(/\/+$/, "") !==
        resolvedRoot.value.replace(/\/+$/, "")),
);
const displayPhase = computed<DownloadPhase>(() =>
  stale.value ? "idle" : phase.value,
);
const statusVisible = computed(() => statusMatches.value && !stale.value);

const stateTitle = computed(() =>
  statusVisible.value ? downloadView.value.title : "尚未检查游戏资源",
);
const stateDetail = computed(() =>
  statusVisible.value
    ? downloadView.value.detail
    : "选择地区和安装目录后检查资源",
);
const stateIcon = computed(() => {
  switch (displayPhase.value) {
    case "ready":
      return "i-check-circle";
    case "failed":
      return "i-warning-circle";
    case "cancelled":
      return "i-pause";
    case "needs-install":
    case "needs-update":
    case "needs-verify":
    case "launcher":
    case "game":
      return "i-download-simple";
    default:
      return "i-arrows-clockwise";
  }
});
const stateTone = computed(() => {
  if (displayPhase.value === "idle") return "is-idle";
  if (displayPhase.value === "failed") return "is-error";
  if (
    ["needs-install", "needs-update", "needs-verify", "cancelled"].includes(
      displayPhase.value,
    )
  )
    return "is-warn";
  return "";
});

const primary = computed(() => {
  switch (displayPhase.value) {
    case "checking":
      return {
        kind: "check" as const,
        label: "正在检查",
        icon: "i-arrows-clockwise",
        busy: true,
      };
    case "launcher":
    case "game":
      return {
        kind: "apply" as const,
        label: installed.value ? "正在更新游戏" : "正在安装游戏",
        icon: "i-download-simple",
        busy: true,
      };
    case "needs-install":
    case "needs-update":
    case "needs-verify":
      return {
        kind: "apply" as const,
        label: installed.value ? "更新游戏" : "安装游戏",
        icon: "i-download-simple",
        busy: false,
      };
    case "cancelled":
      if (state.download?.mode === "check")
        return {
          kind: "check" as const,
          label: "重新检查",
          icon: "i-arrows-clockwise",
          busy: false,
        };
      return {
        kind: "apply" as const,
        label: installed.value ? "继续更新游戏" : "继续安装游戏",
        icon: "i-download-simple",
        busy: false,
      };
    case "ready":
      return {
        kind: "check" as const,
        label: "检查通过",
        icon: "i-check-circle",
        busy: false,
      };
    case "failed":
      return {
        kind: "check" as const,
        label: "重新检查",
        icon: "i-arrows-clockwise",
        busy: false,
      };
    default:
      return {
        kind: "check" as const,
        label: "检查资源",
        icon: "i-arrows-clockwise",
        busy: false,
      };
  }
});

function runPrimary() {
  if (primary.value.busy || primaryDisabled.value) return;
  if (primary.value.kind === "apply")
    void startGameDownload("apply", needsFullRepair.value);
  else void startGameDownload("check", false);
}

const primaryDisabled = computed(
  () =>
    active.value || (state.game !== "idle" && primary.value.kind === "apply"),
);
const showProgress = computed(
  () =>
    statusVisible.value &&
    state.download !== null &&
    phase.value !== "checking" &&
    (active.value || state.download.pendingFiles > 0),
);
// 检查完成但尚未开始时只展示体积摘要，不显示 0% 进度条。
const transferActive = computed(
  () => downloading.value || phase.value === "cancelled",
);
</script>

<template>
  <section
    class="page page--update"
    :class="{ 'is-active': state.page === 'update' }"
    id="page-update"
    aria-labelledby="update-title"
  >
    <header class="page-head">
      <div class="page-head__copy">
        <h2 id="update-title">游戏更新</h2>
      </div>
    </header>

    <div
      class="update-card game-download-card"
      :class="[stateTone, { 'is-checking': displayPhase === 'checking' }]"
    >
      <h3 class="update-card__title">安装与更新</h3>
      <div class="update-state">
        <span class="update-icon" aria-hidden="true"
          ><svg class="icon" viewBox="0 0 256 256">
            <use :href="'#' + stateIcon" /></svg
        ></span>
        <div class="update-state__copy">
          <h3
            id="game-download-state-title"
            aria-live="polite"
            aria-atomic="true"
          >
            {{ stateTitle }}
          </h3>
          <p>{{ stateDetail }}</p>
        </div>
      </div>
      <div class="download-fields">
        <div class="field">
          <span>游戏地区</span>
          <BaseSelect
            id="download-distribution"
            v-model="form.distribution"
            :options="distributionOptions"
            :disabled="active"
          />
        </div>
        <label class="field field--wide">
          <span>安装目录</span>
          <input
            id="download-install-root"
            v-model="currentRoot"
            type="text"
            spellcheck="false"
            autocomplete="off"
            :disabled="active"
            :placeholder="
              form.distribution === 'cn'
                ? '~/Games/Neverness To Everness'
                : '~/Games/Neverness To Everness Global'
            "
          />
        </label>
      </div>
      <p class="download-hint">
        将从当前地区的官方 CDN 获取启动器文件和游戏资源.
        启动器文件就绪后会继续检查游戏资源, 未完成的文件可在下次下载时继续.
      </p>
      <div class="update-actions">
        <button
          class="btn btn--primary"
          id="game-download-primary"
          type="button"
          :disabled="primaryDisabled"
          :class="{
            'is-busy': active,
            'is-spinning': displayPhase === 'checking',
            'is-passed': displayPhase === 'ready',
          }"
          :title="displayPhase === 'ready' ? '再次检查游戏资源' : undefined"
          @click="runPrimary"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use :href="'#' + primary.icon" />
          </svg>
          <span class="btn__text">{{ primary.label }}</span>
        </button>
        <button
          class="btn btn--link"
          type="button"
          :disabled="active"
          @click="startGameDownload('check', true)"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-shield-check" />
          </svg>
          <span class="btn__text">完整校验</span>
        </button>
        <button
          v-if="cancellable"
          class="btn btn--link"
          type="button"
          @click="cancelGameDownload"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-pause" />
          </svg>
          <span class="btn__text">{{
            state.download?.phase === "checking" ? "取消检查" : "暂停下载"
          }}</span>
        </button>
      </div>
      <div v-if="showProgress && state.download" class="download-progress">
        <div v-if="transferActive" class="download-progress__meta">
          <span
            >{{ state.download.completedFiles }} /
            {{ state.download.totalFiles }} 个文件已完成</span
          >
          <span>已下载 {{ downloadView.downloadedText }}</span>
        </div>
        <div
          v-if="downloadView.sizeText"
          class="download-progress__meta download-progress__meta--size"
        >
          <span>{{ downloadView.sizeText }}</span>
          <span v-if="downloadView.splitText">{{
            downloadView.splitText
          }}</span>
        </div>
        <progress
          v-if="transferActive"
          :value="downloadView.progress"
          max="100"
          :aria-label="'资源处理进度 ' + downloadView.progress + '%'"
        />
        <p
          v-if="transferActive && downloadView.stageText"
          class="download-progress__stage"
          :title="state.download.currentFile"
        >
          正在{{ downloadView.stageText
          }}<template v-if="state.download.currentFile"
            >: {{ state.download.currentFile }}</template
          >
        </p>
        <p
          v-else-if="transferActive && state.download.currentFile"
          :title="state.download.currentFile"
        >
          {{ state.download.currentFile }}
        </p>
        <p
          v-if="
            state.download.currentFile && state.download.currentFileBytes > 0
          "
          class="download-progress__file-size"
        >
          当前文件 {{ formatSize(state.download.currentFileBytes) }}
        </p>
        <p v-if="state.download.remoteVersion">
          目标游戏版本 {{ state.download.remoteVersion }}
        </p>
      </div>
    </div>

    <div class="update-card" :class="'is-' + updateView.kind" id="update-card">
      <h3 class="update-card__title">启动器文件校验</h3>
      <div class="update-state">
        <span class="update-icon" aria-hidden="true"
          ><svg class="icon" viewBox="0 0 256 256">
            <use :href="'#' + updateView.icon" /></svg
        ></span>
        <div class="update-state__copy">
          <h3 id="update-state-title">{{ updateView.title }}</h3>
          <p id="update-state-text">{{ updateView.copy }}</p>
        </div>
      </div>
      <div class="update-actions">
        <button
          class="btn btn--link"
          id="update-quick"
          :disabled="state.checking || active"
          :class="{ 'is-busy': state.checking }"
          @click="checkUpdate(false)"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-arrows-clockwise" />
          </svg>
          <span class="btn__text">检查启动器文件</span>
        </button>
        <button
          class="btn btn--link"
          id="update-full"
          :disabled="state.checking || active"
          @click="checkUpdate(true)"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-shield-check" />
          </svg>
          <span class="btn__text">完整校验启动器</span>
        </button>
      </div>
      <div
        class="update-files"
        id="update-files"
        :hidden="!updateView.showFiles"
      >
        <div class="update-files__head">
          <span>待处理文件</span
          ><span id="update-count">{{ updateView.pending }}</span>
        </div>
        <div class="update-files__body" id="update-file-list">
          <div
            v-for="(row, index) in updateView.rows"
            :key="index"
            class="update-file"
          >
            <span class="update-file__tag">{{ row.tag }}</span
            ><span class="update-file__path" :title="row.path">{{
              row.path
            }}</span>
          </div>
          <div v-if="updateView.remaining > 0" class="update-file__more">
            另有 {{ updateView.remaining }} 个文件未展开
          </div>
        </div>
      </div>
    </div>
  </section>
</template>
