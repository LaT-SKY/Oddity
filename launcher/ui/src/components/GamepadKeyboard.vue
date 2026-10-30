<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { motion } from "motion-v";
import { gamepad } from "../core/gamepad";
import { useControlMotion } from "../composables/useControlMotion";
import BaseButton from "./BaseButton.vue";
const draft = ref("");
const shift = ref(false);
const symbols = ref(false);
const error = ref("");
watch(draft, () => {
  error.value = "";
});
const { surface, control } = useControlMotion();
watch(
  () => gamepad.editing,
  (field) => {
    if (field) {
      draft.value = field.value;
      shift.value = false;
      symbols.value = false;
      error.value = "";
    }
  },
);
const numeric = computed(
  () =>
    gamepad.editing instanceof HTMLInputElement &&
    ["number", "tel", "time"].includes(gamepad.editing.type),
);
const time = computed(
  () =>
    gamepad.editing instanceof HTMLInputElement &&
    gamepad.editing.type === "time",
);
const rows = computed(() =>
  numeric.value
    ? ["123", "456", "789", time.value ? "0:" : "0.-"]
    : symbols.value
      ? ["!@#$%^&*()", "_-+=[]{}", ":;\"'/?\\", ".,<>|~`"]
      : ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"],
);
const display = computed(() =>
  gamepad.editing instanceof HTMLInputElement &&
  gamepad.editing.type === "password"
    ? "•".repeat(draft.value.length)
    : draft.value || (time.value ? "HH:mm" : "输入内容"),
);
function commit() {
  const field = gamepad.editing;
  if (!field) return;
  if (time.value && !/^([01]\d|2[0-3]):[0-5]\d$/.test(draft.value)) {
    error.value = "请输入 24 小时时间 (HH:mm).";
    return;
  }
  field.value = draft.value;
  field.dispatchEvent(new Event("input", { bubbles: true }));
  field.dispatchEvent(new Event("change", { bubbles: true }));
  gamepad.editing = null;
  field.focus({ preventScroll: true });
}
</script>
<template>
  <div
    v-if="gamepad.editing"
    class="pad-keyboard-backdrop"
    data-pad-modal
    @keydown.esc.prevent.stop="gamepad.editing = null"
  >
    <motion.section
      class="pad-keyboard"
      role="dialog"
      aria-modal="true"
      :aria-label="time ? '设置时间' : '屏幕键盘'"
      :initial="{ opacity: 0, y: 24 }"
      :animate="{ opacity: 1, y: 0 }"
      :transition="surface"
    >
      <header>
        <h2>{{ time ? "设置时间" : "屏幕键盘" }}</h2>
        <BaseButton
          icon="i-x"
          icon-only
          size="sm"
          data-pad-close
          aria-label="关闭屏幕键盘"
          @click="gamepad.editing = null"
        />
      </header>
      <output class="pad-keyboard__display">{{ display }}</output>
      <p v-if="error" class="field__hint" role="alert">{{ error }}</p>
      <div class="pad-keyboard__keys">
        <div
          v-for="(row, index) in rows"
          :key="index"
          class="pad-keyboard__row"
        >
          <motion.button
            v-for="key in row"
            :key="key"
            type="button"
            :while-press="{ scale: 0.9 }"
            :transition="control"
            @click="draft += shift ? key.toUpperCase() : key"
            >{{ shift ? key.toUpperCase() : key }}</motion.button
          >
        </div>
        <div class="pad-keyboard__row pad-keyboard__actions">
          <BaseButton v-if="!numeric" @click="shift = !shift">{{
            shift ? "小写" : "大写"
          }}</BaseButton
          ><BaseButton v-if="!numeric" @click="symbols = !symbols">{{
            symbols ? "字母" : "符号"
          }}</BaseButton
          ><BaseButton v-if="!numeric" @click="draft += ' '">空格</BaseButton
          ><BaseButton
            v-if="time"
            @click="
              draft = '';
              error = '';
            "
            >清空</BaseButton
          ><BaseButton @click="draft = draft.slice(0, -1)">删除</BaseButton
          ><BaseButton variant="primary" @click="commit">完成</BaseButton>
        </div>
      </div>
    </motion.section>
  </div>
</template>
