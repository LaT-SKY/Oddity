<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { motion } from "motion-v";
import { form, queueSettingsSave, state, ui } from "../core/controller";
import { invoke, isTauri } from "../core/api";
import type { GameModeOptions, GameModeStatus } from "../types/launcher";
import { useControlMotion } from "../composables/useControlMotion";
import BaseButton from "../components/BaseButton.vue";
import BaseSwitch from "../components/BaseSwitch.vue";
import SettingGroup from "../components/settings/SettingGroup.vue";
import SettingRow from "../components/settings/SettingRow.vue";

const { surface } = useControlMotion();
const status = ref<GameModeStatus | null>(null);
const loading = ref(false);
const error = ref("");
let timer: ReturnType<typeof setTimeout> | undefined;
let disposed = false;
const groups = [
  {
    title: "性能优化",
    items: [
      {
        id: "performance",
        label: "启用 GameMode",
        description: "优化游戏运行性能.",
      },
    ],
  },
  {
    title: "桌面行为",
    items: [
      {
        id: "inhibit",
        label: "防止空闲休眠",
        description: "游戏运行时保持屏幕唤醒.",
      },
      {
        id: "notifications",
        label: "屏蔽通知",
        description: "游戏运行时隐藏桌面通知.",
      },
    ],
  },
  {
    title: "性能监控",
    items: [
      {
        id: "overlay",
        label: "显示性能浮层",
        description: "使用 MangoHud 显示帧率与硬件负载.",
      },
    ],
  },
] satisfies {
  title: string;
  items: { id: keyof GameModeOptions; label: string; description: string }[];
}[];
const saveLabel = computed(
  () =>
    ({ idle: "", saving: "正在保存", saved: "已保存", error: "保存失败" })[
      ui.saveState
    ],
);
function available(id: keyof GameModeOptions) {
  return Boolean(
    status.value?.capabilities.some((item) => item.id === id && item.available),
  );
}
function description(item: { id: keyof GameModeOptions; description: string }) {
  const effect = status.value?.session?.find(
    (feature) => feature.id === item.id,
  );
  if (effect?.state === "error") return "启用失败, 请重试.";
  const capability = status.value?.capabilities.find(
    (feature) => feature.id === item.id,
  );
  if (!isTauri() || !capability || capability.available)
    return item.description;
  return {
    performance: "需安装并启动 GameMode.",
    inhibit: "当前桌面不支持.",
    notifications: "当前桌面不支持.",
    overlay: "需安装 MangoHud.",
  }[item.id];
}
async function refresh() {
  if (loading.value || disposed) return;
  if (timer) clearTimeout(timer);
  loading.value = true;
  await invoke("game_mode_status").then(
    (result) => {
      if (disposed) return;
      status.value = result;
      error.value = "";
    },
    (reason: unknown) => {
      if (!disposed)
        error.value = reason instanceof Error ? reason.message : String(reason);
    },
  );
  loading.value = false;
  if (!disposed && state.page === "game-mode" && state.game !== "idle")
    timer = setTimeout(refresh, 2500);
}
watch(
  () => [state.page, state.game],
  () => {
    if (timer) clearTimeout(timer);
    if (state.page === "game-mode") void refresh();
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  disposed = true;
  if (timer) clearTimeout(timer);
});
</script>

<template>
  <motion.section
    id="page-game-mode"
    class="page game-mode-page"
    :class="{ 'is-active': state.page === 'game-mode' }"
    :inert="state.page !== 'game-mode'"
    :aria-hidden="state.page !== 'game-mode' || undefined"
    :initial="false"
    :animate="{
      opacity: state.page === 'game-mode' ? 1 : 0,
      x: state.page === 'game-mode' ? 0 : 22,
      visibility: state.page === 'game-mode' ? 'visible' : 'hidden',
    }"
    :transition="surface"
    aria-labelledby="game-mode-title"
  >
    <header class="page-head game-mode-head">
      <div class="page-head__copy">
        <h2 id="game-mode-title" tabindex="-1">游戏模式</h2>
      </div>
      <div class="settings-head__tools">
        <span class="settings-save" role="status" aria-live="polite">{{
          saveLabel
        }}</span>
        <BaseButton
          v-if="ui.saveState === 'error'"
          variant="ghost"
          @click="queueSettingsSave(0)"
          >重试</BaseButton
        >
        <BaseButton
          variant="ghost"
          icon="i-arrows-clockwise"
          :busy="loading"
          @click="refresh"
          >重新检测</BaseButton
        >
      </div>
    </header>
    <div class="game-mode-content" :data-save-state="ui.saveState">
      <p v-if="error" class="game-mode-error" role="alert">检测失败, 请重试.</p>
      <SettingGroup
        v-for="group in groups"
        :key="group.title"
        :title="group.title"
      >
        <SettingRow
          v-for="item in group.items"
          :key="item.id"
          :label="item.label"
          :description="description(item)"
          :for="`game-mode-${item.id}`"
          :class="{ 'is-unavailable': !available(item.id) }"
        >
          <template #default="{ describedby }">
            <BaseSwitch
              :id="`game-mode-${item.id}`"
              v-model="form.gameMode[item.id]"
              :describedby="describedby"
              :disabled="
                !state.bootstrap ||
                (!form.gameMode[item.id] && !available(item.id))
              "
              @change="queueSettingsSave(0)"
            />
          </template>
        </SettingRow>
      </SettingGroup>
    </div>
  </motion.section>
</template>
