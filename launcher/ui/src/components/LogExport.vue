<script setup lang="ts">
import SettingGroup from "./settings/SettingGroup.vue";
import SettingRow from "./settings/SettingRow.vue";
import BaseButton from "./BaseButton.vue";
import BaseModal from "./BaseModal.vue";
import BaseDisclosure from "./BaseDisclosure.vue";
import { onBeforeUnmount, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke, isTauri } from "../core/api";
import { notify } from "../core/notifications";
import type { LogPreview } from "../types/launcher";

const exporting = ref(false);
const showSamples = ref(false);
const preview = ref<LogPreview | null>(null);
const savedPath = ref("");
const error = ref("");
let disposed = false;

async function exportLogs() {
  if (exporting.value || preview.value) return;
  exporting.value = true;
  savedPath.value = "";
  error.value = "";
  try {
    if (!isTauri()) throw new Error("请在 Oddity 桌面应用中导出日志");
    notify("正在检查日志...", "info", 0, { key: "log-export" });
    const result = await invoke("prepare_log_export");
    if (disposed) {
      await invoke("discard_log_export", { id: result.id });
      return;
    }
    showSamples.value = false;
    preview.value = result;
    notify("日志检查完成.", "ok", 3400, { key: "log-export" });
  } catch (cause) {
    error.value = `检查日志失败: ${String(cause)}`;
    notify(error.value, "error", 6500, { key: "log-export" });
  } finally {
    exporting.value = false;
  }
}

async function closePreview() {
  if (exporting.value || !preview.value) return;
  try {
    await invoke("discard_log_export", { id: preview.value.id });
    preview.value = null;
    error.value = "";
  } catch (cause) {
    error.value = `取消导出失败: ${String(cause)}`;
  }
}

async function saveLogs() {
  if (exporting.value || !preview.value) return;
  exporting.value = true;
  error.value = "";
  try {
    const destination = await open({
      title: "选择日志导出文件夹",
      directory: true,
      multiple: false,
    });
    if (destination === null) return;
    notify("正在保存脱敏日志...", "info", 0, { key: "log-export" });
    savedPath.value = await invoke("save_log_export", {
      id: preview.value.id,
      destination,
    });
    preview.value = null;
    notify("日志已导出.", "ok", 3400, {
      key: "log-export",
    });
    await invoke("open_directory", { path: destination });
  } catch (cause) {
    error.value = savedPath.value
      ? `日志已导出, 但无法打开文件夹: ${String(cause)}`
      : `保存脱敏日志失败: ${String(cause)}`;
    notify(error.value, "error", 6500, { key: "log-export" });
  } finally {
    exporting.value = false;
  }
}

onBeforeUnmount(() => {
  disposed = true;
  if (preview.value) {
    void invoke("discard_log_export", { id: preview.value.id }).catch(
      (cause) => {
        notify(`取消导出失败: ${String(cause)}`, "error", 6500);
      },
    );
  }
});
</script>

<template>
  <SettingGroup title="日志">
    <SettingRow label="启动与后台会话" description="自动隐藏日志中的隐私信息.">
      <BaseButton
        id="settings-export-logs"
        icon="i-download-simple"
        :busy="exporting"
        :disabled="preview !== null"
        @click="exportLogs"
        >{{ exporting ? "正在处理..." : "导出日志" }}</BaseButton
      >
      <template v-if="savedPath || (error && !preview)" #detail>
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
  <BaseModal
    :open="preview !== null"
    title="日志隐私检查"
    @update:open="closePreview"
  >
    <div v-if="preview" class="log-export__preview">
      <p>{{ preview.fileCount }} 个日志文件</p>
      <dl class="log-export__counts">
        <div>
          <dt>用户主目录</dt>
          <dd>{{ preview.redactions.home }} 处</dd>
        </div>
        <div>
          <dt>手机号</dt>
          <dd>{{ preview.redactions.phone }} 处</dd>
        </div>
        <div>
          <dt>认证信息</dt>
          <dd>{{ preview.redactions.credentials }} 处</dd>
        </div>
        <div>
          <dt>令牌</dt>
          <dd>{{ preview.redactions.jwt }} 处</dd>
        </div>
      </dl>
      <p v-if="preview.normalizedFiles" class="field__hint">
        {{ preview.normalizedFiles }} 个文件存在编码异常.
      </p>
      <BaseDisclosure v-model="showSamples" title="脱敏预览">
        <div class="log-export__samples">
          <p v-if="!preview.samples.length" class="field__hint">
            未检测到敏感信息.
          </p>
          <div
            v-for="(sample, index) in preview.samples"
            :key="index"
            class="log-export__sample"
          >
            <small>{{ sample.file }}:{{ sample.line }}</small>
            <pre>{{ sample.text }}</pre>
          </div>
        </div>
      </BaseDisclosure>
      <p v-if="error" role="alert" class="log-export__result">{{ error }}</p>
      <div class="log-export__actions">
        <BaseButton variant="ghost" :disabled="exporting" @click="closePreview"
          >取消</BaseButton
        >
        <BaseButton icon="i-download-simple" :busy="exporting" @click="saveLogs"
          >保存脱敏日志</BaseButton
        >
      </div>
    </div>
  </BaseModal>
</template>

<style scoped>
.log-export__result,
.log-export__sample {
  overflow-wrap: anywhere;
}
.log-export__preview,
.log-export__samples {
  display: grid;
  gap: 14px;
}
.log-export__preview > p {
  margin: 0;
}
.log-export__counts {
  display: grid;
  gap: 8px;
  margin: 0;
}
.log-export__counts > div {
  display: flex;
  justify-content: space-between;
  gap: 16px;
}
.log-export__counts dd {
  margin: 0;
}
.log-export__sample pre {
  margin: 6px 0 0;
  white-space: pre-wrap;
  font-family: var(--mono);
  font-size: var(--text-sm);
}
.log-export__actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
