<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import {
  attendance,
  form,
  loginAttendance,
  loginAttendanceSms,
  sendAttendanceCode,
  logoutAttendance,
} from "../core/controller";
import BaseModal from "./BaseModal.vue";
import TaskCapsule from "./TaskCapsule.vue";
import type { AttendanceAccount, AttendanceReport } from "../types/launcher";
type TaskResult = AttendanceReport["steps"][number];
function skipped(step: TaskResult) {
  return step.message.startsWith("已跳过:");
}
function completed(account: AttendanceAccount) {
  const steps = account.lastReport?.steps ?? [];
  const chores = ["BBS 金币签到", "浏览帖子", "点赞帖子", "分享帖子"];
  return [
    {
      label: "社区签到",
      icon: "i-calendar-check",
      tasks: steps.filter((step) => step.label === "社区签到"),
      minimum: 1,
    },
    {
      label: "异环签到",
      icon: "i-game-controller",
      tasks: steps.filter((step) => step.label.startsWith("角色 ")),
      minimum: 1,
    },
    {
      label: "社区杂活",
      icon: "i-newspaper-clipping",
      tasks: steps.filter((step) => chores.includes(step.label)),
      minimum: chores.length,
    },
    {
      label: "云异环签到",
      icon: "i-cloud-check",
      tasks: steps.filter((step) => step.label === "云异环时长"),
      minimum: 1,
    },
  ]
    .filter(
      (group) =>
        group.tasks.length >= group.minimum &&
        group.tasks.every((step) => step.ok && !skipped(step)),
    )
    .map((group) => ({
      label: group.label,
      icon: group.icon,
      description: [
        ...group.tasks,
        ...(group.label === "社区杂活"
          ? steps.filter((step) => step.label === "今日金币" && step.ok)
          : []),
      ]
        .map((step) => `${step.label}: ${step.message}`)
        .join("\n"),
    }));
}
function taskNotes(account: AttendanceAccount) {
  return (
    account.lastReport?.steps.filter((step) => !step.ok || skipped(step)) ?? []
  );
}
const props = defineProps<{ open: boolean; startMode: "list" | "add" }>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();
const mode = ref<"list" | "add">("list");
const phone = ref("");
const password = ref("");
const remember = ref(false);
const loginMode = ref<"password" | "sms">("password");
const code = ref("");
const deviceId = ref("");
const sentPhone = ref("");
const sending = ref(false);
const cooldown = ref(0);
let timer: ReturnType<typeof setInterval> | undefined;
function resetLogin() {
  password.value = "";
  code.value = "";
  deviceId.value = "";
  sentPhone.value = "";
  error.value = "";
  message.value = "";
  clearInterval(timer);
  timer = undefined;
  cooldown.value = 0;
}
function switchLogin(value: "password" | "sms") {
  session++;
  resetLogin();
  loginMode.value = value;
}
async function sendCode() {
  if (sending.value || attendance.busy || cooldown.value) return;
  const current = session;
  const requestedPhone = phone.value.trim();
  if (!/^[0-9]{11}$/.test(requestedPhone)) {
    error.value = "请输入 11 位手机号码.";
    return;
  }
  sending.value = true;
  error.value = "";
  message.value = "";
  try {
    const device = await sendAttendanceCode(requestedPhone);
    if (
      current !== session ||
      !props.open ||
      requestedPhone !== phone.value.trim()
    )
      return;
    deviceId.value = device;
    sentPhone.value = requestedPhone;
    message.value = "验证码已发送.";
    cooldown.value = 60;
    timer = setInterval(() => {
      if (--cooldown.value <= 0) {
        clearInterval(timer);
        timer = undefined;
      }
    }, 1000);
  } catch (cause) {
    if (current === session) error.value = errorMessage(cause);
  } finally {
    code.value = "";
    sending.value = false;
  }
}
function errorMessage(cause: unknown) {
  return cause && typeof cause === "object" && "message" in cause
    ? String(cause.message)
    : String(cause);
}
const error = ref("");
const message = ref("");
const confirming = ref("");
const relogin = ref(false);
const accounts = computed(() => attendance.status?.accounts ?? []);
let session = 0;
function clear() {
  resetLogin();
  loginMode.value = "password";
  phone.value = "";
  password.value = "";
  remember.value = false;
  error.value = "";
  confirming.value = "";
}
watch(
  () => props.open,
  (open) => {
    session++;
    clear();
    message.value = "";
    relogin.value = false;
    if (open) mode.value = props.startMode;
  },
  { immediate: true },
);
watch(
  phone,
  () => {
    deviceId.value = "";
    sentPhone.value = "";
    code.value = "";
    message.value = "";
    error.value = "";
    clearInterval(timer);
    timer = undefined;
    cooldown.value = 0;
  },
  { flush: "sync" },
);
watch(mode, async (value) => {
  if (!props.open || value !== "add") return;
  await nextTick();
  document.getElementById("attendance-phone")?.focus();
});
onBeforeUnmount(() => {
  session++;
  clear();
});
function add(reloginAccount = false) {
  clear();
  message.value = "";
  relogin.value = reloginAccount;
  mode.value = "add";
}
async function login() {
  const current = session;
  error.value = "";
  message.value = "";
  try {
    if (loginMode.value === "sms") {
      if (!deviceId.value || sentPhone.value !== phone.value.trim())
        throw new Error("请先为当前手机号获取验证码.");
      if (!/^[0-9]{4,8}$/.test(code.value.trim()))
        throw new Error("请输入 6 位短信验证码.");
      await loginAttendanceSms(phone.value, code.value, deviceId.value);
    } else {
      await loginAttendance(phone.value, password.value, remember.value);
    }
    if (current !== session || !props.open) return;
    clear();
    message.value = "账号已添加.";
    mode.value = accounts.value.length ? "list" : "add";
  } catch (cause) {
    if (current === session)
      error.value =
        cause && typeof cause === "object" && "message" in cause
          ? String(cause.message)
          : String(cause);
  } finally {
    code.value = "";
    password.value = "";
  }
}
async function remove(id: string) {
  error.value = "";
  try {
    await logoutAttendance(id);
    confirming.value = "";
  } catch (cause) {
    error.value =
      cause && typeof cause === "object" && "message" in cause
        ? String(cause.message)
        : String(cause);
  }
}
</script>
<template>
  <BaseModal
    :open="open"
    :theme="form.theme"
    :reduce-motion="form.reduceMotion"
    :title="mode === 'list' ? '塔吉多账号' : '添加塔吉多账号'"
    @update:open="emit('update:open', $event)"
  >
    <template v-if="mode === 'list'">
      <article
        v-for="account in accounts"
        :key="account.id"
        class="account-row"
      >
        <strong>{{ account.maskedPhone }}</strong>
        <div
          :class="{
            attendance__error: !!account.attention || account.failedToday,
            'account-row__success': account.signedToday && !account.attention,
          }"
        >
          <template v-if="account.attention"
            ><span>需要重新登录</span>
            <p>{{ account.attention.message }}</p></template
          >
          <template v-else-if="account.signedToday"
            ><span>今日已签到</span
            ><time v-if="account.lastReport">{{
              new Date(account.lastReport.at * 1000).toLocaleTimeString(
                "zh-CN",
                { hour: "2-digit", minute: "2-digit" },
              )
            }}</time></template
          >
          <template v-else-if="account.failedToday"
            ><span>签到失败</span>
            <p>
              {{
                account.lastReport?.steps.find(
                  (step) => !step.optional && !step.ok,
                )?.message
              }}
            </p></template
          >
          <span v-else>今日未签到</span>
        </div>
        <ul
          v-if="completed(account).length"
          class="account-row__tasks"
          aria-label="已完成任务"
        >
          <li v-for="step in completed(account)" :key="step.label">
            <TaskCapsule
              :label="step.label"
              :icon="step.icon"
              :description="step.description"
              :theme="form.theme"
            />
          </li>
        </ul>
        <div v-if="taskNotes(account).length" class="account-row__notes">
          <p
            v-for="(step, index) in taskNotes(account)"
            :key="index"
            :class="{ attendance__error: !step.ok }"
          >
            {{ step.label }}: {{ step.message }}
          </p>
        </div>
        <div class="attendance__actions">
          <template v-if="confirming === account.id"
            ><span class="account-row__danger">确认移除此账号?</span
            ><button
              type="button"
              class="btn btn--link account-row__danger"
              :disabled="attendance.busy"
              @click="remove(account.id)"
            >
              确认移除</button
            ><button
              type="button"
              class="btn btn--link"
              :disabled="attendance.busy"
              @click="confirming = ''"
            >
              取消
            </button></template
          >
          <template v-else
            ><button
              type="button"
              class="btn btn--link"
              :disabled="attendance.busy"
              @click="add(true)"
            >
              重新登录</button
            ><button
              type="button"
              class="btn btn--link account-row__danger"
              :disabled="attendance.busy"
              @click="confirming = account.id"
            >
              <svg class="icon" aria-hidden="true"><use href="#i-trash" /></svg
              >移除
            </button></template
          >
        </div>
      </article>
      <p v-if="!accounts.length" class="attendance__hint">尚未登录</p>
      <footer class="modal__foot">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="attendance.busy"
          @click="add()"
        >
          <svg class="icon" aria-hidden="true"><use href="#i-plus" /></svg
          >添加账号
        </button>
      </footer>
    </template>
    <form v-else @submit.prevent="login">
      <div class="login-tabs" role="tablist" aria-label="登录方式">
        <button
          v-for="tab in ['password', 'sms'] as const"
          :key="tab"
          type="button"
          role="tab"
          class="login-tabs__tab"
          :class="{ 'is-active': loginMode === tab }"
          :aria-selected="loginMode === tab"
          :disabled="attendance.busy || sending"
          @click="switchLogin(tab)"
        >
          {{ tab === "password" ? "密码登录" : "短信验证码" }}
        </button>
      </div>
      <p v-if="relogin" class="attendance__hint">
        请输入需要重新登录的账号手机号和登录凭据
      </p>
      <div class="field-grid">
        <label class="field" :class="{ 'field--wide': loginMode === 'sms' }"
          ><span>手机号码</span
          ><input
            id="attendance-phone"
            v-model="phone"
            type="tel"
            inputmode="numeric"
            autocomplete="username"
            maxlength="11"
            pattern="[0-9]{11}"
            required
            :disabled="attendance.busy"
        /></label>
        <label v-if="loginMode === 'password'" class="field"
          ><span>密码</span
          ><input
            id="attendance-password"
            v-model="password"
            type="password"
            autocomplete="current-password"
            required
            :disabled="attendance.busy"
        /></label>
        <div v-else class="field field--wide">
          <span>短信验证码</span>
          <div class="attendance__code-row">
            <input
              id="attendance-code"
              v-model="code"
              aria-label="短信验证码"
              inputmode="numeric"
              autocomplete="one-time-code"
              maxlength="8"
              required
              :disabled="attendance.busy"
            />
            <button
              type="button"
              class="btn btn--ghost"
              :disabled="attendance.busy || sending || cooldown > 0"
              @click="sendCode"
            >
              {{
                sending
                  ? "正在发送"
                  : cooldown
                    ? `重新发送 (${cooldown}s)`
                    : "发送验证码"
              }}
            </button>
          </div>
        </div>
      </div>
      <label v-if="loginMode === 'password'" class="attendance__remember"
        ><input
          v-model="remember"
          type="checkbox"
          :disabled="attendance.busy"
        />保存密码, 登录失效时自动恢复</label
      >
      <footer class="modal__foot">
        <button
          id="attendance-login"
          class="btn btn--primary"
          type="submit"
          :disabled="attendance.busy || sending"
        >
          <svg class="icon" aria-hidden="true"><use href="#i-sign-in" /></svg
          >{{ attendance.busy ? "正在登录" : "登录" }}</button
        ><button
          class="btn btn--link"
          type="button"
          :disabled="attendance.busy"
          @click="
            clear();
            mode = 'list';
          "
        >
          返回账号列表
        </button>
      </footer>
    </form>
    <p v-if="error" class="attendance__error" role="alert">{{ error }}</p>
    <p v-if="message" role="status">{{ message }}</p>
  </BaseModal>
</template>
