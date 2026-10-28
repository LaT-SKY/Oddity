<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import { form, state } from "../core/controller";
import { invoke } from "../core/api";
import { notify } from "../core/notifications";
import type { GameModeTuning } from "../types/launcher";
import BaseButton from "./BaseButton.vue";
import BaseModal from "./BaseModal.vue";
import BaseSelect from "./BaseSelect.vue";
import SettingGroup from "./settings/SettingGroup.vue";
import SettingRow from "./settings/SettingRow.vue";

const props = defineProps<{ available: boolean }>();
const config = ref<GameModeTuning | null>(null);
const saved = ref("");
const loading = ref(false);
const applying = ref(false);
const confirming = ref(false);
let disposed = false;
const editable = computed(
  () =>
    props.available &&
    form.gameMode.performance &&
    config.value &&
    !loading.value &&
    !applying.value &&
    state.game === "idle",
);
const dirty = computed(
  () => config.value && JSON.stringify(config.value) !== saved.value,
);
const coreModes = reactive({ pinCores: "default", parkCores: "default" });
const coreOptions = [
  { label: "使用默认值", value: "default" },
  { label: "自动", value: "yes" },
  { label: "关闭", value: "no" },
  { label: "指定核心", value: "custom" },
];
const cores = [
  {
    key: "pinCores",
    label: "核心绑定",
    description: "将游戏限制在所选 CPU 核心上.",
  },
  {
    key: "parkCores",
    label: "核心停驻",
    description: "游戏运行时暂时停用所选核心.",
  },
] as const;
const priorities = [
  { label: "使用默认值", value: "default" },
  ...Array.from({ length: 21 }, (_, value) => ({
    label: value === 0 ? "不更改" : `${value} (nice -${value})`,
    value: String(value),
  })),
];
const ioPriorities = [
  { label: "使用默认值", value: "default" },
  { label: "不更改", value: "off" },
  { label: "跟随 CPU 优先级", value: "reset" },
  ...Array.from({ length: 8 }, (_, value) => ({
    label: value === 0 ? "0 (最高)" : String(value),
    value: String(value),
  })),
];
function coreMode(value: string | null) {
  if (value === null) return "default";
  if (["yes", "true", "1"].includes(value)) return "yes";
  if (["no", "false", "0"].includes(value)) return "no";
  return "custom";
}
async function reload() {
  if (loading.value || applying.value) return;
  loading.value = true;
  await invoke("game_mode_tuning_info").then(
    (result) => {
      if (disposed) return;
      config.value = result;
      for (const { key } of cores) coreModes[key] = coreMode(result[key]);
      saved.value = JSON.stringify(result);
    },
    (cause: unknown) => {
      if (!disposed)
        notify(
          cause instanceof Error ? cause.message : String(cause),
          "error",
          6500,
          { key: "game-mode-tuning" },
        );
    },
  );
  loading.value = false;
}
async function apply() {
  if (!editable.value || !config.value) return;
  confirming.value = false;
  applying.value = true;
  const selected = { ...config.value };
  for (const { key } of cores) {
    if (coreModes[key] === "custom" && ["0", "1"].includes(selected[key] ?? ""))
      selected[key] = `${selected[key]}-${selected[key]}`;
  }
  await invoke("game_mode_tuning_apply", { config: selected }).then(
    (result) => {
      if (disposed) return;
      config.value = result;
      for (const { key } of cores) coreModes[key] = coreMode(result[key]);
      saved.value = JSON.stringify(result);
      notify("CPU 与优先级配置已应用");
    },
    (cause: unknown) => {
      if (!disposed)
        notify(
          cause instanceof Error ? cause.message : String(cause),
          "error",
          6500,
          { key: "game-mode-tuning" },
        );
    },
  );
  applying.value = false;
}
watch(
  () => state.page,
  (page) => {
    if (page === "game-mode" && !config.value) void reload();
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  disposed = true;
});
defineExpose({ reload });
</script>

<template>
  <SettingGroup title="CPU 与优先级">
    <div
      class="game-mode-system game-mode-details"
      :class="{ 'is-unavailable': !available || !form.gameMode.performance }"
    >
      <fieldset class="game-mode-fields" :disabled="!editable">
        <template v-for="core in cores" :key="core.key">
          <SettingRow
            :data-game-setting="`cpu-${core.key}`"
            tabindex="-1"
            :label="core.label"
            :description="core.description"
            :for="`cpu-${core.key}-trigger`"
          >
            <BaseSelect
              :id="`cpu-${core.key}`"
              :label="core.label"
              :options="coreOptions"
              :model-value="coreModes[core.key]"
              :disabled="!editable"
              @change="
                coreModes[core.key] = $event;
                config &&
                  (config[core.key] =
                    $event === 'default'
                      ? null
                      : $event === 'custom'
                        ? ''
                        : $event);
              "
            />
          </SettingRow>
          <SettingRow
            v-if="config && coreModes[core.key] === 'custom'"
            :label="`${core.label}列表`"
            description="逻辑核心编号, 例如 0-3,8-11."
            :for="`cpu-${core.key}-list`"
          >
            <input
              :id="`cpu-${core.key}-list`"
              class="control-input"
              :value="config[core.key]"
              :aria-label="`${core.label}列表`"
              maxlength="4096"
              @input="
                config[core.key] = (
                  $event.target as HTMLInputElement
                ).value.trim()
              "
            />
          </SettingRow>
        </template>
        <SettingRow
          data-game-setting="cpu-renice"
          tabindex="-1"
          label="进程优先级"
          description="数值越大优先级越高, 需要 GameMode 权限支持."
          for="cpu-renice-trigger"
        >
          <BaseSelect
            id="cpu-renice"
            label="进程优先级"
            :options="priorities"
            :model-value="
              config?.renice === null || !config
                ? 'default'
                : String(config.renice)
            "
            :disabled="!editable"
            @change="
              config &&
              (config.renice = $event === 'default' ? null : Number($event))
            "
          />
        </SettingRow>
        <SettingRow
          data-game-setting="cpu-ioprio"
          tabindex="-1"
          label="I/O 优先级"
          description="数值越小优先级越高."
          for="cpu-ioprio-trigger"
        >
          <BaseSelect
            id="cpu-ioprio"
            label="I/O 优先级"
            :options="ioPriorities"
            :model-value="config?.ioprio ?? 'default'"
            :disabled="!editable"
            @change="
              config && (config.ioprio = $event === 'default' ? null : $event)
            "
          />
        </SettingRow>
      </fieldset>
      <div class="game-mode-gpu-actions">
        <BaseButton
          :busy="applying"
          :disabled="
            !editable ||
            !dirty ||
            config?.pinCores === '' ||
            config?.parkCores === ''
          "
          @click="confirming = true"
          >应用配置</BaseButton
        >
      </div>
    </div>
    <BaseModal
      v-model:open="confirming"
      title="应用 CPU 与优先级配置"
      :theme="form.theme"
      :reduce-motion="form.reduceMotion"
    >
      <p class="game-mode-confirm">
        将修改系统 GameMode 配置, 影响所有 GameMode 游戏.
      </p>
      <div class="game-mode-gpu-actions">
        <BaseButton variant="ghost" @click="confirming = false">取消</BaseButton
        ><BaseButton @click="apply">确认并授权</BaseButton>
      </div>
    </BaseModal>
  </SettingGroup>
</template>
