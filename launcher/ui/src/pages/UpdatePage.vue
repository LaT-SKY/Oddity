<script setup lang="ts">
import { motion, AnimatePresence, useSpring } from "motion-v";
import { computed, watch } from "vue";
import BaseButton from "../components/BaseButton.vue";
import BaseView from "../components/BaseView.vue";
import {
  useControlMotion,
  controlSpring,
} from "../composables/useControlMotion";
import BaseSelect from "../components/BaseSelect.vue";
import DirectoryField from "../components/DirectoryField.vue";
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
  diskBudget,
  queueSettingsSave,
  settingsPathsLocked,
} from "../core/controller";
import type { DownloadPhase } from "../types/launcher";

const { surface, reduced } = useControlMotion();
const progress = useSpring(0, controlSpring);
watch(
  () => downloadView.value.progress,
  (value) => {
    if (reduced.value) progress.jump(value / 100);
    else progress.set(value / 100);
  },
  { immediate: true },
);

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
const launcherInstalled = computed(() =>
  Boolean(state.bootstrap?.launcherRoot),
);
const awaitingInstall = computed(
  () => !launcherInstalled.value && !active.value && !state.checking,
);
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
  awaitingInstall.value ? "needs-install" : stale.value ? "idle" : phase.value,
);
const statusVisible = computed(
  () => awaitingInstall.value || (statusMatches.value && !stale.value),
);

const stateTitle = computed(() =>
  awaitingInstall.value
    ? "需要安装启动器文件"
    : statusVisible.value
      ? downloadView.value.title
      : "尚未检查游戏资源",
);
const stateDetail = computed(() =>
  awaitingInstall.value
    ? "启动器文件尚未安装, 完成后将继续检查游戏资源"
    : statusVisible.value
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
    !awaitingInstall.value &&
    statusVisible.value &&
    state.download !== null &&
    phase.value !== "checking" &&
    (active.value || state.download.pendingFiles > 0),
);
// 检查完成但尚未开始时只展示体积摘要，不显示 0% 进度条。
const transferActive = computed(
  () => downloading.value || phase.value === "cancelled",
);
const budgetVisible = computed(
  () =>
    diskBudget.value !== null &&
    statusMatches.value &&
    !stale.value &&
    !active.value &&
    Boolean(state.download?.pendingFiles || state.download?.unverifiedFiles),
);
const isActive = computed(() => state.page === "update");
</script>

<template>
  <motion.section
    class="page page--update"
    :class="{ 'is-active': isActive }"
    id="page-update"
    :inert="!isActive"
    :aria-hidden="!isActive || undefined"
    :initial="false"
    :animate="{
      opacity: isActive ? 1 : 0,
      x: isActive ? 0 : 22,
      visibility: isActive ? 'visible' : 'hidden',
    }"
    :transition="surface"
    aria-labelledby="update-title"
  >
    <header class="page-head update-head">
      <div class="page-head__copy">
        <h2 id="update-title" tabindex="-1">游戏更新</h2>
      </div>
      <div class="update-region">
        <BaseSelect
          id="download-distribution"
          label="游戏地区"
          v-model="form.distribution"
          :options="distributionOptions"
          :disabled="settingsPathsLocked"
          @change="queueSettingsSave(0)"
        />
      </div>
    </header>
    <div class="update-content">
      <section
        class="update-card game-download-card"
        :class="[stateTone, { 'is-checking': displayPhase === 'checking' }]"
        aria-labelledby="game-download-section"
      >
        <h3 id="game-download-section" class="update-card__title">游戏资源</h3>
        <BaseView
          :view-key="displayPhase"
          :reduce-motion="form.reduceMotion"
          class="update-status-view"
        >
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
        </BaseView>
        <div
          v-if="budgetVisible && diskBudget"
          class="disk-budget"
          :class="{ 'is-short': diskBudget.insufficient }"
        >
          <div class="disk-budget__head">
            <span class="disk-budget__label">
              <svg class="icon" aria-hidden="true">
                <use href="#i-hard-drives" />
              </svg>
              磁盘空间
            </span>
            <span class="disk-budget__amounts">
              <span title="包含下载文件与应用补丁所需的临时空间"
                >预计需要 <strong>{{ diskBudget.requiredText }}</strong></span
              >
              <span>可用 {{ diskBudget.availableText }}</span>
              <span v-if="diskBudget.insufficient" class="disk-budget__short"
                >还差 {{ diskBudget.shortfallText }}</span
              >
            </span>
          </div>
          <div class="disk-budget__track" aria-hidden="true">
            <span
              class="disk-budget__fill"
              :style="{ width: diskBudget.percent + '%' }"
            />
          </div>
          <p v-if="diskBudget.insufficient" class="disk-budget__hint">
            清理磁盘空间或更换安装目录后重新检查.
          </p>
        </div>
        <AnimatePresence :initial="false">
          <motion.div
            v-if="showProgress && state.download"
            class="download-progress"
            :initial="{ height: 0, opacity: 0 }"
            :animate="{ height: 'auto', opacity: 1 }"
            :exit="{ height: 0, opacity: 0 }"
            :transition="surface"
          >
            <div class="download-progress__summary">
              <div>
                <p v-if="downloadView.sizeText" class="download-progress__size">
                  {{ downloadView.sizeText }}
                </p>
                <p v-if="downloadView.splitText">
                  {{ downloadView.splitText }}
                </p>
              </div>
              <strong v-if="transferActive" class="download-progress__value"
                >{{ Math.round(downloadView.progress) }}<span>%</span></strong
              >
            </div>
            <div v-if="transferActive" class="download-meter">
              <progress
                :value="downloadView.progress"
                max="100"
                :aria-label="'资源下载进度 ' + downloadView.progress + '%'"
              />
              <div class="download-meter__track" aria-hidden="true">
                <motion.div
                  class="download-meter__fill"
                  :style="{ scaleX: progress }"
                />
              </div>
            </div>
            <div v-if="transferActive" class="download-progress__meta">
              <span
                >{{ state.download.completedFiles }} /
                {{ state.download.totalFiles }} 个文件已完成</span
              ><span>已下载 {{ downloadView.downloadedText }}</span>
            </div>
            <p
              v-if="transferActive && downloadView.stageText"
              class="download-progress__stage"
              :title="state.download.currentFile"
            >
              {{
                phase === "cancelled"
                  ? "已暂停"
                  : "正在" + downloadView.stageText
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
                state.download.currentFile &&
                state.download.currentFileBytes > 0
              "
              class="download-progress__file-size"
            >
              当前文件 {{ formatSize(state.download.currentFileBytes) }}
            </p>
            <p v-if="state.download.remoteVersion">
              目标游戏版本 {{ state.download.remoteVersion }}
            </p>
          </motion.div>
        </AnimatePresence>
        <div class="update-actions">
          <BaseButton
            id="game-download-primary"
            data-pad-primary
            :data-pad-action="
              primary.kind === 'apply' ? '更新游戏' : '快速检查'
            "
            :variant="displayPhase === 'ready' ? 'secondary' : 'primary'"
            :icon="primary.icon"
            :disabled="primaryDisabled"
            :busy="active"
            :class="{ 'is-passed': displayPhase === 'ready' }"
            :title="displayPhase === 'ready' ? '再次检查游戏资源' : undefined"
            @click="runPrimary"
            >{{ primary.label }}</BaseButton
          >
          <BaseButton
            variant="ghost"
            :disabled="active"
            @click="startGameDownload('check', true)"
            >完整校验</BaseButton
          >
          <BaseButton
            v-if="cancellable"
            variant="ghost"
            @click="cancelGameDownload"
            >{{
              state.download?.phase === "checking" ? "取消检查" : "暂停下载"
            }}</BaseButton
          >
        </div>
        <div class="download-fields">
          <DirectoryField
            :key="form.distribution"
            id="download-install-root"
            v-model="currentRoot"
            label="安装目录"
            :disabled="settingsPathsLocked"
            @input="queueSettingsSave()"
            @change="queueSettingsSave(0)"
            :placeholder="
              form.distribution === 'cn'
                ? '~/Games/Neverness To Everness'
                : '~/Games/Neverness To Everness Global'
            "
          />
          <p class="download-hint">
            资源来自当前地区的官方 CDN. 未完成的文件可继续下载.
          </p>
        </div>
      </section>
      <section
        class="update-card update-launcher"
        :class="'is-' + updateView.kind"
        id="update-card"
        aria-labelledby="launcher-files-title"
      >
        <h3 id="launcher-files-title" class="update-card__title">启动器文件</h3>
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
          <BaseButton
            variant="ghost"
            id="update-quick"
            :disabled="state.checking || active"
            :busy="state.checking"
            @click="checkUpdate(false)"
            >检查启动器文件</BaseButton
          >
          <BaseButton
            variant="ghost"
            id="update-full"
            :disabled="state.checking || active"
            @click="checkUpdate(true)"
            >完整校验启动器</BaseButton
          >
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
      </section>
    </div>
  </motion.section>
</template>
