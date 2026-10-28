<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { form, state } from "../core/controller";
import { invoke } from "../core/api";
import { notify } from "../core/notifications";
import type { GpuConfig, GpuInfo } from "../types/launcher";
import BaseButton from "./BaseButton.vue";
import BaseModal from "./BaseModal.vue";
import BaseSelect from "./BaseSelect.vue";
import BaseSwitch from "./BaseSwitch.vue";
import SettingGroup from "./settings/SettingGroup.vue";
import SettingRow from "./settings/SettingRow.vue";

const props = defineProps<{ available: boolean }>();
const info = ref<GpuInfo | null>(null);
const loading = ref(false);
const saved = ref("");
const dirty = computed(
  () => info.value && JSON.stringify(info.value.config) !== saved.value,
);
const applying = ref(false);
const confirming = ref(false);
let disposed = false;
const supported = computed(
  () => props.available && Boolean(info.value?.devices.length),
);
const editable = computed(
  () =>
    supported.value &&
    form.gameMode.performance &&
    !applying.value &&
    state.game === "idle",
);
const devices = computed(
  () =>
    info.value?.devices.map((device) => ({
      label: `${device.vendor === "nvidia" ? "NVIDIA" : "AMD"} GPU ${device.id}`,
      value: String(device.id),
    })) ?? [],
);
const powerModes = [
  { label: "不更改", value: "-1" },
  { label: "自适应", value: "0" },
  { label: "最高性能", value: "1" },
  { label: "自动", value: "2" },
];
const amdLevels = [
  { label: "自动", value: "auto" },
  { label: "高性能", value: "high" },
  { label: "低功耗", value: "low" },
  { label: "标准配置", value: "profileStandard" },
  { label: "峰值配置", value: "profilePeak" },
];
async function reload() {
  if (loading.value || applying.value) return;
  loading.value = true;
  await invoke("game_mode_gpu_info").then(
    (result) => {
      if (!disposed) {
        info.value = result;
        saved.value = JSON.stringify(result.config);
      }
    },
    (cause: unknown) => {
      if (!disposed)
        notify(
          cause instanceof Error ? cause.message : String(cause),
          "error",
          6500,
          { key: "game-mode-gpu" },
        );
    },
  );
  loading.value = false;
}
function device(value: string) {
  const selected = info.value?.devices.find(
    (device) => String(device.id) === value,
  );
  if (selected && info.value) {
    info.value.config.device = selected.id;
    info.value.config.vendor = selected.vendor;
  }
}
function number(event: Event, key: "coreOffset" | "memoryOffset") {
  if (!info.value) return;
  const input = event.target as HTMLInputElement;
  const value = input.valueAsNumber;
  if (!Number.isInteger(value) || !input.checkValidity()) {
    input.reportValidity();
    input.value = String(info.value.config[key]);
    return;
  }
  info.value.config[key] = value;
}
async function apply() {
  if (!info.value || !editable.value) return;
  applying.value = true;
  confirming.value = false;
  await invoke("game_mode_gpu_apply", {
    config: { ...info.value.config },
  }).then(
    (result) => {
      if (disposed) return;
      info.value = result;
      saved.value = JSON.stringify(result.config);
      notify("GPU 配置已应用");
    },
    (cause: unknown) => {
      if (!disposed)
        notify(
          cause instanceof Error ? cause.message : String(cause),
          "error",
          6500,
          { key: "game-mode-gpu" },
        );
    },
  );
  applying.value = false;
}
watch(
  () => state.page,
  (page) => {
    if (page === "game-mode" && !info.value) void reload();
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  disposed = true;
});
defineExpose({ reload });
</script>

<template>
  <SettingGroup title="GPU 超频">
    <div
      :class="{ 'is-unavailable': !supported || !form.gameMode.performance }"
      class="game-mode-gpu"
    >
      <SettingRow
        data-game-setting="gpu"
        tabindex="-1"
        label="启用 GPU 调优"
        :description="
          supported
            ? '由 GameMode 应用显卡频率与性能等级.'
            : '需要 GameMode 和支持的 NVIDIA / AMD 显卡.'
        "
        for="gpu-enabled"
      >
        <template #default="{ describedby }"
          ><BaseSwitch
            id="gpu-enabled"
            v-if="info"
            v-model="info.config.enabled"
            :describedby="describedby"
            :disabled="!editable" /><BaseSwitch
            v-else
            id="gpu-enabled"
            :model-value="false"
            :disabled="true"
            :describedby="describedby"
        /></template>
      </SettingRow>
      <fieldset v-if="info" class="game-mode-fields" :disabled="!editable">
        <div v-if="info.config.enabled" class="game-mode-details">
          <SettingRow label="显卡" for="gpu-device-trigger"
            ><BaseSelect
              id="gpu-device"
              label="显卡"
              :model-value="String(info.config.device)"
              :options="devices"
              :disabled="!editable"
              @change="device"
          /></SettingRow>
          <template v-if="info.config.vendor === 'nvidia'">
            <SettingRow label="PowerMizer" for="gpu-power-trigger"
              ><BaseSelect
                id="gpu-power"
                label="PowerMizer"
                :model-value="String(info.config.powerMode)"
                :options="powerModes"
                :disabled="!editable"
                @change="info.config.powerMode = Number($event)"
            /></SettingRow>
            <SettingRow
              label="核心偏移 (MHz)"
              description="-1 保持不变, 0 恢复默认."
              for="gpu-core"
              ><input
                id="gpu-core"
                class="control-input game-mode-number"
                type="number"
                min="-1"
                max="2147483647"
                step="1"
                :value="info.config.coreOffset"
                @change="number($event, 'coreOffset')"
            /></SettingRow>
            <SettingRow
              label="显存偏移 (MHz)"
              description="-1 保持不变, 0 恢复默认."
              for="gpu-memory"
              ><input
                id="gpu-memory"
                class="control-input game-mode-number"
                type="number"
                min="-1"
                max="2147483647"
                step="1"
                :value="info.config.memoryOffset"
                @change="number($event, 'memoryOffset')"
            /></SettingRow>
          </template>
          <SettingRow v-else label="性能等级" for="gpu-amd-trigger"
            ><BaseSelect
              id="gpu-amd"
              label="性能等级"
              :model-value="info.config.amdLevel"
              :options="amdLevels"
              :disabled="!editable"
              @change="info.config.amdLevel = $event as GpuConfig['amdLevel']"
          /></SettingRow>
        </div>
        <div class="game-mode-gpu-actions">
          <BaseButton
            :busy="applying"
            :disabled="!editable || !dirty"
            @click="confirming = true"
            >应用配置</BaseButton
          >
        </div>
      </fieldset>
    </div>
    <BaseModal
      v-model:open="confirming"
      title="应用 GPU 配置"
      :theme="form.theme"
      :reduce-motion="form.reduceMotion"
    >
      <p class="game-mode-confirm">
        将修改系统 GameMode 配置, 影响所有 GameMode 游戏.
        超频可能导致系统不稳定或硬件损坏.
      </p>
      <div class="game-mode-gpu-actions">
        <BaseButton variant="ghost" @click="confirming = false">取消</BaseButton
        ><BaseButton @click="apply">确认并授权</BaseButton>
      </div>
    </BaseModal>
  </SettingGroup>
</template>
