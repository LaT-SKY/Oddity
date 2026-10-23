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
</script>

<template>
  <section
    class="page page--update"
    :class="{ 'is-active': state.page === 'update' }"
    id="page-update"
    aria-labelledby="update-title"
  >
    <div class="update-wrap">
      <h2 class="update-title" id="update-title">游戏更新</h2>
      <div class="update-card game-download-card">
        <div class="update-state">
          <span class="update-icon" aria-hidden="true"
            ><svg class="icon" viewBox="0 0 256 256">
              <use href="#i-arrows-clockwise" /></svg
          ></span>
          <div class="update-state__copy">
            <h3>{{ statusMatches ? downloadView.title : "选择游戏地区" }}</h3>
            <p>
              {{
                statusMatches
                  ? downloadView.detail
                  : "选择地区和安装目录后检查资源"
              }}
            </p>
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
          将从当前地区的官方 CDN
          获取启动器配置和游戏资源。未完成的文件可在下次下载时继续。
        </p>
        <div class="update-actions">
          <button
            class="btn btn--link"
            type="button"
            :disabled="active"
            @click="startGameDownload('check', false)"
          >
            检查资源
          </button>
          <button
            class="btn btn--link"
            type="button"
            :disabled="active"
            @click="startGameDownload('check', true)"
          >
            完整校验
          </button>
          <button
            class="btn btn--primary"
            type="button"
            :disabled="active || state.game !== 'idle'"
            @click="startGameDownload('apply', false)"
          >
            安装或更新
          </button>
          <button
            v-if="active"
            class="btn btn--link"
            type="button"
            @click="cancelGameDownload"
          >
            暂停下载
          </button>
        </div>
        <div
          v-if="
            statusMatches &&
            state.download &&
            state.download.phase !== 'checking' &&
            (active || state.download.pendingFiles > 0)
          "
          class="download-progress"
          role="status"
          aria-live="polite"
        >
          <div class="download-progress__meta">
            <span
              >{{ state.download.completedFiles }} /
              {{ state.download.totalFiles }} 个文件</span
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
            :value="downloadView.progress"
            max="100"
            :aria-label="'下载进度 ' + downloadView.progress + '%'"
          />
          <p
            v-if="downloadView.stageText"
            class="download-progress__stage"
            :title="state.download.currentFile"
          >
            正在{{ downloadView.stageText
            }}<template v-if="state.download.currentFile"
              >: {{ state.download.currentFile }}</template
            >
          </p>
          <p
            v-else-if="state.download.currentFile"
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

      <h3 class="update-subtitle">启动器文件校验</h3>
      <div
        class="update-card"
        :class="'is-' + updateView.kind"
        id="update-card"
      >
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
            检查启动器文件
          </button>
          <button
            class="btn btn--link"
            id="update-full"
            :disabled="state.checking || active"
            @click="checkUpdate(true)"
          >
            完整校验启动器
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
    </div>
  </section>
</template>
