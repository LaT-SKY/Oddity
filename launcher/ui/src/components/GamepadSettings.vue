<script setup lang="ts">
import { gamepad, gamepadFeedback } from "../core/gamepad";
import { form, queueSettingsSave } from "../core/controller";
import BaseSelect from "./BaseSelect.vue";
import BaseSwitch from "./BaseSwitch.vue";
import SettingGroup from "./settings/SettingGroup.vue";
import SettingRow from "./settings/SettingRow.vue";
import GamepadGlyph from "./GamepadGlyph.vue";
const iconOptions = [
  { value: "auto", label: "自动识别" },
  { value: "dualsense", label: "DualSense" },
  { value: "ds4", label: "DualShock 4" },
  { value: "xbox", label: "Xbox" },
  { value: "switch", label: "Nintendo Switch" },
];
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
    </SettingGroup>
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
      <p v-if="gamepad.deviceError" class="gamepad-settings-hint" role="status">
        {{ gamepad.deviceError }}
      </p>
      <p
        v-if="gamepad.feedbackError"
        class="gamepad-settings-hint"
        role="status"
      >
        {{ gamepad.feedbackError }}
      </p>
    </SettingGroup>
  </section>
</template>
