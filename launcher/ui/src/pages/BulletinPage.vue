<script setup lang="ts">
import { computed } from "vue";
import { fetchBulletins, openUrl, state } from "../core/controller";
import { distributions } from "../core/distribution";
import type { BulletinItem } from "../types/launcher";

const global = computed(
  () => state.bootstrap?.settings.distribution === "global",
);
const links = computed(() => distributions[global.value ? "global" : "cn"]);
const items = computed(() => state.bulletins?.items || []);
const bilibili = computed(() =>
  items.value.filter((item) => item.kind === "bilibili"),
);
const previews = computed(() =>
  items.value.filter((item) => item.kind === "preview"),
);
const maintenance = computed(() =>
  items.value.filter((item) => item.kind === "maintenance"),
);
const errors = computed(() => state.bulletins?.errors || []);
const loading = computed(() => state.bulletinsLoading);

function label(item: BulletinItem) {
  return item.publishedAt
    ? `${item.source} · ${item.publishedAt}`
    : item.source;
}
</script>

<template>
  <section
    class="page bulletin-page"
    id="page-bulletin"
    :class="{ 'is-active': state.page === 'bulletin' }"
    aria-labelledby="bulletin-title"
  >
    <header class="page-head">
      <div class="page-head__copy">
        <h2 id="bulletin-title">公告</h2>
      </div>
      <div class="page-head__actions">
        <button
          class="btn btn--link"
          type="button"
          :disabled="loading"
          @click="fetchBulletins(false, true)"
        >
          <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-arrows-clockwise"></use>
          </svg>
          <span class="btn__text">{{ loading ? "正在更新" : "刷新公告" }}</span>
        </button>
      </div>
    </header>

    <article class="bulletin-facts">
      <span class="panel-label">游戏版本</span>
      <div class="bulletin-number">
        {{ state.bootstrap?.gameVersion || "未检测" }}
      </div>
      <p class="bulletin-copy">
        {{
          state.bootstrap?.gameExists ? "游戏资源已就绪" : "尚未检测到游戏资源"
        }}。启动器版本 {{ state.bootstrap?.launcherVersion || "未知" }} / build
        {{ state.bootstrap?.launcherBuild || "未知" }}。
      </p>
    </article>

    <div class="bulletin-sections">
      <section
        v-if="!global"
        class="bulletin-section bulletin-section--bilibili"
        aria-labelledby="bilibili-title"
      >
        <div class="bulletin-section__head">
          <div>
            <h3 id="bilibili-title">B站官方动态</h3>
          </div>
        </div>
        <div v-if="bilibili.length" class="bulletin-cards">
          <button
            v-for="item in bilibili"
            :key="item.url"
            class="bulletin-card bulletin-card--bilibili"
            type="button"
            @click="openUrl(item.url)"
          >
            <img
              v-if="item.imageUrl"
              class="bulletin-card__image"
              :src="item.imageUrl"
              alt=""
            />
            <span class="bulletin-card__meta">{{ label(item) }}</span
            ><strong>{{ item.title }}</strong>
            <span v-if="item.summary" class="bulletin-card__summary">{{
              item.summary
            }}</span>
            <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
              <use href="#i-arrow-up-right"></use>
            </svg>
          </button>
        </div>
        <p v-else class="bulletin-empty bulletin-empty--small" role="status">
          暂无可用动态，稍后可重新刷新
        </p>
      </section>

      <section
        class="bulletin-section bulletin-section--preview"
        aria-labelledby="preview-title"
      >
        <div class="bulletin-section__head">
          <div>
            <h3 id="preview-title">{{ global ? "国际服新闻" : "版本前瞻" }}</h3>
          </div>
        </div>
        <div
          v-if="previews.length"
          class="bulletin-cards bulletin-cards--preview"
        >
          <button
            v-for="item in previews"
            :key="item.url"
            class="bulletin-card bulletin-card--preview"
            type="button"
            @click="openUrl(item.url)"
          >
            <img
              v-if="item.imageUrl"
              class="bulletin-card__image"
              :src="item.imageUrl"
              alt=""
            />
            <span class="bulletin-card__meta">{{ label(item) }}</span
            ><strong>{{ item.title }}</strong>
            <span class="bulletin-card__summary">{{ item.summary }}</span>
            <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
              <use href="#i-arrow-up-right"></use>
            </svg>
          </button>
        </div>
        <p v-else class="bulletin-empty bulletin-empty--small">
          暂无前瞻数据。
        </p>
      </section>

      <section
        class="bulletin-section bulletin-section--maintenance"
        aria-labelledby="maintenance-title"
      >
        <div class="bulletin-section__head">
          <div>
            <h3 id="maintenance-title">
              {{ global ? "国际服公告" : "停服维护公告" }}
            </h3>
          </div>
        </div>
        <div v-if="maintenance.length" class="bulletin-list">
          <button
            v-for="item in maintenance"
            :key="item.url"
            class="bulletin-list__row"
            type="button"
            @click="openUrl(item.url)"
          >
            <span
              ><strong>{{ item.title }}</strong
              ><small>{{ item.summary }}</small></span
            ><span class="bulletin-list__date">{{
              item.publishedAt || "—"
            }}</span>
            <svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
              <use href="#i-arrow-up-right"></use>
            </svg>
          </button>
        </div>
        <p v-else class="bulletin-empty bulletin-empty--small">
          暂无公告数据。
        </p>
      </section>

      <p v-if="errors.length" class="bulletin-source-note">
        部分来源暂时不可用：{{ errors.join("；") }}。已展示成功读取的内容。
      </p>
    </div>

    <div class="bulletin-links">
      <button
        class="link-row"
        id="open-official"
        type="button"
        @click="openUrl(links.official)"
      >
        <span>打开官方公告页</span
        ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-arrow-up-right"></use>
        </svg>
      </button>
      <button
        class="link-row"
        id="open-contract"
        type="button"
        @click="openUrl(links.contract)"
      >
        <span>游戏用户协议</span
        ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-arrow-up-right"></use>
        </svg>
      </button>
      <button
        class="link-row"
        id="open-privacy"
        type="button"
        @click="openUrl(links.privacy)"
      >
        <span>个人信息保护政策</span
        ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-arrow-up-right"></use>
        </svg>
      </button>
      <button
        class="link-row"
        id="open-support"
        type="button"
        @click="openUrl(links.support)"
      >
        <span>官方客服与修复</span
        ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
          <use href="#i-arrow-up-right"></use>
        </svg>
      </button>
    </div>
  </section>
</template>
