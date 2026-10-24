<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

interface Option {
  label: string;
  value: string;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: Option[];
    id?: string;
    disabled?: boolean;
    placeholder?: string;
  }>(),
  {
    id: undefined,
    disabled: false,
    placeholder: "请选择",
  },
);

const emit = defineEmits<{
  "update:modelValue": [value: string];
  change: [value: string];
}>();

const isOpen = ref(false);
const rootRef = ref<HTMLElement | null>(null);

const currentLabel = computed(() => {
  const selected = props.options.find((opt) => opt.value === props.modelValue);
  return selected ? selected.label : props.placeholder;
});

function toggle() {
  if (props.disabled) return;
  isOpen.value = !isOpen.value;
}

function selectOption(val: string) {
  if (props.disabled) return;
  isOpen.value = false;
  if (val !== props.modelValue) {
    emit("update:modelValue", val);
    emit("change", val);
  }
}

function onNativeChange(event: Event) {
  const target = event.target as HTMLSelectElement;
  if (target && target.value !== props.modelValue) {
    emit("update:modelValue", target.value);
    emit("change", target.value);
  }
}

function onClickOutside(event: MouseEvent) {
  if (rootRef.value && !rootRef.value.contains(event.target as Node)) {
    isOpen.value = false;
  }
}

function onKeydown(event: KeyboardEvent) {
  if (props.disabled) return;
  if (event.key === "Escape") {
    isOpen.value = false;
  } else if (event.key === "Enter" || event.key === " ") {
    if (!isOpen.value) {
      event.preventDefault();
      isOpen.value = true;
    }
  } else if (event.key === "ArrowDown") {
    event.preventDefault();
    if (!isOpen.value) {
      isOpen.value = true;
    } else {
      const idx = props.options.findIndex((o) => o.value === props.modelValue);
      if (idx < props.options.length - 1) {
        selectOption(props.options[idx + 1].value);
      }
    }
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    if (!isOpen.value) {
      isOpen.value = true;
    } else {
      const idx = props.options.findIndex((o) => o.value === props.modelValue);
      if (idx > 0) {
        selectOption(props.options[idx - 1].value);
      }
    }
  }
}

onMounted(() => {
  document.addEventListener("pointerdown", onClickOutside);
});

onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onClickOutside);
});
</script>

<template>
  <div
    ref="rootRef"
    class="custom-select"
    :class="{ 'is-open': isOpen, 'is-disabled': disabled }"
  >
    <button
      type="button"
      class="select-trigger"
      :id="id ? `${id}-trigger` : undefined"
      :aria-expanded="isOpen"
      :aria-haspopup="'listbox'"
      :disabled="disabled"
      @click="toggle"
      @keydown="onKeydown"
    >
      <span class="select-trigger__text">{{ currentLabel }}</span>
      <svg
        class="icon select-trigger__arrow"
        viewBox="0 0 256 256"
        aria-hidden="true"
      >
        <use href="#i-caret-down"></use>
      </svg>
    </button>

    <Transition name="select-popover">
      <div v-if="isOpen" class="select-dropdown" role="listbox">
        <div
          v-for="opt in options"
          :key="opt.value"
          class="select-option"
          :class="{ 'is-selected': opt.value === modelValue }"
          role="option"
          :aria-selected="opt.value === modelValue"
          @click="selectOption(opt.value)"
        >
          <span class="select-option__text">{{ opt.label }}</span>
          <svg
            v-if="opt.value === modelValue"
            class="icon select-option__check"
            viewBox="0 0 256 256"
            aria-hidden="true"
          >
            <use href="#i-check-circle"></use>
          </svg>
        </div>
      </div>
    </Transition>

    <select
      :id="id"
      :value="modelValue"
      :disabled="disabled"
      tabindex="-1"
      aria-hidden="true"
      class="select-native-hidden"
      @change="onNativeChange"
    >
      <option v-for="opt in options" :key="opt.value" :value="opt.value">
        {{ opt.label }}
      </option>
    </select>
  </div>
</template>
