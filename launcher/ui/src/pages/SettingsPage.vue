<script setup lang="ts">
import { motion, useAnimate } from "motion-v";
import { computed, nextTick, reactive, ref, watch } from "vue";
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
  runtimeEnvironmentBusy,
  runtimePathsLocked,
} from "../core/controller";
import {
  settingsCategories,
  settingsEntries,
  settingsNavigation as navigation,
  type SettingsCategory,
  type SettingsEntry,
} from "../core/settingsCatalog";
import { useControlMotion } from "../composables/useControlMotion";
import RuntimeEnvironments from "../components/RuntimeEnvironments.vue";
import TaygedoSettings from "../components/TaygedoSettings.vue";
import BaseSelect from "../components/BaseSelect.vue";
import BaseSwitch from "../components/BaseSwitch.vue";
import BaseButton from "../components/BaseButton.vue";
import BaseInput from "../components/BaseInput.vue";
import DirectoryField from "../components/DirectoryField.vue";
import SettingGroup from "../components/settings/SettingGroup.vue";
import SettingRow from "../components/settings/SettingRow.vue";
import LogExport from "../components/LogExport.vue";
import GamepadSettings from "../components/GamepadSettings.vue";

const root = ref<HTMLElement | null>(null);
const { control, surface, reduced } = useControlMotion();
const [, animate] = useAnimate();
// 分类面板按首次访问挂载, 避免启动时构建全部设置项。
const visited = reactive(new Set<SettingsCategory>(["appearance"]));
watch(
  () => [state.page, navigation.category] as const,
  ([page, category]) => {
    if (page === "settings") visited.add(category);
  },
  { immediate: true },
);
const themeOptions = [
  { label: "亮色", value: "light" },
  { label: "暗色", value: "dark" },
];
const distributionOptions = [
  { label: "国服", value: "cn" },
  { label: "国际服", value: "global" },
];
const mediaOptions = [
  { label: "动态视频", value: "video" },
  { label: "静态照片", value: "photo" },
];
const launchOptions = [
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
  () => launchOptions.find((option) => option.value === form.launchMode)?.hint,
);
const searching = computed(() => !!navigation.query.trim());
const results = computed(() => {
  const words = navigation.query.trim().toLocaleLowerCase().split(/\s+/);
  return settingsEntries.filter((entry) =>
    words.every((word) =>
      `${entry.label} ${entry.description} ${entry.keywords} ${settingsCategories.find((category) => category.id === entry.category)!.label}`
        .toLocaleLowerCase()
        .includes(word),
    ),
  );
});
const entry = (id: SettingsEntry["id"]) =>
  settingsEntries.find((entry) => entry.id === id)!;
function chooseCategory(category: SettingsCategory) {
  navigation.category = category;
  navigation.query = "";
}
async function locate(item: SettingsEntry) {
  visited.add(item.category);
  chooseCategory(item.category);
  await nextTick();
  const target = root.value!.querySelector<HTMLElement>(
    `[data-setting="${item.id}"]`,
  )!;
  target.scrollIntoView({ block: "center", behavior: "instant" });
  const control = target.querySelector<HTMLElement>(
    "button:not(:disabled), input:not(:disabled):not([hidden])",
  );
  (control ?? target).focus({ preventScroll: true });
  void animate(
    target,
    { backgroundColor: ["var(--signal-dim)", "rgba(0, 0, 0, 0)"] },
    { duration: reduced.value ? 0 : 1.1 },
  );
}
function categoryKeys(event: KeyboardEvent) {
  const index = settingsCategories.findIndex(
    (category) => category.id === navigation.category,
  );
  const next =
    event.key === "ArrowDown" || event.key === "ArrowRight"
      ? (index + 1) % settingsCategories.length
      : event.key === "ArrowUp" || event.key === "ArrowLeft"
        ? (index - 1 + settingsCategories.length) % settingsCategories.length
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? settingsCategories.length - 1
            : -1;
  if (next < 0) return;
  event.preventDefault();
  chooseCategory(settingsCategories[next].id);
  root.value
    ?.querySelector<HTMLElement>(`[data-category="${navigation.category}"]`)
    ?.focus();
}
function searchKeys(event: KeyboardEvent) {
  if (event.key === "Escape" && navigation.query) {
    event.preventDefault();
    event.stopPropagation();
    navigation.query = "";
    root.value?.querySelector<HTMLInputElement>("#settings-search")?.focus();
  } else if (
    event.key === "ArrowDown" &&
    searching.value &&
    event.target === root.value?.querySelector("#settings-search")
  ) {
    event.preventDefault();
    root.value?.querySelector<HTMLElement>(".settings-result")?.focus();
  }
}
function resultKeys(event: KeyboardEvent) {
  const buttons = [
    ...root.value!.querySelectorAll<HTMLButtonElement>(".settings-result"),
  ];
  const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const next = index + (event.key === "ArrowDown" ? 1 : -1);
    if (next < 0)
      root.value?.querySelector<HTMLInputElement>("#settings-search")?.focus();
    else buttons[Math.min(next, buttons.length - 1)]?.focus();
  }
}
const isActive = computed(() => state.page === "settings");
</script>

<template>
  <motion.section
    id="page-settings"
    class="page settings-page"
    :class="{ 'is-active': isActive }"
    :inert="!isActive"
    :aria-hidden="!isActive || undefined"
    :initial="false"
    :animate="{
      opacity: isActive ? 1 : 0,
      x: isActive ? 0 : 22,
      visibility: isActive ? 'visible' : 'hidden',
    }"
    :transition="surface"
    aria-labelledby="settings-title"
  >
    <div ref="root" class="settings-workspace" @keydown="searchKeys">
      <header class="page-head settings-head">
        <div class="page-head__copy">
          <h2 id="settings-title" tabindex="-1">设置</h2>
        </div>
        <div class="settings-head__tools">
          <span class="settings-save" role="status" aria-live="polite">{{
            ui.saveState === "saving"
              ? "正在保存"
              : ui.saveState === "saved"
                ? "已保存"
                : ui.saveState === "error"
                  ? "保存失败"
                  : ""
          }}</span>
          <BaseButton
            id="settings-redetect"
            data-pad-primary
            data-pad-action="重新检测"
            variant="ghost"
            icon="i-arrows-clockwise"
            :busy="ui.redetecting"
            @click="redetect"
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
          id="settings-search"
          v-model="navigation.query"
          type="search"
          aria-label="搜索设置"
          placeholder="搜索设置"
          autocomplete="off"
          :aria-controls="searching ? 'settings-results' : undefined"
        />
        <BaseButton
          v-if="navigation.query"
          class="settings-search__clear"
          variant="ghost"
          size="sm"
          icon="i-x"
          icon-only
          aria-label="清除搜索"
          @click="navigation.query = ''"
        />
      </div>
      <div class="settings-frame">
        <nav
          class="settings-categories"
          aria-label="设置分类"
          @keydown="categoryKeys"
        >
          <motion.button
            v-for="category in settingsCategories"
            :key="category.id"
            type="button"
            class="settings-category category-tab"
            :data-category="category.id"
            :aria-current="
              !searching && navigation.category === category.id
                ? 'true'
                : undefined
            "
            :tabindex="navigation.category === category.id ? 0 : -1"
            :while-press="reduced ? undefined : { scale: 0.97 }"
            :transition="control"
            :aria-controls="`settings-panel-${category.id}`"
            @click="chooseCategory(category.id)"
          >
            <motion.span
              v-if="navigation.category === category.id"
              class="settings-category__focus category-tab__selection"
              layout-id="settings-category-focus"
              :layout-dependency="navigation.category"
              :animate="{ opacity: searching ? 0.35 : 1 }"
              :transition="control"
            />
            <svg class="icon" aria-hidden="true">
              <use :href="`#${category.icon}`" /></svg
            ><span>{{ category.label }}</span>
          </motion.button>
        </nav>
        <form
          id="settings-form"
          class="settings-stage"
          :data-save-state="ui.saveState"
          @submit.prevent
        >
          <fieldset class="settings-fields" :disabled="runtimeEnvironmentBusy">
            <motion.div
              v-for="category in settingsCategories"
              :key="category.id"
              :id="`settings-panel-${category.id}`"
              class="settings-panel"
              :class="{
                'is-current': !searching && navigation.category === category.id,
              }"
              role="region"
              :aria-label="category.label"
              :aria-hidden="
                searching || navigation.category !== category.id || undefined
              "
              :inert="searching || navigation.category !== category.id"
              :style="{
                pointerEvents:
                  !searching && navigation.category === category.id
                    ? 'auto'
                    : 'none',
                zIndex:
                  !searching && navigation.category === category.id ? 1 : 0,
              }"
              :initial="false"
              :animate="{
                opacity:
                  !searching && navigation.category === category.id ? 1 : 0,
                x: !searching && navigation.category === category.id ? 0 : 12,
              }"
              :transition="surface"
            >
              <GamepadSettings
                v-if="visited.has(category.id) && category.id === 'gamepad'"
              />
              <template
                v-else-if="
                  visited.has(category.id) && category.id === 'appearance'
                "
              >
                <SettingGroup title="外观">
                  <SettingRow
                    data-setting="theme"
                    tabindex="-1"
                    :label="entry('theme').label"
                    :description="entry('theme').description"
                    for="set-theme-trigger"
                    v-slot="{ describedby }"
                  >
                    <BaseSelect
                      id="set-theme"
                      label="主题"
                      v-model="form.theme"
                      :options="themeOptions"
                      :aria-describedby="describedby"
                      @change="
                        applyTheme(form.theme);
                        queueSettingsSave(0);
                      "
                    />
                  </SettingRow>
                  <SettingRow
                    data-setting="media"
                    tabindex="-1"
                    :label="entry('media').label"
                    :description="entry('media').description"
                    for="set-hero-media-trigger"
                    v-slot="{ describedby }"
                  >
                    <BaseSelect
                      id="set-hero-media"
                      label="主页主视觉"
                      v-model="form.heroMedia"
                      :options="mediaOptions"
                      :aria-describedby="describedby"
                      @change="
                        applyHeroMedia(form.heroMedia);
                        queueSettingsSave(0);
                      "
                    />
                  </SettingRow>
                  <SettingRow
                    data-setting="bulletins"
                    tabindex="-1"
                    :label="entry('bulletins').label"
                    :description="entry('bulletins').description"
                    for="set-stack-bulletins"
                    v-slot="{ describedby }"
                  >
                    <BaseSwitch
                      id="set-stack-bulletins"
                      v-model="form.stackBulletins"
                      :label="entry('bulletins').label"
                      :describedby="describedby"
                      @change="queueSettingsSave(0)"
                    />
                  </SettingRow>
                  <SettingRow
                    data-setting="motion"
                    tabindex="-1"
                    :label="entry('motion').label"
                    :description="entry('motion').description"
                    for="set-motion"
                    v-slot="{ describedby }"
                  >
                    <BaseSwitch
                      id="set-motion"
                      v-model="form.reduceMotion"
                      label="减少动效"
                      :describedby="describedby"
                      @change="
                        applyReduceMotion(form.reduceMotion);
                        applyHeroMedia(form.heroMedia);
                        queueSettingsSave(0);
                      "
                    />
                  </SettingRow>
                </SettingGroup>
                <SettingGroup title="窗口行为">
                  <SettingRow
                    data-setting="tray"
                    tabindex="-1"
                    :label="entry('tray').label"
                    :description="entry('tray').description"
                    for="set-minimize-to-tray"
                    v-slot="{ describedby }"
                  >
                    <BaseSwitch
                      id="set-minimize-to-tray"
                      v-model="form.minimizeToTrayOnClose"
                      :label="entry('tray').label"
                      :describedby="describedby"
                      @change="queueSettingsSave(0)"
                    />
                  </SettingRow>
                  <SettingRow
                    data-setting="return"
                    tabindex="-1"
                    :label="entry('return').label"
                    :description="entry('return').description"
                    for="set-show-after-game-exit"
                    v-slot="{ describedby }"
                  >
                    <BaseSwitch
                      id="set-show-after-game-exit"
                      v-model="form.showLauncherAfterGameExit"
                      :label="entry('return').label"
                      :describedby="describedby"
                      @change="queueSettingsSave(0)"
                    />
                  </SettingRow>
                </SettingGroup>
              </template>
              <template
                v-else-if="visited.has(category.id) && category.id === 'game'"
              >
                <SettingGroup title="游戏安装">
                  <p
                    v-if="settingsPathsLocked"
                    class="settings-lock"
                    role="status"
                  >
                    {{
                      state.game !== "idle" || runtimeEnvironmentBusy
                        ? "游戏或运行环境任务进行中, 暂时无法修改运行路径."
                        : "游戏资源任务进行中, 暂时无法修改游戏地区和安装目录."
                    }}
                  </p>
                  <SettingRow
                    data-setting="distribution"
                    tabindex="-1"
                    label="游戏地区"
                    for="set-distribution-trigger"
                    ><BaseSelect
                      id="set-distribution"
                      label="游戏地区"
                      v-model="form.distribution"
                      :options="distributionOptions"
                      :disabled="settingsPathsLocked"
                      @change="queueSettingsSave(0)"
                  /></SettingRow>
                  <div
                    class="settings-path"
                    data-setting="install"
                    tabindex="-1"
                  >
                    <DirectoryField
                      id="set-install"
                      v-model="form.installRoot"
                      label="国服安装目录"
                      placeholder="/path/to/Neverness To Everness"
                      :disabled="settingsPathsLocked"
                      @input="queueSettingsSave()"
                      @change="queueSettingsSave(0)"
                    />
                  </div>
                  <div
                    class="settings-path"
                    data-setting="global-install"
                    tabindex="-1"
                  >
                    <DirectoryField
                      id="set-global-install"
                      v-model="form.globalInstallRoot"
                      label="国际服安装目录"
                      placeholder="~/Games/Neverness To Everness Global"
                      :disabled="settingsPathsLocked"
                      @input="queueSettingsSave()"
                      @change="queueSettingsSave(0)"
                    />
                  </div>
                </SettingGroup>
                <SettingGroup title="启动"
                  ><SettingRow
                    data-setting="launch"
                    tabindex="-1"
                    label="启动方式"
                    :description="launchModeHint"
                    for="set-mode-trigger"
                    ><BaseSelect
                      id="set-mode"
                      label="启动方式"
                      v-model="form.launchMode"
                      :options="launchOptions"
                      :disabled="runtimePathsLocked"
                      @change="queueSettingsSave(0)" /></SettingRow
                ></SettingGroup>
              </template>
              <div
                v-else-if="
                  visited.has(category.id) && category.id === 'runtime'
                "
                data-setting="runtime"
                tabindex="-1"
              >
                <RuntimeEnvironments />
              </div>
              <template
                v-else-if="
                  visited.has(category.id) && category.id === 'account'
                "
              >
                <div data-setting="account" tabindex="-1">
                  <TaygedoSettings
                    v-if="isActive && form.distribution === 'cn'"
                  />
                  <SettingGroup
                    v-else-if="form.distribution === 'global'"
                    title="账号与签到"
                    ><p class="field__hint">
                      塔吉多签到和实时数据仅适用于国服.
                      国际服请使用官方账号服务.
                    </p>
                    <BaseButton
                      variant="ghost"
                      icon="i-game-controller"
                      @click="chooseCategory('game')"
                      >选择游戏地区</BaseButton
                    ></SettingGroup
                  >
                </div>
              </template>
              <template v-else-if="visited.has(category.id)">
                <div data-setting="logs" tabindex="-1"><LogExport /></div>
                <div data-setting="diagnostics" tabindex="-1">
                  <SettingGroup title="运行诊断"
                    ><div id="settings-checks" class="checks">
                      <div
                        v-for="(item, index) in state.bootstrap?.checks ?? []"
                        :key="index"
                        class="check-row"
                      >
                        <i
                          class="check-dot"
                          :class="item.ok ? 'is-ok' : 'is-bad'"
                          aria-hidden="true"
                        />
                        <div class="check-copy">
                          <strong>{{ item.label }}</strong
                          ><span>{{ item.detail }}</span>
                        </div>
                        <span
                          class="settings-check-status"
                          :class="{ 'is-bad': !item.ok }"
                          >{{ item.ok ? "正常" : "待处理" }}</span
                        >
                      </div>
                    </div></SettingGroup
                  >
                </div>
              </template>
            </motion.div>
          </fieldset>
          <div
            v-if="searching"
            id="settings-results"
            class="settings-search-results"
            @keydown="resultKeys"
          >
            <p class="settings-results-count" role="status">
              {{
                results.length
                  ? `${results.length} 项匹配设置`
                  : `未找到“${navigation.query.trim()}”`
              }}
            </p>
            <button
              v-for="item in results"
              :key="item.id"
              class="settings-result"
              type="button"
              @click="locate(item)"
            >
              <span
                ><strong>{{ item.label }}</strong
                ><small
                  >{{
                    settingsCategories.find(
                      (category) => category.id === item.category,
                    )!.label
                  }}
                  · {{ item.description }}</small
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
