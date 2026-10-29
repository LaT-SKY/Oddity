<script setup lang="ts">
import { motion } from "motion-v";
import { computed, ref } from "vue";
import type { BulletinItem } from "../types/launcher";
import { fetchBulletins, form, openUrl, state } from "../core/controller";
import { distributions } from "../core/distribution";
import BaseButton from "../components/BaseButton.vue";
import BaseTabs from "../components/BaseTabs.vue";
import BaseView from "../components/BaseView.vue";
import BulletinEntry from "../components/BulletinEntry.vue";
import BulletinGroup from "../components/BulletinGroup.vue";
import BulletinSkeleton from "../components/BulletinSkeleton.vue";
import { useControlMotion } from "../composables/useControlMotion";
const global = computed(
  () => state.bootstrap?.settings.distribution === "global",
);
const links = computed(() => distributions[global.value ? "global" : "cn"]);
const items = computed(() => state.bulletins?.items ?? []);
const errors = computed(() => state.bulletins?.errors ?? []);
const loading = computed(() => state.bulletinsLoading);
const stacked = computed(() => form.stackBulletins);
const filter = ref<"all" | "social" | "preview" | "maintenance">("all");
const filters = [
  { value: "all", label: "全部" },
  { value: "social", label: "官方动态" },
  { value: "preview", label: "版本资讯" },
  { value: "maintenance", label: "维护公告" },
] as const;
const visible = computed(() =>
  items.value.filter(
    (item) =>
      filter.value === "all" ||
      (filter.value === "social"
        ? ["bilibili", "x"].includes(item.kind)
        : item.kind === filter.value),
  ),
);
const groups = computed(() => {
  const result = new Map<string, BulletinItem[]>();
  for (const item of visible.value) {
    const group = result.get(item.source);
    if (group) group.push(item);
    else result.set(item.source, [item]);
  }
  return [...result.entries()];
});
const { surface } = useControlMotion();
const isActive = computed(() => state.page === "bulletin");
</script>
<template>
  <motion.section
    class="page bulletin-page"
    id="page-bulletin"
    :inert="!isActive"
    :aria-hidden="!isActive || undefined"
    :initial="false"
    :animate="{
      opacity: isActive ? 1 : 0,
      x: isActive ? 0 : 22,
      visibility: isActive ? 'visible' : 'hidden',
    }"
    :transition="surface"
    :class="{ 'is-active': isActive }"
    aria-labelledby="bulletin-title"
  >
    <header class="page-head">
      <div class="page-head__copy">
        <h2 id="bulletin-title" tabindex="-1">公告</h2>
      </div>
      <BaseButton
        variant="ghost"
        icon="i-arrows-clockwise"
        :busy="loading"
        data-pad-primary
        data-pad-action="刷新公告"
        @click="fetchBulletins(false, true)"
        >{{ loading ? "正在更新" : "刷新公告" }}</BaseButton
      >
    </header>
    <BaseTabs
      class="bulletin-filters"
      variant="navigation"
      v-model="filter"
      :options="filters"
      label="公告分类"
    />
    <div class="bulletin-content" :aria-busy="loading">
      <div v-if="errors.length" class="bulletin-errors">
        <div
          v-for="error in errors"
          :key="error"
          class="bulletin-source-error"
          role="status"
        >
          <svg class="icon" aria-hidden="true"><use href="#i-info" /></svg>
          <p>{{ error }}</p>
        </div>
        <div class="bulletin-retry">
          <BaseButton
            variant="ghost"
            :disabled="loading"
            @click="fetchBulletins(false, true)"
            >重新读取</BaseButton
          ><BaseButton
            v-if="global"
            variant="ghost"
            @click="openUrl('https://x.com/NTE_JP')"
            >打开 X 官方账号</BaseButton
          >
        </div>
      </div>
      <BulletinSkeleton v-if="loading && !items.length" />
      <template v-else>
        <p v-if="loading" class="bulletin-refreshing" role="status">
          正在读取最新公告
        </p>
        <BaseView
          :view-key="`${global}:${filter}:${stacked}`"
          class="bulletin-feed"
        >
          <template v-if="stacked">
            <template v-for="[source, group] in groups" :key="source">
              <BulletinGroup
                v-if="group.length > 1"
                :items="group"
                @open="openUrl"
              />
              <BulletinEntry v-else :item="group[0]" @open="openUrl" />
            </template>
          </template>
          <template v-else>
            <BulletinEntry
              v-for="item in visible"
              :key="item.url"
              :item="item"
              @open="openUrl"
            />
          </template>
          <p v-if="!visible.length" class="bulletin-empty">
            {{ errors.length ? "该来源暂时无法读取." : "暂无公告." }}
          </p>
        </BaseView>
      </template>
      <footer class="bulletin-links">
        <BaseButton
          variant="ghost"
          id="open-official"
          @click="openUrl(links.official)"
          >官方公告</BaseButton
        >
        <BaseButton
          variant="ghost"
          id="open-contract"
          @click="openUrl(links.contract)"
          >用户协议</BaseButton
        >
        <BaseButton
          variant="ghost"
          id="open-privacy"
          @click="openUrl(links.privacy)"
          >隐私政策</BaseButton
        >
        <BaseButton
          variant="ghost"
          id="open-support"
          @click="openUrl(links.support)"
          >官方客服</BaseButton
        >
      </footer>
    </div>
  </motion.section>
</template>
