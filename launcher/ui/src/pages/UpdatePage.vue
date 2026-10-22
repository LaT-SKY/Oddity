<script setup lang="ts">
import { state, checkUpdate, updateView } from "../core/controller";
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
      <div
        class="update-card"
        :class="'is-' + updateView.kind"
        id="update-card"
      >
        <div class="update-state">
          <span class="update-icon" aria-hidden="true">
            <svg class="icon" viewBox="0 0 256 256">
              <use id="update-icon-use" :href="'#' + updateView.icon"></use>
            </svg>
          </span>
          <div class="update-state__copy">
            <h3 id="update-state-title">{{ updateView.title }}</h3>
            <p id="update-state-text">{{ updateView.copy }}</p>
          </div>
        </div>
        <div class="update-actions">
          <button
            class="btn btn--primary"
            id="update-quick"
            :disabled="state.checking"
            :class="{ 'is-busy': state.checking }"
            @click="checkUpdate(false)"
          >
            <svg class="icon" aria-hidden="true">
              <use href="#i-arrows-clockwise" />
            </svg>
            <span class="btn__text">检查更新</span>
          </button>
          <button
            class="btn btn--link"
            id="update-full"
            :disabled="state.checking"
            :class="{ 'is-busy': state.checking }"
            @click="checkUpdate(true)"
          >
            完整校验
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
