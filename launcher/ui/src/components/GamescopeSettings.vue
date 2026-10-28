<script setup lang="ts">
import { computed } from "vue";
import { form, queueSettingsSave, state } from "../core/controller";
import BaseSelect from "./BaseSelect.vue";
import BaseSwitch from "./BaseSwitch.vue";
import SettingGroup from "./settings/SettingGroup.vue";
import SettingRow from "./settings/SettingRow.vue";
import type { GamescopeOptions } from "../types/launcher";

defineProps<{ available: boolean }>();
const options = computed(() => form.gameMode.gamescope);
const windows = [
  { label: "全屏", value: "fullscreen" },
  { label: "无边框", value: "borderless" },
  { label: "窗口", value: "windowed" },
];
const scalers = [
  { label: "线性", value: "linear" },
  { label: "最近邻", value: "nearest" },
  { label: "FSR", value: "fsr" },
  { label: "NIS", value: "nis" },
  { label: "整数缩放", value: "integer" },
];
const resolutions = [
  { label: "游戏分辨率", width: "gameWidth", height: "gameHeight" },
  { label: "输出分辨率", width: "outputWidth", height: "outputHeight" },
] as const;
function number(
  event: Event,
  field: "gameWidth" | "gameHeight" | "outputWidth" | "outputHeight" | "fps",
) {
  const input = event.target as HTMLInputElement;
  const value = input.value === "" ? 0 : input.valueAsNumber;
  if (!Number.isInteger(value) || !input.checkValidity()) {
    input.reportValidity();
    input.value = String(options.value[field]);
    return;
  }
  options.value[field] = value;
  queueSettingsSave(0);
}
function select(field: "windowMode" | "scaling", value: string) {
  if (field === "windowMode")
    options.value.windowMode = value as GamescopeOptions["windowMode"];
  else options.value.scaling = value as GamescopeOptions["scaling"];
  queueSettingsSave(0);
}
</script>

<template>
  <SettingGroup title="Gamescope">
    <SettingRow
      data-game-setting="gamescope"
      tabindex="-1"
      label="启用 Gamescope"
      :description="
        available ? '独立控制游戏窗口、分辨率与帧率.' : '需安装 Gamescope.'
      "
      for="game-mode-gamescope"
      :class="{ 'is-unavailable': !available }"
    >
      <template #default="{ describedby }"
        ><BaseSwitch
          id="game-mode-gamescope"
          v-model="options.enabled"
          :describedby="describedby"
          :disabled="!state.bootstrap || (!options.enabled && !available)"
          @change="queueSettingsSave(0)"
      /></template>
    </SettingRow>
    <div
      v-if="options.enabled"
      class="game-mode-details"
      :class="{ 'is-unavailable': !available }"
    >
      <fieldset class="game-mode-fields" :disabled="!available">
        <SettingRow label="窗口模式" for="gamescope-window-trigger"
          ><BaseSelect
            id="gamescope-window"
            label="窗口模式"
            :model-value="options.windowMode"
            :options="windows"
            @change="select('windowMode', $event)"
        /></SettingRow>
        <SettingRow
          v-for="size in resolutions"
          :key="size.width"
          :label="size.label"
          description="0 使用默认值."
        >
          <div class="game-mode-resolution">
            <input
              :id="`gamescope-${size.width}`"
              class="control-input"
              type="number"
              min="0"
              max="16384"
              step="1"
              :aria-label="`${size.label}宽度`"
              :value="options[size.width]"
              @change="number($event, size.width)"
            />
            <span aria-hidden="true">×</span>
            <input
              :id="`gamescope-${size.height}`"
              class="control-input"
              type="number"
              min="0"
              max="16384"
              step="1"
              :aria-label="`${size.label}高度`"
              :value="options[size.height]"
              @change="number($event, size.height)"
            />
          </div>
        </SettingRow>
        <SettingRow label="帧率限制" description="0 不限制." for="gamescope-fps"
          ><input
            id="gamescope-fps"
            class="control-input game-mode-number"
            type="number"
            min="0"
            max="1000"
            step="1"
            :value="options.fps"
            @change="number($event, 'fps')"
        /></SettingRow>
        <SettingRow label="缩放方式" for="gamescope-scaling-trigger"
          ><BaseSelect
            id="gamescope-scaling"
            label="缩放方式"
            :model-value="options.scaling"
            :options="scalers"
            @change="select('scaling', $event)"
        /></SettingRow>
      </fieldset>
    </div>
  </SettingGroup>
</template>
