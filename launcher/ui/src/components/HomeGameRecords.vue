<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, useId, watch } from "vue";
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
import { isTauri } from "../core/api";
import type { GameRecord } from "../types/launcher";

const panelId = useId();
const triggerId = useId();
const open = ref(false);
const selectedId = ref("");
const direction = ref(1);
const prefersReducedMotion = useReducedMotion();
const reduced = computed(() => form.reduceMotion || prefersReducedMotion.value);
const spring = computed(() =>
  reduced.value
    ? { duration: 0 }
    : {
        type: "spring" as const,
        stiffness: 440,
        damping: 26,
        mass: 0.8,
        opacity: { duration: 0.12 },
      },
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
  () => failure.value?.message || gameRecords.error || attendance.error,
);
const busy = computed(
  () => gameRecords.loading || attendance.busy || attendance.running,
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
  void loadAttendance();
  refresh();
  timer = setInterval(onFocus, 30_000);
  window.addEventListener("focus", onFocus);
  document.addEventListener("visibilitychange", onFocus);
  document.addEventListener("pointerdown", outside);
});
onBeforeUnmount(() => {
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
    ><div class="game-records" @keydown.esc.stop.prevent="close">
      <motion.button
        :id="triggerId"
        type="button"
        class="game-records__capsule"
        :aria-expanded="open"
        :aria-controls="panelId"
        :aria-label="open ? '收起游戏状态' : '打开游戏状态'"
        :while-hover="reduced ? {} : { y: -2 }"
        :while-press="reduced ? {} : { scale: 0.95 }"
        :transition="spring"
        @click="open = !open"
      >
        <span class="game-records__badge"
          ><svg class="icon" aria-hidden="true">
            <use href="#i-game-controller" /></svg
        ></span>
        <span>体力与周本</span>
        <span v-if="accounts.length" class="game-records__count">{{
          accounts.length
        }}</span>
        <motion.svg
          class="icon game-records__caret"
          aria-hidden="true"
          :animate="{ rotate: open ? 180 : 0 }"
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
          :aria-labelledby="panelId + '-title'"
          :aria-busy="gameRecords.loading"
          :aria-hidden="!open || undefined"
          :inert="!open"
          :initial="{
            opacity: 0,
            scale: reduced ? 1 : 0.86,
            y: reduced ? 0 : -18,
          }"
          :animate="{ opacity: 1, scale: 1, y: 0 }"
          :exit="{
            opacity: 0,
            scale: reduced ? 1 : 0.92,
            y: reduced ? 0 : -12,
          }"
          :transition="spring"
        >
          <header class="game-records__head">
            <div>
              <p class="game-records__eyebrow">
                {{ isTauri() ? "塔吉多 · 国服" : "浏览器预览 · 模拟数据" }}
              </p>
              <h2 :id="panelId + '-title'">游戏状态</h2>
            </div>
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
            :animate="{ opacity: 1, x: 0 }"
            :transition="spring"
          >
            <div class="game-records__identity">
              <h3 :title="record?.roleName || record?.roleId">
                {{ record?.roleName || record?.roleId || account.maskedPhone }}
              </h3>
              <p>
                <span>{{ account.maskedPhone }}</span
                ><span v-if="record?.serverName" :title="record.serverName">{{
                  record.serverName
                }}</span>
              </p>
            </div>
            <template v-if="record">
              <dl class="game-records__primary">
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
              </dl>
              <div class="game-records__track" aria-hidden="true">
                <i :style="{ width: percent(record) + '%' }"></i>
              </div>
              <dl class="game-records__secondary">
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
              </dl>
            </template>
            <p v-else-if="gameRecords.loading" class="game-records__hint">
              正在读取角色信息...
            </p>
            <p v-else-if="!error" class="game-records__hint">
              等待获取角色信息.
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
                <i>/ {{ String(accounts.length).padStart(2, "0") }}</i></span
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
            <span v-else-if="accounts.length" class="game-records__single"
              >01 / 01</span
            >
          </footer>
        </motion.aside></AnimatePresence
      >
    </div></MotionConfig
  >
</template>

<style scoped>
.game-records {
  position: absolute;
  z-index: 5;
  top: 28px;
  right: 28px;
  display: flex;
  justify-content: flex-end;
  width: 304px;
  color: var(--bone);
  font-family: var(--font);
}
.game-records__capsule {
  display: flex;
  align-items: center;
  gap: 10px;
  height: 44px;
  padding: 5px 14px 5px 5px;
  border: 1px solid var(--line-strong);
  border-radius: 999px;
  background: var(--surface-solid);
  color: var(--bone);
  font: 500 12px var(--font);
  cursor: pointer;
  white-space: nowrap;
}
.game-records__badge {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 50%;
  background: var(--signal-dim);
  color: var(--accent-text);
}
.game-records__badge .icon {
  width: 18px;
  height: 18px;
}
.game-records__count {
  padding-left: 10px;
  border-left: 1px solid var(--line);
  font-variant-numeric: tabular-nums;
  color: var(--faint);
}
.game-records__caret {
  width: 12px;
  height: 12px;
}
.game-records__panel {
  position: absolute;
  top: 56px;
  right: 0;
  width: 304px;
  padding: 18px;
  display: grid;
  grid-template-rows: 38px;
  grid-auto-rows: auto;
  gap: 14px;
  border: 1px solid var(--line-strong);
  border-radius: 24px;
  background: var(--surface-solid);
  transform-origin: top right;
  overflow: clip;
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.08);
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
.game-records__eyebrow {
  margin: 0 0 4px;
  font-size: 9px;
  color: var(--faint);
  letter-spacing: 0.06em;
}
h2 {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
}
.game-records__icon-button {
  display: grid;
  place-items: center;
  flex: none;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--line);
  border-radius: 50%;
  color: var(--bone);
  background: transparent;
  cursor: pointer;
}
.game-records__icon-button:hover {
  background: var(--hover);
}
.game-records__icon-button:disabled {
  opacity: 0.45;
  cursor: wait;
}
.game-records__account {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: clip;
}
h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.game-records__identity p {
  display: flex;
  gap: 10px;
  margin: 5px 0 0;
  font-size: 10px;
  color: var(--faint);
}
.game-records__identity p > span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.game-records__identity p > span:first-child {
  flex: none;
}
.game-records__primary {
  display: grid;
  grid-template-columns: 1.45fr 1fr;
  gap: 12px;
  margin: 18px 0 0;
}
dt {
  color: var(--muted);
  font-size: 11px;
}
dt small {
  margin-left: 3px;
  font-size: 9px;
  color: var(--faint);
}
dd {
  margin: 5px 0 0;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.game-records__primary dd strong {
  font-size: 32px;
  line-height: 1;
  font-weight: 500;
  letter-spacing: -0.04em;
}
.game-records__primary dd span {
  color: var(--faint);
  font-size: 12px;
}
.game-records__primary > div:last-child {
  padding-left: 18px;
  border-left: 1px solid var(--line);
}
.game-records__track {
  flex: none;
  height: 3px;
  margin: 10px 0 16px;
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
  padding-left: 18px;
}
.game-records__secondary dd {
  font-size: 13px;
}
.game-records__status {
  padding-top: 8px;
}
.game-records__error {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  margin: 0;
  color: var(--danger);
  font-size: 11px;
  line-height: 15px;
  overflow-wrap: anywhere;
}
.game-records__status:has(.game-records__link) .game-records__error {
  -webkit-line-clamp: 1;
}
.game-records__foot {
  min-height: 34px;
  justify-content: flex-end;
  border-top: 1px solid var(--line);
  padding-top: 8px;
  font-size: 10px;
  color: var(--faint);
}
.game-records__pager {
  gap: 10px;
  color: var(--bone);
  font-variant-numeric: tabular-nums;
}
.game-records__pager i {
  color: var(--faint);
  font-style: normal;
}
.game-records__previous svg {
  transform: rotate(90deg);
}
.game-records__next svg {
  transform: rotate(-90deg);
}
.game-records__single {
  font-variant-numeric: tabular-nums;
}
.game-records__hint {
  color: var(--muted);
  font-size: 12px;
  margin-top: 18px;
}
.game-records__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  min-height: 0;
  overflow: clip;
  text-align: center;
}
.game-records__empty p {
  margin: 0;
  font-size: 12px;
  color: var(--muted);
}
.game-records__empty-mark {
  display: grid;
  place-items: center;
  width: 48px;
  height: 48px;
  border-radius: 50%;
  background: var(--signal-dim);
  color: var(--accent-text);
}
.game-records__empty-mark .icon {
  width: 24px;
  height: 24px;
}
.game-records__link {
  padding: 0;
  border: 0;
  color: var(--accent-text);
  background: transparent;
  font: 500 11px var(--font);
  cursor: pointer;
}
@media (max-width: 1060px) {
  .game-records {
    top: 20px;
    right: 20px;
  }
}
</style>
