<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { isTauri } from "../core/api";
import {
  attendance,
  attendanceForm,
  loadAttendance,
  runAttendance,
} from "../core/controller";
import BaseSelect from "./BaseSelect.vue";
import TaygedoAccountDialog from "./TaygedoAccountDialog.vue";
const open = ref(false);
const startMode = ref<"list" | "add">("list");
const accounts = computed(() => attendance.status?.accounts ?? []);
function manage(mode: "list" | "add") {
  startMode.value = mode;
  open.value = true;
}
const primary = computed(() => {
  if (!attendance.loaded && attendance.loading)
    return { label: "正在读取", icon: "i-calendar-check", disabled: true };
  if (!accounts.value.length)
    return { label: "立即登录", icon: "i-sign-in", mode: "add" as const };
  if (attendance.running)
    return { label: "正在签到", icon: "i-calendar-check", disabled: true };
  if (accounts.value.every((account) => account.signedToday))
    return { label: "今日已签到", icon: "i-check-circle", disabled: true };
  if (
    accounts.value
      .filter((account) => !account.signedToday)
      .every((account) =>
        ["auth", "verification"].includes(account.attention?.kind ?? ""),
      )
  )
    return { label: "重新登录", icon: "i-sign-in", mode: "list" as const };
  return { label: "立即签到", icon: "i-calendar-check" };
});
const failure = computed(() => {
  const unsigned = accounts.value.filter((account) => !account.signedToday);
  const attention = unsigned.filter((account) => account.attention).length;
  if (attention) return `${attention} 个账号需要重新登录.`;
  const failed = unsigned.filter((account) => account.failedToday).length;
  return failed ? `${failed} 个账号签到失败.` : "";
});
const options = [
  { label: "启动器打开时", value: "launch" },
  { label: "开机时", value: "boot" },
  { label: "每天定时", value: "daily" },
  { label: "仅手动", value: "manual" },
];
let poll: ReturnType<typeof setInterval>;
onMounted(() => {
  void loadAttendance();
  poll = setInterval(() => {
    void loadAttendance();
  }, 5000);
});
onBeforeUnmount(() => clearInterval(poll));
</script>
<template>
  <section
    class="settings-section attendance"
    aria-labelledby="attendance-title"
  >
    <div class="attendance__head">
      <h3 id="attendance-title">塔吉多签到</h3>
      <button
        v-if="accounts.length"
        id="attendance-manage"
        type="button"
        class="btn btn--link"
        @click="manage('list')"
      >
        <svg class="icon" aria-hidden="true"><use href="#i-users" /></svg
        >管理账号
      </button>
    </div>
    <p class="attendance__hint">每天自动完成塔吉多社区与异环角色签到</p>
    <p v-if="!isTauri()" class="attendance__hint">浏览器预览使用模拟数据</p>
    <label class="switch-row"
      ><span
        ><strong>自动签到</strong
        ><small>{{
          !accounts.length
            ? "登录账号后可开启"
            : !attendanceForm.enabled
              ? "开启后按所选时机执行"
              : attendanceForm.trigger === "launch"
                ? "每次打开启动器时执行"
                : attendanceForm.trigger === "boot"
                  ? "用户登录后执行一次"
                  : attendanceForm.trigger === "daily"
                    ? "按北京时间每天执行"
                    : "只在下方的立即签到按钮中执行"
        }}</small></span
      ><input
        id="attendance-enabled"
        v-model="attendanceForm.enabled"
        type="checkbox"
        :disabled="!accounts.length || attendance.busy" /><i
        class="switch"
        aria-hidden="true"
      ></i
    ></label>
    <div v-if="attendanceForm.enabled" class="field-grid attendance__schedule">
      <div
        class="field"
        :class="{ 'field--wide': attendanceForm.trigger !== 'daily' }"
      >
        <span>签到时机</span
        ><BaseSelect
          id="attendance-trigger"
          v-model="attendanceForm.trigger"
          :options="options"
          :disabled="attendance.busy"
        />
      </div>
      <label v-if="attendanceForm.trigger === 'daily'" class="field"
        ><span>每日时间</span
        ><input
          id="attendance-time"
          v-model="attendanceForm.time"
          type="time"
          required
          :disabled="attendance.busy"
      /></label>
    </div>
    <div class="attendance__actions">
      <button
        id="attendance-action"
        type="button"
        class="btn btn--primary"
        :disabled="primary.disabled || attendance.busy"
        @click="primary.mode ? manage(primary.mode) : runAttendance()"
      >
        <svg class="icon" aria-hidden="true">
          <use :href="'#' + primary.icon" /></svg
        ><span class="btn__text">{{ primary.label }}</span>
      </button>
    </div>
    <p v-if="attendance.error" class="attendance__error" role="alert">
      {{ attendance.error }}
    </p>
    <p v-else-if="failure" class="attendance__error" role="status">
      {{ failure }}
      <button
        id="attendance-failure-details"
        type="button"
        class="btn btn--link"
        @click="manage('list')"
      >
        查看详情
      </button>
    </p>
    <TaygedoAccountDialog v-model:open="open" :start-mode="startMode" />
  </section>
</template>
