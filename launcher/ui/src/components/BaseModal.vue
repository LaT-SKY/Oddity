<script setup lang="ts">
import { nextTick, onBeforeUnmount, onUpdated, ref, useId, watch } from "vue";
const props = defineProps<{ open: boolean; title: string; theme?: string }>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();
const titleId = useId();
const panel = ref<HTMLElement | null>(null);
let previous: HTMLElement | null = null;
function close() {
  emit("update:open", false);
}
function restore() {
  if (previous?.isConnected) previous.focus();
  previous = null;
}
watch(
  () => props.open,
  async (open) => {
    if (open) {
      previous = document.activeElement as HTMLElement | null;
      await nextTick();
      if (props.open)
        panel.value?.querySelector<HTMLElement>("button")?.focus();
    } else {
      await nextTick();
      restore();
    }
  },
  { immediate: true },
);
onBeforeUnmount(restore);
// 切换表单或移除账号后, 将焦点留在弹窗内。
onUpdated(() => {
  if (
    props.open &&
    panel.value &&
    !panel.value.contains(document.activeElement)
  ) {
    panel.value.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
  }
});
function keydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    close();
  }
  if (event.key !== "Tab") return;
  const items = Array.from(
    panel.value?.querySelectorAll<HTMLElement>(
      'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]',
    ) ?? [],
  ).filter((item) => !item.closest('[hidden], [aria-hidden="true"]'));
  const first = items[0],
    last = items.at(-1);
  if (!first) {
    event.preventDefault();
    panel.value?.focus();
  } else if (
    event.shiftKey &&
    (document.activeElement === first ||
      !items.includes(document.activeElement as HTMLElement))
  ) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
</script>
<template>
  <Teleport to="body">
    <div v-if="open" class="modal" :data-theme="theme">
      <div class="modal__backdrop" @click="close"></div>
      <section
        ref="panel"
        class="modal__panel"
        role="dialog"
        aria-modal="true"
        :aria-labelledby="titleId"
        tabindex="-1"
        @keydown="keydown"
      >
        <header class="modal__head">
          <h2 :id="titleId">{{ title }}</h2>
          <button
            type="button"
            class="btn btn--link"
            aria-label="关闭弹窗"
            @click="close"
          >
            <svg class="icon" aria-hidden="true"><use href="#i-x" /></svg>
          </button>
        </header>
        <div class="modal__body"><slot /></div>
      </section>
    </div>
  </Teleport>
</template>
