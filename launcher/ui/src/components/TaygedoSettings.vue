<script setup lang="ts">
import {
  attendanceTaskOptions,
  attendanceTasksComplete,
} from "../core/attendanceTasks";
import BaseSwitch from "./BaseSwitch.vue";
import BaseInput from "./BaseInput.vue";
import BaseDisclosure from "./BaseDisclosure.vue";
import SettingRow from "./settings/SettingRow.vue";
import SettingGroup from "./settings/SettingGroup.vue";
import BaseButton from "./BaseButton.vue";
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { isDesktop } from "../core/api";
import {
  attendance,
  attendanceForm,
  loadAttendance,
  queueSettingsSave,
  runAttendance,
} from "../core/controller";
import BaseSelect from "./BaseSelect.vue";
import TaygedoAccountDialog from "./TaygedoAccountDialog.vue";
import { settingsNavigation } from "../core/settingsCatalog";
const open = ref(false);
const scheduleExpanded = ref(true);
const tasksExpanded = ref(true);
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
  if (!Object.values(attendanceForm.tasks).some(Boolean))
    return { label: "请先授权任务", icon: "i-calendar-check", disabled: true };
  if (
    accounts.value.every((account) =>
      attendanceTasksComplete(account, attendanceForm.tasks),
    )
  )
    return { label: "今日已签到", icon: "i-check-circle", disabled: true };
  if (
    accounts.value
      .filter(
        (account) => !attendanceTasksComplete(account, attendanceForm.tasks),
      )
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
    // 只在账号分类可见时轮询, 其他分类不产生后台读取。
    if (settingsNavigation.category !== "account") return;
    void loadAttendance();
  }, 5000);
});
onBeforeUnmount(() => clearInterval(poll));
</script>
<template>
  <SettingGroup
    title="塔吉多签到"
    id="attendance-title"
    class="attendance"
    description="每天自动完成塔吉多社区与异环角色签到"
  >
    <template #action>
      <BaseButton
        v-if="accounts.length"
        id="attendance-manage"
        type="button"
        variant="ghost"
        @click="manage('list')"
      >
        <svg class="icon" aria-hidden="true"><use href="#i-users" /></svg
        >管理账号
      </BaseButton>
    </template>
    <p v-if="!isDesktop()" class="attendance__hint">浏览器预览使用模拟数据</p>
    <SettingRow
      label="自动签到"
      for="attendance-enabled"
      :description="
        !accounts.length
          ? '登录账号后可开启'
          : !attendanceForm.enabled
            ? '开启后按所选时机执行'
            : attendanceForm.trigger === 'launch'
              ? '每次打开启动器时执行'
              : attendanceForm.trigger === 'boot'
                ? '用户登录后执行一次'
                : attendanceForm.trigger === 'daily'
                  ? '按北京时间每天执行'
                  : '只在下方的立即签到按钮中执行'
      "
      v-slot="{ describedby }"
    >
      <BaseSwitch
        id="attendance-enabled"
        v-model="attendanceForm.enabled"
        label="自动签到"
        :describedby="describedby"
        :disabled="!accounts.length || attendance.busy"
        @change="queueSettingsSave(0)"
      />
    </SettingRow>
    <BaseDisclosure
      v-if="attendanceForm.enabled"
      title="签到时机"
      v-model="scheduleExpanded"
    >
      <div class="field-grid attendance__schedule">
        <div
          class="field"
          :class="{ 'field--wide': attendanceForm.trigger !== 'daily' }"
        >
          <span>签到时机</span
          ><BaseSelect
            id="attendance-trigger"
            label="签到时机"
            v-model="attendanceForm.trigger"
            :options="options"
            :disabled="attendance.busy"
            @change="queueSettingsSave(0)"
          />
        </div>
        <label v-if="attendanceForm.trigger === 'daily'" class="field"
          ><span>每日时间</span
          ><BaseInput
            id="attendance-time"
            v-model="attendanceForm.time"
            type="time"
            required
            :disabled="attendance.busy"
            @change="queueSettingsSave(0)"
        /></label>
      </div>
    </BaseDisclosure>
    <BaseDisclosure title="任务授权" v-model="tasksExpanded">
      <p class="attendance__hint">
        授权适用于所有账号, 手动与自动执行均只完成已开启的任务.
        保存后在下次执行时生效.
      </p>
      <SettingRow
        v-for="task in attendanceTaskOptions"
        :key="task.key"
        :label="task.label"
        :for="'attendance-task-' + task.key"
        :description="task.description"
        v-slot="{ describedby }"
      >
        <BaseSwitch
          :id="'attendance-task-' + task.key"
          v-model="attendanceForm.tasks[task.key]"
          :label="task.label"
          :describedby="describedby"
          :disabled="!attendance.loaded || attendance.busy"
          @change="queueSettingsSave(0)"
        />
      </SettingRow>
    </BaseDisclosure>
    <div class="attendance__actions">
      <BaseButton
        id="attendance-action"
        type="button"
        variant="primary"
        :disabled="primary.disabled || attendance.busy"
        @click="primary.mode ? manage(primary.mode) : runAttendance()"
      >
        <svg class="icon" aria-hidden="true">
          <use :href="'#' + primary.icon" /></svg
        ><span class="btn__text">{{ primary.label }}</span>
      </BaseButton>
    </div>
    <p v-if="attendance.error" class="attendance__error" role="alert">
      {{ attendance.error }}
    </p>
    <p v-else-if="failure" class="attendance__error" role="status">
      {{ failure }}
      <BaseButton
        id="attendance-failure-details"
        type="button"
        variant="ghost"
        @click="manage('list')"
      >
        查看详情
      </BaseButton>
    </p>
    <TaygedoAccountDialog v-model:open="open" :start-mode="startMode" />
  </SettingGroup>
</template>
