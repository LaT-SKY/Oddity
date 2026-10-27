<script setup lang="ts">
import SettingGroup from "./settings/SettingGroup.vue";
import SettingRow from "./settings/SettingRow.vue";
import BaseButton from "./BaseButton.vue";
import { ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke, isTauri } from "../core/api";
import { notify } from "../core/notifications";

const exporting = ref(false);
const savedPath = ref("");
const error = ref("");

async function exportLogs() {
  if (exporting.value) return;
  exporting.value = true;
  savedPath.value = "";
  error.value = "";
  try {
    if (!isTauri()) throw new Error("请在 Oddity 桌面应用中导出日志");
    const destination = await open({
      title: "选择日志导出文件夹",
      directory: true,
      multiple: false,
    });
    if (destination === null) return;
    notify("正在导出日志...", "info", 0, { key: "log-export" });
    savedPath.value = await invoke("export_logs", { destination });
    notify("日志导出成功, 正在打开文件夹.", "ok", 3400, { key: "log-export" });
    try {
      await invoke("open_directory", { path: destination });
    } catch (cause) {
      error.value = `日志已导出, 但无法打开文件夹: ${String(cause)}`;
      notify(error.value, "error", 6500, { key: "log-export" });
    }
  } catch (cause) {
    error.value = `导出日志失败: ${String(cause)}`;
    notify(error.value, "error", 6500, { key: "log-export" });
  } finally {
    exporting.value = false;
  }
}
</script>

<template>
  <SettingGroup title="日志">
    <SettingRow
      label="启动与后台会话"
      description="将 Oddity 启动及后台会话日志打包为 ZIP, 保存到所选文件夹."
    >
      <BaseButton
        id="settings-export-logs"
        icon="i-download-simple"
        :busy="exporting"
        @click="exportLogs"
        >{{ exporting ? "正在导出..." : "导出日志" }}</BaseButton
      >
      <template v-if="savedPath || error" #detail>
        <small
          v-if="savedPath"
          class="field__hint log-export__result"
          role="status"
          >日志已导出至: {{ savedPath }}</small
        >
        <small
          v-if="error"
          class="field__hint log-export__result"
          role="alert"
          >{{ error }}</small
        >
      </template>
    </SettingRow>
  </SettingGroup>
</template>

<style scoped>
.log-export__result {
  overflow-wrap: anywhere;
}
</style>
