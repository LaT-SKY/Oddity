<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
} from "vue";
import { motion, useReducedMotion } from "motion-v";
import NotificationActions from "./NotificationActions.vue";
import { useNotificationArrival } from "../composables/useNotificationArrival";
import { form } from "../core/controller";
import {
  notifications,
  dismissNotification,
  pauseNotifications,
  clearNotifications,
} from "../core/notifications";

const expanded = ref(false);
const frontHeight = ref(96);
const root = ref<HTMLElement | null>(null);
const prefersReduced = useReducedMotion();
const reduced = computed(() => form.reduceMotion || prefersReduced.value);
const { displayed, landed, incoming, revealed, retiring, phase, settled } =
  useNotificationArrival(root, reduced);
const visible = computed(() =>
  displayed.value.filter(
    (item) =>
      item.id === incoming.value ||
      retiring.value.includes(item.id) ||
      (expanded.value ? settled.value : settled.value.slice(0, 3)).some(
        (candidate) => candidate.id === item.id,
      ),
  ),
);
const depth = (id: number) =>
  Math.max(
    0,
    settled.value.findIndex((item) => item.id === id),
  );
const contentReady = (id: number) =>
  !retiring.value.includes(id) &&
  (id === incoming.value ? revealed.value === id : landed.value.includes(id));
const interaction = computed(() =>
  reduced.value
    ? { duration: 0 }
    : { type: "spring" as const, stiffness: 440, damping: 32 },
);
let observer: ResizeObserver | undefined;
async function measure() {
  await nextTick();
  pauseNotifications("focus", !!root.value?.contains(document.activeElement));
  if (!notifications.length) pauseNotifications("hover", false);
  observer?.disconnect();
  const card = root.value?.querySelector<HTMLElement>(
    '[data-front="true"]:not(.is-retiring) .notification-surface',
  );
  if (card) {
    frontHeight.value = card.offsetHeight;
    observer?.observe(card);
  }
}
watch(
  () => notifications.map((item) => `${item.id}:${item.revision}`).join(","),
  () => {
    if (notifications.length < 2) expanded.value = false;
    void measure();
  },
);
watch([landed, retiring], () => {
  void measure();
});
watch(expanded, async (value) => {
  pauseNotifications("expanded", value);
  await nextTick();
  pauseNotifications("focus", !!root.value?.contains(document.activeElement));
});
function onFocusOut(event: FocusEvent) {
  if (!root.value?.contains(event.relatedTarget as Node | null))
    pauseNotifications("focus", false);
}
function visibility() {
  pauseNotifications("hidden", document.hidden);
}
function outside(event: PointerEvent) {
  if (
    expanded.value &&
    event.target instanceof Node &&
    !root.value?.contains(event.target)
  )
    expanded.value = false;
}
onMounted(() => {
  observer = new ResizeObserver((entries) => {
    frontHeight.value = (entries[0].target as HTMLElement).offsetHeight;
  });
  void measure();
  visibility();
  document.addEventListener("visibilitychange", visibility);
  document.addEventListener("pointerdown", outside);
});
onBeforeUnmount(() => {
  observer?.disconnect();
  document.removeEventListener("visibilitychange", visibility);
  document.removeEventListener("pointerdown", outside);
  for (const reason of ["expanded", "hover", "focus", "hidden"])
    pauseNotifications(reason, false);
});
</script>

<template>
  <section
    v-if="visible.length"
    ref="root"
    class="notification-stack"
    :class="{ 'is-expanded': expanded, 'is-animating': incoming !== null }"
    :data-phase="phase"
    :data-departing="retiring.length"
    aria-label="通知"
    :data-theme="form.theme"
    @pointerenter="pauseNotifications('hover', true)"
    @pointerleave="pauseNotifications('hover', false)"
    @focusin="pauseNotifications('focus', true)"
    @focusout="onFocusOut"
    @keydown.esc.stop.prevent="expanded = false"
  >
    <motion.div
      v-if="expanded"
      class="notification-toolbar"
      :initial="{ opacity: 0 }"
      :animate="{ opacity: 1 }"
    >
      <span>{{ notifications.length }} 条通知</span>
      <div>
        <button
          class="notification-ghost"
          @click="expanded = false"
          type="button"
        >
          收起
        </button>
        <button
          class="notification-ghost"
          @click="clearNotifications"
          type="button"
        >
          全部清除
        </button>
      </div>
    </motion.div>
    <div
      class="notification-list"
      :style="{
        paddingBottom: expanded
          ? '0px'
          : `${Math.max(0, Math.min(settled.length, 3) - 1) * 10}px`,
      }"
    >
      <motion.div
        v-for="item in visible"
        :key="item.id"
        class="notification-slot"
        layout="position"
        :data-notification-id="item.id"
        :data-front="landed.includes(item.id) && depth(item.id) === 0"
        :class="{
          'is-arriving': incoming === item.id,
          'is-retiring': retiring.includes(item.id),
        }"
        :style="{
          position:
            landed.includes(item.id) && (expanded || depth(item.id) === 0)
              ? 'relative'
              : 'absolute',
          zIndex:
            incoming === item.id || retiring.includes(item.id)
              ? displayed.length + 1
              : notifications.length - depth(item.id),
          pointerEvents:
            contentReady(item.id) && (expanded || depth(item.id) === 0)
              ? 'auto'
              : 'none',
        }"
        :aria-hidden="
          !contentReady(item.id) ||
          (!expanded && depth(item.id) > 0) ||
          undefined
        "
        :inert="!contentReady(item.id) || (!expanded && depth(item.id) > 0)"
        :initial="false"
        :animate="{ opacity: 1, y: 0, scale: 1 }"
        :transition="interaction"
      >
        <motion.div
          class="notification-depth"
          :animate="{
            y: expanded ? 0 : depth(item.id) * 10,
            scale: expanded ? 1 : 1 - depth(item.id) * 0.035,
            opacity: expanded ? 1 : 1 - depth(item.id) * 0.16,
          }"
          :style="{
            height:
              !expanded && depth(item.id) > 0 ? `${frontHeight}px` : undefined,
          }"
          :transition="interaction"
        >
          <div class="notification-flight">
            <div class="notification-surface">
              <motion.article
                :initial="false"
                :animate="{
                  opacity: contentReady(item.id) ? 1 : 0,
                  y: contentReady(item.id) ? 0 : 6,
                }"
                :transition="{ duration: reduced ? 0 : 0.12 }"
                class="notification-card"
                :data-kind="item.kind"
                :aria-busy="item.busy"
              >
                <div class="notification-heading">
                  <svg class="icon notification-symbol" aria-hidden="true">
                    <use
                      :href="
                        item.kind === 'error'
                          ? '#i-warning-circle'
                          : item.kind === 'ok'
                            ? '#i-check-circle'
                            : '#i-info'
                      "
                    />
                  </svg>
                  <div class="notification-copy">
                    <strong v-if="item.title" class="notification-title">{{
                      item.title
                    }}</strong>
                    <p
                      class="notification-message"
                      role="status"
                      aria-live="polite"
                      aria-atomic="true"
                    >
                      {{ item.message }}
                    </p>
                  </div>
                  <span
                    v-if="item.count > 1"
                    class="notification-repeat"
                    :aria-label="`重复 ${item.count} 次`"
                    >×{{ item.count }}</span
                  >
                  <motion.button
                    v-if="
                      !expanded &&
                      depth(item.id) === 0 &&
                      notifications.length > 1
                    "
                    class="notification-icon-button"
                    :disabled="incoming !== null && phase < 3"
                    type="button"
                    :aria-label="`展开 ${notifications.length} 条通知`"
                    :while-press="{ scale: 0.88 }"
                    @click="expanded = true"
                  >
                    <span class="notification-count">{{
                      notifications.length
                    }}</span>
                    <svg class="icon" aria-hidden="true">
                      <use href="#i-caret-down" />
                    </svg>
                  </motion.button>
                  <motion.button
                    class="notification-icon-button notification-dismiss"
                    type="button"
                    aria-label="关闭通知"
                    :while-press="{ scale: 0.88 }"
                    @click="dismissNotification(item.id)"
                  >
                    <svg class="icon" aria-hidden="true">
                      <use href="#i-x" />
                    </svg>
                  </motion.button>
                </div>
                <p
                  v-if="item.actionError"
                  class="notification-action-error"
                  role="alert"
                >
                  {{ item.actionError }}
                </p>
                <NotificationActions :item="item" :reduced="reduced" />
              </motion.article>
            </div>
          </div>
        </motion.div>
      </motion.div>
    </div>
  </section>
</template>
