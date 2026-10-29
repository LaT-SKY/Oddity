<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { motion, useAnimate } from "motion-v";
import { form, queueSettingsSave, state, ui } from "../core/controller";
import { notify } from "../core/notifications";
import { invoke, isTauri } from "../core/api";
import type { GameModeOptions, GameModeStatus } from "../types/launcher";
import { useControlMotion } from "../composables/useControlMotion";
import GamescopeSettings from "../components/GamescopeSettings.vue";
import CpuSettings from "../components/CpuSettings.vue";
import GpuSettings from "../components/GpuSettings.vue";
import BaseInput from "../components/BaseInput.vue";
import BaseButton from "../components/BaseButton.vue";
import BaseSwitch from "../components/BaseSwitch.vue";
import SettingGroup from "../components/settings/SettingGroup.vue";
import SettingRow from "../components/settings/SettingRow.vue";

const { surface, control, reduced } = useControlMotion();
const [, animate] = useAnimate();
const root = ref<HTMLElement | null>(null);
const status = ref<GameModeStatus | null>(null);
const gpu = ref<InstanceType<typeof GpuSettings> | null>(null);
const tuning = ref<InstanceType<typeof CpuSettings> | null>(null);
const loading = ref(false);
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
  items: {
    id: Exclude<keyof GameModeOptions, "gamescope">;
    label: string;
    description: string;
  }[];
}[];
const categories = [
  { id: "general", label: "常规", icon: "i-game-controller" },
  { id: "cpu", label: "CPU 与优先级", icon: "i-cpu" },
  { id: "gpu", label: "GPU 超频", icon: "i-graphics-card" },
  { id: "gamescope", label: "Gamescope", icon: "i-monitor" },
] as const;
type Category = (typeof categories)[number]["id"];
const category = ref<Category>("general");
const query = ref("");
const searching = computed(() => Boolean(query.value.trim()));
const entries: {
  id: string;
  category: Category;
  label: string;
  description: string;
  keywords: string;
}[] = [
  ...groups.flatMap((group) =>
    group.items.map((item) => ({
      ...item,
      category: "general" as const,
      keywords: group.title,
    })),
  ),
  {
    id: "cpu-pinCores",
    category: "cpu",
    label: "核心绑定",
    description: "将游戏限制在所选 CPU 核心上.",
    keywords: "pin cores 处理器 亲和性 affinity",
  },
  {
    id: "cpu-parkCores",
    category: "cpu",
    label: "核心停驻",
    description: "游戏运行时暂时停用所选核心.",
    keywords: "park cores 停放 处理器",
  },
  {
    id: "cpu-renice",
    category: "cpu",
    label: "进程优先级",
    description: "设置游戏进程的调度优先级.",
    keywords: "renice nice 调度",
  },
  {
    id: "cpu-ioprio",
    category: "cpu",
    label: "I/O 优先级",
    description: "设置游戏进程的磁盘 I/O 优先级.",
    keywords: "ioprio 磁盘 读写",
  },
  {
    id: "gpu",
    category: "gpu",
    label: "GPU 超频",
    description: "显卡频率偏移与性能等级.",
    keywords: "NVIDIA AMD 显存 核心 PowerMizer MHz",
  },
  {
    id: "gamescope",
    category: "gamescope",
    label: "启用 Gamescope",
    description: "游戏窗口、分辨率、帧率与缩放.",
    keywords: "窗口 全屏 无边框 FSR NIS 整数缩放",
  },
];
const results = computed(() => {
  const words = query.value.trim().toLocaleLowerCase().split(/\s+/);
  return entries.filter((entry) =>
    words.every((word) =>
      `${entry.label} ${entry.description} ${entry.keywords} ${categories.find((category) => category.id === entry.category)!.label}`
        .toLocaleLowerCase()
        .includes(word),
    ),
  );
});
function chooseCategory(value: Category) {
  category.value = value;
  query.value = "";
}
function categoryKeys(event: KeyboardEvent) {
  const index = categories.findIndex((item) => item.id === category.value);
  const next =
    event.key === "ArrowDown" || event.key === "ArrowRight"
      ? (index + 1) % categories.length
      : event.key === "ArrowUp" || event.key === "ArrowLeft"
        ? (index - 1 + categories.length) % categories.length
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? categories.length - 1
            : -1;
  if (next < 0) return;
  event.preventDefault();
  chooseCategory(categories[next].id);
  root.value
    ?.querySelector<HTMLElement>(`[data-category="${category.value}"]`)
    ?.focus();
}
async function locate(entry: (typeof entries)[number]) {
  chooseCategory(entry.category);
  await nextTick();
  const target = root.value?.querySelector<HTMLElement>(
    `[data-game-setting="${entry.id}"]`,
  );
  if (!target) return;
  target.scrollIntoView({ block: "center", behavior: "instant" });
  (
    target.querySelector<HTMLElement>(
      "button:not(:disabled), input:not(:disabled)",
    ) ?? target
  ).focus({ preventScroll: true });
  void animate(
    target,
    { backgroundColor: ["var(--signal-dim)", "rgba(0, 0, 0, 0)"] },
    { duration: reduced.value ? 0 : 1.1 },
  );
}
function searchKeys(event: KeyboardEvent) {
  if (event.key === "Escape" && query.value) {
    event.preventDefault();
    event.stopPropagation();
    query.value = "";
    root.value?.querySelector<HTMLInputElement>("#game-mode-search")?.focus();
  } else if (
    event.key === "ArrowDown" &&
    searching.value &&
    event.target === root.value?.querySelector("#game-mode-search")
  ) {
    event.preventDefault();
    root.value?.querySelector<HTMLElement>(".settings-result")?.focus();
  }
}
function resultKeys(event: KeyboardEvent) {
  if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
  event.preventDefault();
  const buttons = [
    ...root.value!.querySelectorAll<HTMLButtonElement>(".settings-result"),
  ];
  const index =
    buttons.indexOf(document.activeElement as HTMLButtonElement) +
    (event.key === "ArrowDown" ? 1 : -1);
  if (index < 0)
    root.value?.querySelector<HTMLInputElement>("#game-mode-search")?.focus();
  else buttons[Math.min(index, buttons.length - 1)]?.focus();
}
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
function description(item: {
  id: Exclude<keyof GameModeOptions, "gamescope">;
  description: string;
}) {
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
      for (const effect of result.session ?? []) {
        const previous = status.value?.session?.find(
          (item) => item.id === effect.id,
        );
        if (
          effect.state === "error" &&
          (previous?.state !== "error" || previous.detail !== effect.detail)
        ) {
          notify(effect.detail, "error", 6500, {
            key: `game-mode-${effect.id}`,
          });
        }
      }
      status.value = result;
    },
    (reason: unknown) => {
      if (!disposed)
        notify(
          reason instanceof Error ? reason.message : String(reason),
          "error",
          6500,
          { key: "game-mode-detect" },
        );
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
    <div ref="root" class="settings-workspace" @keydown="searchKeys">
      <header class="page-head settings-head">
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
            data-pad-primary
            data-pad-action="重新检测"
            @click="
              refresh();
              gpu?.reload();
              tuning?.reload();
            "
            >重新检测</BaseButton
          >
        </div>
      </header>
      <div class="settings-search">
        <svg
          class="settings-search__icon"
          viewBox="0 0 24 24"
          fill="none"
          aria-hidden="true"
        >
          <circle
            cx="10.5"
            cy="10.5"
            r="6.5"
            stroke="currentColor"
            stroke-width="1.6"
          />
          <path
            d="m15.5 15.5 4.5 4.5"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linecap="round"
          />
        </svg>
        <BaseInput
          id="game-mode-search"
          v-model="query"
          type="search"
          aria-label="搜索游戏模式设置"
          placeholder="搜索游戏模式设置"
          autocomplete="off"
          :aria-controls="searching ? 'game-mode-results' : undefined"
        />
        <BaseButton
          v-if="query"
          class="settings-search__clear"
          variant="ghost"
          size="sm"
          icon="i-x"
          icon-only
          aria-label="清除搜索"
          @click="query = ''"
        />
      </div>
      <div class="settings-frame">
        <nav
          class="settings-categories"
          aria-label="游戏模式分类"
          @keydown="categoryKeys"
        >
          <motion.button
            v-for="item in categories"
            :key="item.id"
            type="button"
            class="settings-category category-tab"
            :data-category="item.id"
            :aria-current="
              !searching && category === item.id ? 'true' : undefined
            "
            :tabindex="category === item.id ? 0 : -1"
            :while-press="reduced ? undefined : { scale: 0.97 }"
            :transition="control"
            :aria-controls="`game-mode-panel-${item.id}`"
            @click="chooseCategory(item.id)"
          >
            <motion.span
              v-if="category === item.id"
              class="settings-category__focus category-tab__selection"
              layout-id="game-mode-category-focus"
              :animate="{ opacity: searching ? 0.35 : 1 }"
              :transition="control"
            />
            <svg class="icon" aria-hidden="true">
              <use :href="`#${item.icon}`" /></svg
            ><span>{{ item.label }}</span>
          </motion.button>
        </nav>
        <form
          class="settings-stage"
          :data-save-state="ui.saveState"
          @submit.prevent
        >
          <motion.div
            v-for="item in categories"
            :key="item.id"
            :id="`game-mode-panel-${item.id}`"
            class="settings-panel"
            :class="{ 'is-current': !searching && category === item.id }"
            role="region"
            :aria-label="item.label"
            :aria-hidden="searching || category !== item.id || undefined"
            :inert="searching || category !== item.id"
            :style="{
              pointerEvents:
                !searching && category === item.id ? 'auto' : 'none',
              zIndex: !searching && category === item.id ? 1 : 0,
            }"
            :initial="false"
            :animate="{
              opacity: !searching && category === item.id ? 1 : 0,
              x: !searching && category === item.id ? 0 : 12,
            }"
            :transition="surface"
          >
            <template v-if="item.id === 'general'">
              <SettingGroup
                v-for="group in groups"
                :key="group.title"
                :title="group.title"
              >
                <SettingRow
                  v-for="option in group.items"
                  :key="option.id"
                  :data-game-setting="option.id"
                  tabindex="-1"
                  :label="option.label"
                  :description="description(option)"
                  :for="`game-mode-${option.id}`"
                  :class="{ 'is-unavailable': !available(option.id) }"
                >
                  <template #default="{ describedby }">
                    <BaseSwitch
                      :id="`game-mode-${option.id}`"
                      v-model="form.gameMode[option.id]"
                      :describedby="describedby"
                      :disabled="
                        !state.bootstrap ||
                        (!form.gameMode[option.id] && !available(option.id))
                      "
                      @change="queueSettingsSave(0)"
                    />
                  </template>
                </SettingRow>
              </SettingGroup>
            </template>
            <CpuSettings
              v-else-if="item.id === 'cpu'"
              :ref="
                (value) => {
                  tuning = value as InstanceType<typeof CpuSettings> | null;
                }
              "
              :available="available('performance')"
            />
            <GpuSettings
              v-else-if="item.id === 'gpu'"
              :ref="
                (value) => {
                  gpu = value as InstanceType<typeof GpuSettings> | null;
                }
              "
              :available="available('performance')"
            />
            <GamescopeSettings v-else :available="available('gamescope')" />
          </motion.div>
          <div
            v-if="searching"
            id="game-mode-results"
            class="settings-search-results"
            aria-label="搜索结果"
            @keydown="resultKeys"
          >
            <p class="settings-results-count" role="status">
              {{
                results.length
                  ? `找到 ${results.length} 项设置`
                  : "未找到相关设置"
              }}
            </p>
            <button
              v-for="entry in results"
              :key="entry.id"
              class="settings-result"
              type="button"
              @click="locate(entry)"
            >
              <span
                ><strong>{{ entry.label }}</strong
                ><small
                  >{{
                    categories.find((item) => item.id === entry.category)!.label
                  }}
                  · {{ entry.description }}</small
                ></span
              ><svg class="icon" aria-hidden="true">
                <use href="#i-arrow-up-right" />
              </svg>
            </button>
          </div>
        </form>
      </div>
    </div>
  </motion.section>
</template>
