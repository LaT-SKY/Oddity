<script setup lang="ts">
import BaseButton from "./BaseButton.vue";
import BaseInput from "./BaseInput.vue";
import { ref } from "vue";
import { homeDir, join } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";

const props = defineProps<{
  id: string;
  modelValue: string;
  label: string;
  placeholder?: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{
  "update:modelValue": [value: string];
  input: [];
  change: [];
}>();
const picking = ref(false);
const error = ref("");

async function browse() {
  if (props.disabled || picking.value) return;
  picking.value = true;
  error.value = "";
  try {
    let defaultPath = props.modelValue.trim();
    if (defaultPath === "~" || defaultPath.startsWith("~/")) {
      defaultPath = await join(await homeDir(), defaultPath.slice(2));
    }
    const selected = await open({
      title: `选择${props.label}`,
      directory: true,
      multiple: false,
      defaultPath: defaultPath.startsWith("/") ? defaultPath : undefined,
    });
    // 对话框打开期间, 资源任务可能锁定安装目录.
    if (selected !== null && !props.disabled) {
      emit("update:modelValue", selected);
      emit("change");
    }
  } catch (cause) {
    error.value = `无法打开目录选择框: ${String(cause)}`;
  } finally {
    picking.value = false;
  }
}
</script>

<template>
  <div class="field">
    <label :for="id">{{ label }}</label>
    <div class="directory-field__controls">
      <BaseInput
        :id="id"
        :model-value="modelValue"
        mono
        :invalid="!!error"
        :aria-describedby="error ? `${id}-error` : undefined"
        type="text"
        spellcheck="false"
        autocomplete="off"
        :placeholder="placeholder"
        :disabled="disabled || picking"
        @update:model-value="emit('update:modelValue', $event)"
        @input="emit('input')"
        @change="emit('change')"
      />
      <BaseButton
        type="button"
        variant="secondary"
        icon="i-folder-open"
        :disabled="disabled || picking"
        :aria-label="`浏览${label}`"
        :busy="picking"
        @click="browse"
      >
        浏览
      </BaseButton>
    </div>
    <small v-if="error" :id="`${id}-error`" class="field__hint" role="alert">{{
      error
    }}</small>
  </div>
</template>
