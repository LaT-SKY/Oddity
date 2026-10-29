<script setup lang="ts">
import { ref } from "vue";
import { open, save } from "../core/filePicker";
import { gamepad, gamepadFeedback } from "../core/gamepad";
import { form, queueSettingsSave, saveSettings } from "../core/controller";
import { invoke } from "../core/api";
import type { Feedback } from "../types/gamepad";
import BaseButton from "./BaseButton.vue";
import BaseSelect from "./BaseSelect.vue";
import BaseSwitch from "./BaseSwitch.vue";
import SettingGroup from "./settings/SettingGroup.vue";
import SettingRow from "./settings/SettingRow.vue";
import GamepadGlyph from "./GamepadGlyph.vue";
const effect = ref<Feedback>("confirm");
const message = ref("");
const busy = ref(false);
const iconOptions = [
  { value: "auto", label: "自动识别" },
  { value: "dualsense", label: "DualSense" },
  { value: "ds4", label: "DualShock 4" },
  { value: "xbox", label: "Xbox" },
  { value: "switch", label: "Nintendo Switch" },
];
const effects = [
  { value: "confirm", label: "确认" },
  { value: "complete", label: "完成" },
  { value: "error", label: "错误" },
];
async function test() {
  busy.value = true;
  message.value = "";
  if (await saveSettings(true)) await gamepadFeedback(effect.value);
  busy.value = false;
}
async function importProfile() {
  const path = await open({
    multiple: false,
    filters: [{ name: "振动配置", extensions: ["json"] }],
  });
  if (!path) return;
  const profile = await invoke("gamepad_profile_load", { path }).catch(
    (error) => {
      message.value = String(error);
      return null;
    },
  );
  if (profile) {
    form.gamepad.profiles[effect.value] = profile;
    queueSettingsSave(0);
    message.value = "已导入反馈配置.";
  }
}
async function exportProfile() {
  const path = await save({
    defaultPath: `oddity-${effect.value}.json`,
    filters: [{ name: "振动配置", extensions: ["json"] }],
  });
  if (!path) return;
  await invoke("gamepad_profile_save", {
    path,
    profile: JSON.parse(JSON.stringify(form.gamepad.profiles[effect.value])),
  })
    .then(() => {
      message.value = "已导出反馈配置.";
    })
    .catch((error) => {
      message.value = String(error);
    });
}
</script>
<template>
  <section data-setting="gamepad" tabindex="-1">
    <SettingGroup title="手柄">
      <div class="gamepad-connection">
        <svg class="icon" aria-hidden="true">
          <use href="#i-game-controller" /></svg
        ><span>{{ gamepad.device?.name ?? "未连接手柄" }}</span
        ><span v-if="gamepad.device">已连接</span>
      </div>
      <p v-if="gamepad.device" class="gamepad-settings-hint">
        {{
          [
            gamepad.device.rumble ? "双通道振动" : "",
            gamepad.device.triggerRumble ? "扳机振动" : "",
            gamepad.device.adaptive ? "自适应扳机" : "",
            gamepad.device.audio ? "USB 音频触觉" : "",
          ]
            .filter(Boolean)
            .join(" / ") || "当前设备未提供触觉反馈"
        }}
      </p>
      <SettingRow label="启用手柄操作" for="set-gamepad-enabled"
        ><BaseSwitch
          id="set-gamepad-enabled"
          v-model="form.gamepad.enabled"
          label="启用手柄操作"
          @change="queueSettingsSave(0)"
      /></SettingRow>
      <SettingRow label="按键图标" for="set-gamepad-icons-trigger"
        ><BaseSelect
          id="set-gamepad-icons"
          v-model="form.gamepad.icons"
          :options="iconOptions"
          label="按键图标"
          @change="queueSettingsSave(0)"
      /></SettingRow>
      <div class="gamepad-icon-preview">
        <span><GamepadGlyph button="confirm" />确认</span
        ><span><GamepadGlyph button="back" />返回</span
        ><span><GamepadGlyph button="west" />启动</span
        ><span><GamepadGlyph button="north" />玩家信息</span>
      </div>
      <SettingRow label="左摇杆死区" for="set-gamepad-deadzone"
        ><div class="gamepad-strength">
          <input
            id="set-gamepad-deadzone"
            type="range"
            min="10"
            max="75"
            step="1"
            :value="Math.round(form.gamepad.deadZone * 100)"
            @input="
              form.gamepad.deadZone =
                ($event.target as HTMLInputElement).valueAsNumber / 100;
              queueSettingsSave();
            "
            @change="queueSettingsSave(0)"
          /><output>{{ Math.round(form.gamepad.deadZone * 100) }}%</output>
        </div></SettingRow
      >
      <p class="gamepad-settings-hint">
        肩键切页. 分类栏中确认进入, 返回退出. R3 执行页面主操作.
      </p>
    </SettingGroup>
    <p class="gamepad-settings-hint" role="status">
      {{
        gamepad.idle.error
          ? `无法保持唤醒: ${gamepad.idle.error}`
          : gamepad.idle.requested
            ? "已请求防止熄屏与空闲休眠."
            : "手柄操作期间自动保持唤醒, 停止操作 60 秒后恢复桌面空闲策略."
      }}
    </p>
    <SettingGroup title="触觉反馈" data-pad-feedback="off">
      <SettingRow label="手柄振动" for="set-gamepad-vibration"
        ><BaseSwitch
          id="set-gamepad-vibration"
          v-model="form.gamepad.vibration"
          label="手柄振动"
          @change="
            queueSettingsSave(0);
            gamepadFeedback('stop');
          "
      /></SettingRow>
      <SettingRow label="整体增益" for="set-gamepad-strength"
        ><div class="gamepad-strength">
          <input
            id="set-gamepad-strength"
            v-model.number="form.gamepad.strength"
            type="range"
            min="0"
            max="100"
            step="1"
            :disabled="!form.gamepad.vibration"
            @input="queueSettingsSave()"
            @change="queueSettingsSave(0)"
          /><output>{{ form.gamepad.strength }}%</output>
        </div></SettingRow
      >
      <SettingRow label="反馈场景" for="set-gamepad-event-trigger"
        ><BaseSelect
          id="set-gamepad-event"
          v-model="effect"
          :options="effects"
          label="反馈场景"
      /></SettingRow>
      <SettingRow label="反馈配置"
        ><div class="gamepad-profile-actions">
          <span class="gamepad-profile-name">{{
            form.gamepad.profiles[effect].name
          }}</span
          ><BaseButton
            :disabled="!gamepad.device || !form.gamepad.vibration"
            :busy="busy"
            @click="test"
            >测试</BaseButton
          ><BaseButton @click="gamepadFeedback('stop')">停止</BaseButton>
        </div></SettingRow
      >
      <div class="gamepad-profile-actions">
        <BaseButton icon="i-download-simple" @click="importProfile"
          >导入配置</BaseButton
        ><BaseButton @click="exportProfile">导出配置</BaseButton>
      </div>
      <p class="gamepad-settings-hint">
        支持微调页导出的包络、USB 音频与自适应扳机配置.
        设备能力不足时会说明原因.
      </p>
      <p
        v-if="message || gamepad.error"
        class="gamepad-settings-hint"
        role="status"
      >
        {{ message || gamepad.error }}
      </p>
    </SettingGroup>
  </section>
</template>
