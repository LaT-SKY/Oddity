<script setup lang="ts">
import { AnimatePresence, motion, useDomRef } from "motion-v";
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useId,
  watch,
  type Ref,
} from "vue";
import { useControlMotion } from "../composables/useControlMotion";

defineOptions({ inheritAttrs: false });
interface Option {
  label: string;
  value: string;
  disabled?: boolean;
}
const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: Option[];
    id?: string;
    disabled?: boolean;
    placeholder?: string;
    label?: string;
  }>(),
  { placeholder: "请选择" },
);
const emit = defineEmits<{
  "update:modelValue": [value: string];
  change: [value: string];
}>();
const { control, surface } = useControlMotion();
const uid = useId();
const listId = `${uid}-options`;
const isOpen = ref(false);
const active = ref(-1);
const trigger = ref<HTMLButtonElement | null>(null);
const menu = useDomRef() as Ref<HTMLElement | null>;
const position = ref({
  left: "0px",
  top: "0px",
  width: "0px",
  maxHeight: "280px",
});
const placement = ref("bottom");
const theme = ref("light");
const portal = ref<Element>(document.body);
const currentLabel = computed(
  () =>
    props.options.find((option) => option.value === props.modelValue)?.label ??
    props.placeholder,
);
function place() {
  if (!isOpen.value || !trigger.value) return;
  const box = trigger.value.getBoundingClientRect();
  const below = window.innerHeight - box.bottom - 14;
  const above = box.top - 14;
  const desired = Math.min(props.options.length * 40 + 12, 280);
  const up = below < desired && above > below;
  const height = Math.max(0, Math.min(280, up ? above : below));
  placement.value = up ? "top" : "bottom";
  position.value = {
    left: `${Math.max(8, Math.min(box.left, window.innerWidth - box.width - 8))}px`,
    top: `${up ? box.top - Math.min(desired, height) - 6 : box.bottom + 6}px`,
    width: `${box.width}px`,
    maxHeight: `${height}px`,
  };
}
async function open() {
  if (props.disabled || trigger.value?.matches(":disabled")) return;
  active.value = props.options.findIndex(
    (option) => option.value === props.modelValue && !option.disabled,
  );
  if (active.value < 0)
    active.value = props.options.findIndex((option) => !option.disabled);
  theme.value =
    trigger.value?.closest<HTMLElement>("[data-theme]")?.dataset.theme ??
    "light";
  portal.value = trigger.value?.closest(".modal") ?? document.body;
  isOpen.value = true;
  place();
  await nextTick();
  menu.value
    ?.querySelector<HTMLElement>(`[data-index="${active.value}"]`)
    ?.scrollIntoView?.({ block: "nearest" });
}
function selectOption(option: Option) {
  if (props.disabled || option.disabled) return;
  isOpen.value = false;
  if (option.value !== props.modelValue) {
    emit("update:modelValue", option.value);
    emit("change", option.value);
  }
  trigger.value?.focus({ preventScroll: true });
}
function onNativeChange(event: Event) {
  const target = event.target as HTMLSelectElement;
  const option = props.options.find((option) => option.value === target.value);
  if (option) selectOption(option);
  target.value = props.modelValue;
}
function outside(event: PointerEvent) {
  const target = event.target as Node;
  if (!trigger.value?.contains(target) && !menu.value?.contains(target))
    isOpen.value = false;
}
function keydown(event: KeyboardEvent) {
  if (props.disabled) return;
  if (event.key === "Escape" && isOpen.value) {
    event.preventDefault();
    event.stopPropagation();
    isOpen.value = false;
  } else if (event.key === "Tab") isOpen.value = false;
  else if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    if (!isOpen.value) void open();
    else if (props.options[active.value])
      selectOption(props.options[active.value]);
  } else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
    event.preventDefault();
    if (!isOpen.value) {
      void open();
      return;
    }
    const direction = event.key === "ArrowUp" || event.key === "End" ? -1 : 1;
    let index =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? props.options.length - 1
          : active.value + direction;
    for (; index >= 0 && index < props.options.length; index += direction) {
      if (!props.options[index].disabled) {
        active.value = index;
        break;
      }
    }
    void nextTick(() =>
      menu.value
        ?.querySelector<HTMLElement>(`[data-index="${active.value}"]`)
        ?.scrollIntoView?.({ block: "nearest" }),
    );
  }
}
watch(
  () => props.disabled,
  (disabled) => {
    if (disabled) isOpen.value = false;
  },
);
onMounted(() => {
  document.addEventListener("pointerdown", outside);
  window.addEventListener("resize", place);
  window.addEventListener("scroll", place, true);
});
onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", outside);
  window.removeEventListener("resize", place);
  window.removeEventListener("scroll", place, true);
});
</script>
<template>
  <div
    class="custom-select"
    :class="{ 'is-open': isOpen, 'is-disabled': disabled }"
  >
    <button
      v-bind="$attrs"
      ref="trigger"
      type="button"
      class="select-trigger"
      :id="id ? `${id}-trigger` : undefined"
      role="combobox"
      :aria-label="label"
      :aria-expanded="isOpen"
      aria-haspopup="listbox"
      :aria-controls="isOpen ? listId : undefined"
      :aria-activedescendant="
        isOpen && active >= 0 ? `${uid}-${active}` : undefined
      "
      :disabled="disabled"
      @click="isOpen ? (isOpen = false) : open()"
      @keydown="keydown"
    >
      <span class="select-trigger__text">{{ currentLabel }}</span>
      <motion.svg
        :animate="{ rotate: isOpen ? 180 : 0 }"
        :transition="control"
        class="icon select-trigger__arrow"
        viewBox="0 0 256 256"
        aria-hidden="true"
        ><use href="#i-caret-down"
      /></motion.svg>
    </button>
    <Teleport :to="portal">
      <AnimatePresence>
        <motion.div
          v-if="isOpen"
          :id="listId"
          ref="menu"
          class="select-dropdown control-popover"
          :data-theme="theme"
          :data-placement="placement"
          role="listbox"
          :aria-label="label"
          :style="position"
          :initial="{
            opacity: 0,
            clipPath:
              placement === 'top'
                ? 'inset(100% 0 0 0 round 16px)'
                : 'inset(0 0 100% 0 round 16px)',
          }"
          :animate="{ opacity: 1, clipPath: 'inset(0% 0% 0% 0% round 16px)' }"
          :exit="{
            opacity: 0,
            clipPath:
              placement === 'top'
                ? 'inset(100% 0 0 0 round 16px)'
                : 'inset(0 0 100% 0 round 16px)',
          }"
          :transition="surface"
          @pointerdown.prevent
        >
          <div
            v-for="(option, index) in options"
            :id="`${uid}-${index}`"
            :key="option.value"
            class="select-option"
            :data-index="index"
            :class="{
              'is-selected': option.value === modelValue,
              'is-highlighted': index === active,
            }"
            role="option"
            :aria-selected="option.value === modelValue"
            :aria-disabled="option.disabled || undefined"
            @pointermove="!option.disabled && (active = index)"
            @click="selectOption(option)"
          >
            <motion.span
              v-if="index === active"
              class="select-option__focus"
              :layout-id="`${uid}-focus`"
              :transition="control"
            />
            <span class="select-option__text">{{ option.label }}</span>
            <svg
              v-if="option.value === modelValue"
              class="icon select-option__check"
              viewBox="0 0 256 256"
              aria-hidden="true"
            >
              <use href="#i-check-circle" />
            </svg>
          </div>
        </motion.div>
      </AnimatePresence>
    </Teleport>
    <select
      :id="id"
      :value="modelValue"
      :disabled="disabled"
      tabindex="-1"
      aria-hidden="true"
      hidden
      @change="onNativeChange"
    >
      <option
        v-for="option in options"
        :key="option.value"
        :value="option.value"
        :disabled="option.disabled"
      >
        {{ option.label }}
      </option>
    </select>
  </div>
</template>
