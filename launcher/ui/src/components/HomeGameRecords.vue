<script setup lang="ts">
import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  useId,
  useTemplateRef,
  watch,
} from "vue";
import {
  AnimatePresence,
  MotionConfig,
  motion,
  useReducedMotion,
} from "motion-v";
import {
  attendance,
  form,
  gameRecords,
  loadAttendance,
  refreshGameRecords,
  showPage,
  state,
} from "../core/controller";
import { usePlayerMotion } from "../composables/usePlayerMotion";
import { playerExpanded } from "../composables/useSceneMotion";
import type { GameRecord } from "../types/launcher";

const panelId = useId();
const triggerId = useId();
const open = playerExpanded;
const selectedId = ref("");
const direction = ref(1);
const prefersReducedMotion = useReducedMotion();
const reduced = computed(() => form.reduceMotion || prefersReducedMotion.value);
const spring = computed(() =>
  reduced.value
    ? { duration: 0 }
    : {
        type: "spring" as const,
        stiffness: 380,
        damping: 28,
        mass: 0.9,
        opacity: { duration: 0.12 },
      },
);
const surface = useTemplateRef<HTMLElement>("surface");
const details = useTemplateRef<HTMLElement>("details");
const { detailsReady, summaryReady, moving } = usePlayerMotion(
  open,
  reduced,
  surface,
  details,
);
const accounts = computed(() => attendance.status?.accounts ?? []);
const account = computed(
  () =>
    accounts.value.find((item) => item.id === selectedId.value) ??
    accounts.value[0],
);
const index = computed(() =>
  Math.max(
    0,
    accounts.value.findIndex((item) => item.id === account.value?.id),
  ),
);
const record = computed(() => account.value?.gameRecord);
const failure = computed(() => account.value?.gameRecordError);
const error = computed(
  () =>
    failure.value?.message ||
    gameRecords.error ||
    (!accounts.value.length ? attendance.error : ""),
);
const busy = computed(
  () => gameRecords.loading || (attendance.busy && !attendance.running),
);
const needsLogin = computed(() =>
  ["auth", "verification"].includes(failure.value?.kind ?? ""),
);
function select(offset: number) {
  if (!accounts.value.length) return;
  direction.value = offset;
  selectedId.value =
    accounts.value[
      (index.value + offset + accounts.value.length) % accounts.value.length
    ].id;
}
function close() {
  open.value = false;
  document.getElementById(triggerId)?.focus();
}
function settings() {
  close();
  showPage("settings");
}
function outside(event: PointerEvent) {
  const root = document.getElementById(triggerId)?.parentElement;
  if (
    open.value &&
    event.target instanceof Node &&
    !root?.contains(event.target)
  )
    open.value = false;
}
function refresh(force = false) {
  if (state.page === "home" && !document.hidden) void refreshGameRecords(force);
}
watch(
  () => [accounts.value.map((a) => a.id).join(","), state.page, busy.value],
  () => refresh(),
);
let timer: ReturnType<typeof setInterval>;
const onFocus = () => refresh();
onMounted(() => {
  open.value = false;
  void loadAttendance();
  refresh();
  timer = setInterval(onFocus, 30_000);
  window.addEventListener("focus", onFocus);
  document.addEventListener("visibilitychange", onFocus);
  document.addEventListener("pointerdown", outside);
});
onBeforeUnmount(() => {
  open.value = false;
  clearInterval(timer);
  window.removeEventListener("focus", onFocus);
  document.removeEventListener("visibilitychange", onFocus);
  document.removeEventListener("pointerdown", outside);
});
function value(number: number | null | undefined) {
  return number ?? "--";
}
function meter(
  current: number | null | undefined,
  max: number | null | undefined,
) {
  return `${value(current)} / ${value(max)}`;
}
function percent(data: GameRecord) {
  if (data.stamina == null || !data.staminaMax) return 0;
  return Math.max(0, Math.min(100, (data.stamina / data.staminaMax) * 100));
}
</script>

<template>
  <MotionConfig :reduced-motion="form.reduceMotion ? 'always' : 'user'"
    ><div
      class="game-records"
      :data-moving="moving"
      :class="{ 'game-records--dark': form.theme === 'dark' }"
      @keydown.esc.stop.prevent="close"
    >
      <div ref="surface" class="game-records__surface">
        <motion.button
          :id="triggerId"
          type="button"
          class="game-records__capsule"
          :aria-expanded="open"
          :aria-controls="panelId"
          :aria-label="open ? '收起游戏状态' : '打开游戏状态'"
          :while-hover="
            reduced ? {} : { backgroundColor: 'rgba(128, 128, 128, 0.08)' }
          "
          :while-press="reduced ? {} : { scale: 0.95 }"
          :transition="spring"
          @click="open = !open"
        >
          <span class="game-records__badge"
            ><svg class="icon" aria-hidden="true">
              <use href="#i-users" /></svg
          ></span>
          <motion.span
            class="game-records__summary"
            :animate="{
              opacity: summaryReady ? 1 : 0,
              x: summaryReady ? 0 : -6,
            }"
            :transition="{ duration: reduced ? 0 : 0.12 }"
          >
            <span class="game-records__name">{{
              record?.roleName ||
              record?.roleId ||
              (account ? account.maskedPhone : "玩家信息")
            }}</span>
            <span class="game-records__stamina">{{
              record
                ? `体力 ${meter(record.stamina, record.staminaMax)}`
                : account
                  ? "查看游戏状态"
                  : "登录塔吉多"
            }}</span>
          </motion.span>
          <motion.svg
            class="icon game-records__caret"
            aria-hidden="true"
            :animate="{ rotate: open ? 180 : 0, opacity: summaryReady ? 1 : 0 }"
            :transition="spring"
          >
            <use href="#i-caret-down" />
          </motion.svg>
        </motion.button>

        <AnimatePresence
          ><motion.aside
            v-if="open"
            :id="panelId"
            key="records"
            class="game-records__panel"
            :aria-labelledby="triggerId"
            :aria-busy="gameRecords.loading"
            :aria-hidden="!open || undefined"
            :inert="!open"
            :initial="{ opacity: 0, y: 8 }"
            :animate="{
              opacity: detailsReady ? 1 : 0,
              y: detailsReady ? 0 : 8,
            }"
            :exit="{ opacity: 0 }"
            :transition="{ duration: reduced ? 0 : 0.14 }"
            ><div ref="details" class="game-records__details">
              <header v-if="account" class="game-records__head">
                <p class="game-records__identity">
                  <span>{{ account.maskedPhone }}</span>
                  <span v-if="record?.serverName">{{ record.serverName }}</span>
                </p>
                <div class="game-records__actions">
                  <motion.button
                    v-if="accounts.length"
                    class="game-records__icon-button"
                    :while-press="reduced ? {} : { scale: 0.88 }"
                    :transition="spring"
                    type="button"
                    aria-label="刷新游戏状态"
                    :disabled="busy"
                    @click="refresh(true)"
                  >
                    <svg class="icon" aria-hidden="true">
                      <use href="#i-arrows-clockwise" />
                    </svg>
                  </motion.button>
                </div>
              </header>
              <motion.article
                v-if="account"
                :key="account.id"
                class="game-records__account"
                aria-live="polite"
                :initial="{
                  opacity: reduced ? 1 : 0,
                  x: reduced ? 0 : direction * 16,
                }"
                :animate="{ opacity: detailsReady ? 1 : 0, x: 0 }"
                :transition="spring"
              >
                <template v-if="record">
                  <motion.dl
                    class="game-records__primary"
                    :initial="{ opacity: 0, y: 6 }"
                    :animate="{
                      opacity: detailsReady ? 1 : 0,
                      y: detailsReady ? 0 : 6,
                    }"
                    :transition="{ ...spring, delay: reduced ? 0 : 0.045 }"
                  >
                    <div>
                      <dt>体力 <small>本性像素</small></dt>
                      <dd>
                        <strong>{{ value(record.stamina) }}</strong
                        ><span> / {{ value(record.staminaMax) }}</span>
                      </dd>
                    </div>
                    <div>
                      <dt>周本剩余</dt>
                      <dd>
                        <strong>{{ value(record.weeklyRemaining) }}</strong
                        ><span> 次</span>
                      </dd>
                    </div>
                  </motion.dl>
                  <div class="game-records__track" aria-hidden="true">
                    <i :style="{ width: percent(record) + '%' }"></i>
                  </div>
                  <motion.dl
                    class="game-records__secondary"
                    :initial="{ opacity: 0, y: 6 }"
                    :animate="{
                      opacity: detailsReady ? 1 : 0,
                      y: detailsReady ? 0 : 6,
                    }"
                    :transition="{ ...spring, delay: reduced ? 0 : 0.09 }"
                  >
                    <div>
                      <dt>都市活力</dt>
                      <dd>
                        {{ meter(record.cityStamina, record.cityStaminaMax) }}
                      </dd>
                    </div>
                    <div>
                      <dt>今日活跃度</dt>
                      <dd>{{ value(record.activity) }}</dd>
                    </div>
                  </motion.dl>
                </template>
                <p v-else-if="gameRecords.loading" class="game-records__hint">
                  正在读取角色信息...
                </p>
                <p v-else-if="!error" class="game-records__hint">
                  等待获取角色信息.
                </p>
                <p
                  v-if="gameRecords.waiting"
                  class="game-records__hint"
                  role="status"
                >
                  正在等待账号任务完成, 稍后自动刷新.
                </p>
                <div v-if="error || needsLogin" class="game-records__status">
                  <p
                    v-if="error"
                    class="game-records__error"
                    role="status"
                    :title="error + (record ? '. 当前显示上次数据.' : '')"
                  >
                    {{ error }}{{ record ? ". 当前显示上次数据." : "" }}
                  </p>
                  <motion.button
                    v-if="needsLogin"
                    type="button"
                    class="game-records__link"
                    @click="settings"
                  >
                    前往设置重新登录 <span aria-hidden="true">↗</span>
                  </motion.button>
                </div>
              </motion.article>
              <div v-else class="game-records__empty">
                <span class="game-records__empty-mark" aria-hidden="true"
                  ><svg class="icon"><use href="#i-users" /></svg
                ></span>
                <p
                  v-if="error"
                  class="game-records__error"
                  role="status"
                  :title="error"
                >
                  {{ error }}
                </p>
                <p v-else>
                  {{
                    !attendance.loaded && attendance.loading
                      ? "正在读取账号..."
                      : "登录塔吉多, 查看体力与周本信息."
                  }}
                </p>
                <motion.button
                  type="button"
                  class="game-records__link"
                  @click="settings"
                >
                  前往设置登录 <span aria-hidden="true">↗</span>
                </motion.button>
              </div>

              <footer v-if="accounts.length" class="game-records__foot">
                <nav
                  v-if="accounts.length > 1"
                  class="game-records__pager"
                  aria-label="切换账号"
                  @keydown.left.prevent="select(-1)"
                  @keydown.right.prevent="select(1)"
                >
                  <motion.button
                    class="game-records__icon-button game-records__previous"
                    type="button"
                    aria-label="上一个账号"
                    @click="select(-1)"
                  >
                    <svg class="icon" aria-hidden="true">
                      <use href="#i-caret-down" />
                    </svg>
                  </motion.button>
                  <span aria-live="polite" aria-atomic="true"
                    >{{ String(index + 1).padStart(2, "0") }}
                    <i
                      >/ {{ String(accounts.length).padStart(2, "0") }}</i
                    ></span
                  >
                  <motion.button
                    class="game-records__icon-button game-records__next"
                    type="button"
                    aria-label="下一个账号"
                    @click="select(1)"
                  >
                    <svg class="icon" aria-hidden="true">
                      <use href="#i-caret-down" />
                    </svg>
                  </motion.button>
                </nav>
              </footer></div></motion.aside
        ></AnimatePresence>
      </div></div
  ></MotionConfig>
</template>

<style scoped>
.game-records {
  position: absolute;
  top: var(--scene-inset);
  right: var(--scene-inset);
  z-index: 5;
  width: 280px;
  color: var(--bone);
  font-family: var(--font);
}
.game-records__surface {
  position: relative;
  width: 204px;
  height: 44px;
  margin-left: auto;
  overflow: clip;
  border: 1px solid rgb(255 255 255 / 32%);
  border-radius: 14px;
  background: rgb(247 249 251 / 96%);
  color: #202832;
  box-shadow: 0 6px 24px rgb(0 0 0 / 12%);
  transform-origin: center;
  --muted: #56616e;
  --faint: #65717d;
  --line: rgb(30 45 60 / 10%);
  --accent-text: #365c80;
  --signal-strong: #5f7c95;
}
.game-records--dark .game-records__surface {
  background: rgb(23 29 38 / 96%);
  color: #f0f3f7;
  border-color: rgb(255 255 255 / 14%);
  --muted: #b3bdca;
  --faint: #9aa8b7;
  --line: rgb(225 235 245 / 12%);
  --accent-text: #c0d5ea;
  --signal-strong: #a7bed3;
}
.game-records__capsule {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  height: 42px;
  padding: 0 12px 0 7px;
  color: inherit;
  text-align: left;
  cursor: pointer;
}
.game-records__badge {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  flex: none;
  color: inherit;
}
.game-records__badge .icon {
  width: 18px;
  height: 18px;
}
.game-records__summary {
  display: flex;
  flex: 1;
  min-width: 0;
  flex-direction: column;
  gap: 1px;
}
.game-records__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  line-height: 15px;
  font-weight: 600;
}
.game-records__stamina {
  color: var(--muted);
  font-size: 10px;
  line-height: 14px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.game-records__caret {
  width: 10px;
  height: 10px;
  flex: none;
}
.game-records__panel {
  position: absolute;
  top: 42px;
  right: 0;
  width: 278px;
  overflow: clip;
}
.game-records__details {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px 16px 16px;
}
.game-records__head,
.game-records__actions,
.game-records__foot,
.game-records__pager {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.game-records__head {
  border-top: 1px solid var(--line);
  padding-top: 12px;
}
.game-records__icon-button {
  display: grid;
  place-items: center;
  flex: none;
  width: 26px;
  height: 26px;
  padding: 0;
  border: 1px solid var(--line);
  border-radius: 8px;
  color: inherit;
  background: transparent;
  cursor: pointer;
}
.game-records__icon-button .icon {
  width: 13px;
  height: 13px;
}
.game-records__icon-button:hover {
  background: var(--hover);
}
.game-records__icon-button:disabled {
  opacity: 0.45;
}
.game-records__account {
  display: flex;
  flex-direction: column;
  min-width: 0;
  overflow: clip;
}
.game-records__identity {
  display: flex;
  gap: 10px;
  margin: 0;
  color: var(--faint);
  font-size: 10px;
}
.game-records__identity span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.game-records__primary {
  display: grid;
  grid-template-columns: 1.45fr 1fr;
  gap: 12px;
  margin: 18px 0 0;
}
dt {
  color: var(--muted);
  font-size: 10px;
}
dt small {
  margin-left: 3px;
  color: var(--faint);
  font-size: 9px;
}
dd {
  margin: 5px 0 0;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.game-records__primary dd strong {
  font-size: 30px;
  line-height: 1;
  font-weight: 500;
  letter-spacing: -0.05em;
}
.game-records__primary dd span {
  color: var(--faint);
  font-size: 11px;
}
.game-records__primary > div:last-child {
  padding-left: 16px;
  border-left: 1px solid var(--line);
}
.game-records__track {
  height: 2px;
  margin: 12px 0 16px;
  background: var(--line);
  overflow: clip;
}
.game-records__track i {
  display: block;
  height: 100%;
  background: var(--signal-strong);
}
.game-records__secondary {
  display: grid;
  grid-template-columns: 1.45fr 1fr;
  gap: 12px;
  margin: 0;
}
.game-records__secondary > div:last-child {
  padding-left: 16px;
}
.game-records__secondary dd {
  font-size: 12px;
}
.game-records__status {
  padding-top: 10px;
}
.game-records__error {
  margin: 0;
  color: var(--danger);
  font-size: 11px;
  line-height: 16px;
  overflow-wrap: anywhere;
}
.game-records__foot {
  justify-content: flex-end;
  border-top: 1px solid var(--line);
  padding-top: 10px;
  min-height: 0;
  font-size: 10px;
  color: var(--faint);
}
.game-records__foot:empty {
  display: none;
}
.game-records__pager {
  gap: 10px;
  font-variant-numeric: tabular-nums;
}
.game-records__pager i {
  font-style: normal;
  color: var(--faint);
}
.game-records__previous svg {
  transform: rotate(90deg);
}
.game-records__next svg {
  transform: rotate(-90deg);
}
.game-records__hint {
  margin: 14px 0 0;
  font-size: 11px;
  color: var(--muted);
}
.game-records__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 10px 0 4px;
  text-align: center;
}
.game-records__empty p {
  margin: 0;
  color: var(--muted);
  font-size: 11px;
}
.game-records__empty-mark {
  display: none;
}
.game-records__link {
  padding: 0;
  border: 0;
  color: var(--accent-text);
  background: transparent;
  font: 500 11px var(--font);
  cursor: pointer;
}
</style>
