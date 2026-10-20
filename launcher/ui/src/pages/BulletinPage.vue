<script setup lang="ts">
import { state, openUrl } from "../core/controller";
</script>

<template>
  <section
    class="page"
    id="page-bulletin"
    :class="{ 'is-active': state.page === 'bulletin' }"
    aria-labelledby="bulletin-title"
  >
    <header class="page-head">
      <div class="page-head__copy">
        <div class="eyebrow">
          <span class="eyebrow__line"></span>BULLETIN / 城市广播
        </div>
        <h2 id="bulletin-title">官方消息，保持原样</h2>
        <p>
          公告、活动与登录页仍由官方页面承载。原生启动器只保留入口与版本事实，不复制、不伪造公告。
        </p>
      </div>
    </header>

    <div class="bulletin-grid">
      <article class="bulletin-hero">
        <div class="panel-label">CURRENT BUILD</div>
        <div class="bulletin-number">
          {{ state.bootstrap?.gameVersion || "未检测" }}
        </div>
        <p class="bulletin-copy">
          {{
            state.bootstrap?.gameExists
              ? "游戏资源已就绪"
              : "尚未检测到游戏资源"
          }}。启动器版本 {{ state.bootstrap?.launcherVersion || "未知" }} /
          build
          {{ state.bootstrap?.launcherBuild || "未知" }}，官方登录与补丁通过
          Proton 桥接完成。
        </p>
        <button
          class="btn btn--primary"
          id="open-official"
          @click="
            openUrl('https://yh.wanmei.com/launcher/launcher_ob2.html?expand=0')
          "
        >
          <span class="btn__text">打开官方公告</span>
          <span class="btn__icon"
            ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
              <use href="#i-arrow-up-right"></use></svg
          ></span>
        </button>
        <span class="bulletin-url mono"
          >yh.wanmei.com/launcher/launcher_ob2.html</span
        >
      </article>

      <div class="bulletin-links">
        <button
          class="link-row"
          id="open-contract"
          @click="openUrl('https://www.wanmei.com/safestatic/contract.html')"
        >
          <span>完美世界游戏用户协议</span
          ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-arrow-up-right"></use>
          </svg>
        </button>
        <button
          class="link-row"
          id="open-privacy"
          @click="openUrl('https://www.wanmei.com/safestatic/privacy.html')"
        >
          <span>个人信息保护政策</span
          ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-arrow-up-right"></use>
          </svg>
        </button>
        <button
          class="link-row"
          id="open-support"
          @click="openUrl('https://games.wanmei.com/m/sdk_kf3/index.html')"
        >
          <span>官方客服与修复</span
          ><svg class="icon" viewBox="0 0 256 256" aria-hidden="true">
            <use href="#i-arrow-up-right"></use>
          </svg>
        </button>
        <p class="note">外链由系统浏览器打开，不在启动器内执行远程网页。</p>
      </div>
    </div>
  </section>
</template>
