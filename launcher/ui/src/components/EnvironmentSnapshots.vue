<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import BaseButton from "./BaseButton.vue";
import BaseDisclosure from "./BaseDisclosure.vue";
import BaseInput from "./BaseInput.vue";
import BaseModal from "./BaseModal.vue";
import SettingGroup from "./settings/SettingGroup.vue";
import SettingRow from "./settings/SettingRow.vue";
import { invoke } from "../core/api";
import { notify } from "../core/notifications";
import type {
  SnapshotComparison,
  SnapshotDiffRow,
  SnapshotFailure,
  SnapshotOverview,
  SnapshotReference,
  SnapshotStatus,
  SnapshotSummary,
} from "../types/launcher";

const props = withDefaults(defineProps<{ revision?: number }>(), {
  revision: 0,
});
const overview = ref<SnapshotOverview | null>(null);
const error = ref("");
let disposed = false;

const compareOpen = ref(false);
const compareTarget = ref<"baseline" | "reference">("baseline");
const comparison = ref<SnapshotComparison | null>(null);
const comparing = ref(false);
const compareError = ref("");
let compareRun = 0;

const saveOpen = ref(false);
const label = ref("");
const saving = ref(false);
const saveError = ref("");

const deleteOpen = ref(false);
const deleting = ref(false);
const deleteError = ref("");

const clearOpen = ref(false);
const clearing = ref(false);
const clearError = ref("");

const showUnchanged = ref(false);

const hasBaseline = computed(() => !!overview.value?.baseline);
const hasReference = computed(() => !!overview.value?.reference);
const hasHistory = computed(() => (overview.value?.attempts ?? 0) > 0);
const isReplacing = computed(() => hasReference.value);

const referenceDescription = computed(() => {
  const reference = overview.value?.reference;
  if (!reference) return "固定当前环境, 之后可以随时与它比较.";
  return (
    reference.label +
    " · " +
    summaryText(reference) +
    " · 保存于 " +
    formatTime(reference.savedAt)
  );
});
const baselineDescription = computed(() => {
  const baseline = overview.value?.baseline;
  if (!baseline) return "游戏连续运行满 1 分钟后, 本次启动会成为自动基准.";
  const parts = [
    summaryText(baseline),
    formatTime(baseline.capturedAt),
    durationText(baseline.durationMs),
  ].filter(Boolean);
  if (baseline.failure) parts.push(failureText(baseline.failure));
  return parts.join(" · ");
});
const latestDescription = computed(() => {
  const latest = overview.value?.latest;
  if (!latest) return "每次启动会在这里记录结果.";
  const parts = [
    summaryText(latest),
    formatTime(latest.capturedAt),
    statusLabel(latest.status),
    durationText(latest.durationMs),
  ].filter(Boolean);
  if (latest.failure) parts.push(failureText(latest.failure));
  return parts.join(" · ");
});

const comparisonSummary = computed(() => {
  const value = comparison.value;
  if (!value) return "";
  const changed = value.changed ? value.changed + " 项变化" : "未发现变化";
  return value.unknown
    ? changed + " · " + value.unknown + " 项无法比较"
    : changed;
});
const comparisonTargetLabel = computed(
  () =>
    comparison.value?.targetLabel ??
    (compareTarget.value === "reference" ? "参考快照" : "上次成功启动"),
);

const changedRows = computed(() =>
  (comparison.value?.rows ?? []).filter(
    (row) =>
      row.status === "changed" ||
      row.status === "added" ||
      row.status === "removed",
  ),
);
const unknownRows = computed(() =>
  (comparison.value?.rows ?? []).filter((row) => row.status === "unknown"),
);
const sameRows = computed(() =>
  (comparison.value?.rows ?? []).filter((row) => row.status === "same"),
);
const changedGroups = computed(() => groupRows(changedRows.value));
const unknownGroups = computed(() => groupRows(unknownRows.value));
const sameGroups = computed(() => groupRows(sameRows.value));

type RowGroup = { section: string; rows: SnapshotDiffRow[] };

function groupRows(rows: SnapshotDiffRow[]): RowGroup[] {
  const groups: RowGroup[] = [];
  for (const row of rows) {
    const last = groups[groups.length - 1];
    if (last?.section === row.section) last.rows.push(row);
    else groups.push({ section: row.section, rows: [row] });
  }
  return groups;
}

async function load() {
  error.value = "";
  try {
    const next = await invoke("environment_snapshot_overview");
    if (!disposed) overview.value = next;
  } catch (cause) {
    if (!disposed) error.value = "读取环境快照失败: " + String(cause);
  }
}

function openCompare(target: "baseline" | "reference") {
  if (target === "baseline" && !hasBaseline.value) return;
  if (target === "reference" && !hasReference.value) return;
  compareTarget.value = target;
  compareOpen.value = true;
  comparison.value = null;
  compareError.value = "";
  showUnchanged.value = false;
  void runCompare();
}

function closeCompare() {
  compareOpen.value = false;
  compareRun += 1;
  comparison.value = null;
  compareError.value = "";
}

function selectTarget(target: "baseline" | "reference") {
  if (target === compareTarget.value) return;
  if (target === "baseline" && !hasBaseline.value) return;
  if (target === "reference" && !hasReference.value) return;
  compareTarget.value = target;
  comparison.value = null;
  showUnchanged.value = false;
  void runCompare();
}

async function runCompare() {
  const run = ++compareRun;
  comparing.value = true;
  compareError.value = "";
  try {
    const next = await invoke("environment_snapshot_compare", {
      target: compareTarget.value,
    });
    if (disposed || run !== compareRun) return;
    comparison.value = next;
  } catch (cause) {
    if (disposed || run !== compareRun) return;
    compareError.value = "读取对比结果失败: " + String(cause);
  } finally {
    if (!disposed && run === compareRun) comparing.value = false;
  }
}

function openSave() {
  saveError.value = "";
  const current = overview.value;
  if (current?.reference) {
    label.value = current.reference.label;
  } else {
    const environment =
      current?.baseline?.environment ??
      current?.latest?.environment ??
      "当前环境";
    label.value = truncateLabel(environment + " · " + formatTime(Date.now()));
  }
  saveOpen.value = true;
}

async function saveReference() {
  if (saving.value) return;
  const replacing = isReplacing.value;
  saving.value = true;
  saveError.value = "";
  try {
    await invoke("environment_snapshot_save_reference", {
      label: label.value.trim() || null,
      attemptId: null,
    });
    if (disposed) return;
    saveOpen.value = false;
    notify(replacing ? "参考快照已替换." : "参考快照已保存.", "ok", 3400, {
      key: "snapshot-reference",
    });
    await load();
  } catch (cause) {
    if (disposed) return;
    saveError.value = "保存参考快照失败: " + String(cause);
    notify(saveError.value, "error", 6500, { key: "snapshot-reference" });
  } finally {
    if (!disposed) saving.value = false;
  }
}

function openDelete() {
  deleteError.value = "";
  deleteOpen.value = true;
}

async function deleteReference() {
  if (deleting.value) return;
  deleting.value = true;
  deleteError.value = "";
  try {
    await invoke("environment_snapshot_delete_reference");
    if (disposed) return;
    deleteOpen.value = false;
    notify("参考快照已删除.", "ok", 3400, { key: "snapshot-reference" });
    await load();
  } catch (cause) {
    if (disposed) return;
    deleteError.value = "删除参考快照失败: " + String(cause);
  } finally {
    if (!disposed) deleting.value = false;
  }
}

function openClear() {
  clearError.value = "";
  clearOpen.value = true;
}

async function clearHistory() {
  if (clearing.value) return;
  clearing.value = true;
  clearError.value = "";
  try {
    await invoke("environment_snapshot_clear_history");
    if (disposed) return;
    clearOpen.value = false;
    notify("启动历史已清除.", "ok", 3400, { key: "snapshot-reference" });
    await load();
  } catch (cause) {
    if (disposed) return;
    clearError.value = "清除启动历史失败: " + String(cause);
  } finally {
    if (!disposed) clearing.value = false;
  }
}

function formatTime(value: number | null | undefined) {
  if (!value) return "未知时间";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function distributionLabel(distribution: "cn" | "global") {
  return distribution === "global" ? "国际服" : "国服";
}

function summaryText(summary: SnapshotSummary | SnapshotReference) {
  return [
    summary.environment ?? "未选择运行环境",
    summary.proton ?? "未识别 Proton",
    distributionLabel(summary.distribution),
  ].join(" · ");
}

function statusLabel(status: SnapshotStatus) {
  switch (status) {
    case "starting":
      return "正在启动";
    case "running":
      return "运行中";
    case "success":
      return "成功";
    case "failed":
      return "启动失败";
    case "interrupted":
      return "已中断";
  }
}

function diffStatusLabel(status: SnapshotDiffRow["status"]) {
  switch (status) {
    case "same":
      return "未变化";
    case "changed":
      return "已变化";
    case "unknown":
      return "无法比较";
    case "added":
      return "新增";
    case "removed":
      return "移除";
  }
}

function failureText(failure: SnapshotFailure) {
  if (failure.reason === "spawn") return "启动进程失败";
  if (failure.reason === "interrupted") return "应用退出时未结束";
  return failure.exitCode === null
    ? "提前退出"
    : "提前退出 (退出码 " + failure.exitCode + ")";
}

function durationText(value: number | null) {
  if (!value || value < 0) return "";
  const minutes = Math.round(value / 60_000);
  if (minutes >= 1) return "运行 " + minutes + " 分钟";
  return "运行 " + Math.max(1, Math.round(value / 1000)) + " 秒";
}

function truncateLabel(value: string) {
  return Array.from(value).slice(0, 32).join("");
}

function sameTextChanged(row: SnapshotDiffRow) {
  return (
    row.status === "changed" &&
    row.baseline !== null &&
    row.baseline === row.current
  );
}

watch(
  () => props.revision,
  () => {
    void load();
  },
);
onMounted(() => {
  void load();
});
onBeforeUnmount(() => {
  disposed = true;
  compareRun += 1;
});
</script>

<template>
  <div class="snapshot-settings">
    <SettingGroup
      title="环境快照"
      description="记录每次启动的系统与运行环境, 便于对比变化."
    >
      <div v-show="!overview && !error" class="snapshot-state" role="status">
        正在读取环境快照...
      </div>
      <p v-show="error" class="field__hint" role="alert">{{ error }}</p>
      <div v-show="overview" class="snapshot-content">
        <SettingRow label="参考快照" :description="referenceDescription">
          <div class="snapshot-actions">
            <BaseButton
              v-if="hasReference"
              data-testid="compare-reference"
              size="sm"
              @click="openCompare('reference')"
              >对比当前环境</BaseButton
            >
            <BaseButton
              v-if="hasReference"
              data-testid="replace-reference"
              size="sm"
              variant="ghost"
              @click="openSave"
              >替换</BaseButton
            >
            <BaseButton
              v-if="hasReference"
              data-testid="delete-reference"
              size="sm"
              variant="danger"
              @click="openDelete"
              >删除</BaseButton
            >
            <BaseButton
              v-if="!hasReference"
              data-testid="save-reference"
              size="sm"
              icon="i-plus"
              @click="openSave"
              >保存当前环境为参考</BaseButton
            >
          </div>
        </SettingRow>
        <SettingRow label="上次成功启动" :description="baselineDescription">
          <BaseButton
            v-if="hasBaseline"
            data-testid="compare-baseline"
            size="sm"
            @click="openCompare('baseline')"
            >对比当前环境</BaseButton
          >
          <span v-else class="field__hint">还没有成功启动记录</span>
        </SettingRow>
        <SettingRow label="最近一次启动" :description="latestDescription">
          <span v-if="!overview?.latest" class="field__hint"
            >还没有启动记录</span
          >
        </SettingRow>
        <div class="snapshot-footer">
          <BaseButton
            v-if="hasHistory"
            data-testid="clear-history"
            size="sm"
            variant="ghost"
            @click="openClear"
            >清除启动历史</BaseButton
          >
          <small class="field__hint"
            >快照仅保存在本机; 不包含 prefix 内部文件或游戏文件.</small
          >
        </div>
      </div>
    </SettingGroup>

    <BaseModal
      :open="compareOpen"
      title="环境对比"
      @update:open="
        (value) => {
          if (!value) closeCompare();
        }
      "
    >
      <div class="snapshot-compare">
        <div class="snapshot-targets" role="group" aria-label="对比基准">
          <BaseButton
            size="sm"
            :variant="compareTarget === 'baseline' ? 'primary' : 'ghost'"
            :disabled="!hasBaseline"
            @click="selectTarget('baseline')"
            >上次成功</BaseButton
          >
          <BaseButton
            size="sm"
            :variant="compareTarget === 'reference' ? 'primary' : 'ghost'"
            :disabled="!hasReference"
            @click="selectTarget('reference')"
            >参考快照</BaseButton
          >
        </div>
        <p v-if="comparing" role="status" aria-live="polite">
          正在读取对比结果...
        </p>
        <p v-else-if="compareError" role="alert">{{ compareError }}</p>
        <template v-else-if="comparison">
          <p class="snapshot-compare__summary">
            <strong>{{ comparisonSummary }}</strong>
            <span
              >{{ comparisonTargetLabel }} ·
              {{ formatTime(comparison.targetTime) }} →
              {{ comparison.currentEnvironment ?? "未选择运行环境" }} ·
              当前</span
            >
          </p>

          <div v-if="changedGroups.length" class="snapshot-table-wrap">
            <table class="snapshot-table" aria-label="变化项">
              <thead>
                <tr>
                  <th scope="col">项目</th>
                  <th scope="col">{{ comparisonTargetLabel }}</th>
                  <th scope="col">当前环境</th>
                  <th scope="col">状态</th>
                </tr>
              </thead>
              <tbody>
                <template
                  v-for="group in changedGroups"
                  :key="'changed-' + group.section"
                >
                  <tr class="snapshot-table__section">
                    <th colspan="4">{{ group.section }}</th>
                  </tr>
                  <tr v-for="row in group.rows" :key="row.key">
                    <th scope="row">{{ row.label }}</th>
                    <td :class="{ 'is-missing': row.baseline === null }">
                      {{ row.baseline ?? "未采集" }}
                    </td>
                    <td :class="{ 'is-missing': row.current === null }">
                      {{ row.current ?? "未采集" }}
                    </td>
                    <td>
                      <span
                        class="snapshot-status"
                        :class="'is-' + row.status"
                        >{{ diffStatusLabel(row.status) }}</span
                      >
                      <small v-if="sameTextChanged(row)">标识已变化</small>
                    </td>
                  </tr>
                </template>
              </tbody>
            </table>
          </div>

          <div v-if="unknownGroups.length" class="snapshot-table-wrap">
            <h3 class="snapshot-table__title">
              无法比较 ({{ unknownRows.length }})
            </h3>
            <table class="snapshot-table" aria-label="无法比较的项">
              <thead>
                <tr>
                  <th scope="col">项目</th>
                  <th scope="col">{{ comparisonTargetLabel }}</th>
                  <th scope="col">当前环境</th>
                  <th scope="col">状态</th>
                </tr>
              </thead>
              <tbody>
                <template
                  v-for="group in unknownGroups"
                  :key="'unknown-' + group.section"
                >
                  <tr class="snapshot-table__section">
                    <th colspan="4">{{ group.section }}</th>
                  </tr>
                  <tr v-for="row in group.rows" :key="row.key">
                    <th scope="row">{{ row.label }}</th>
                    <td :class="{ 'is-missing': row.baseline === null }">
                      {{ row.baseline ?? "未采集" }}
                    </td>
                    <td :class="{ 'is-missing': row.current === null }">
                      {{ row.current ?? "未采集" }}
                    </td>
                    <td>
                      <span class="snapshot-status is-unknown">{{
                        diffStatusLabel(row.status)
                      }}</span>
                    </td>
                  </tr>
                </template>
              </tbody>
            </table>
          </div>

          <p
            v-if="!changedGroups.length && !unknownGroups.length"
            class="field__hint"
          >
            未发现变化.
          </p>

          <BaseDisclosure
            v-if="sameGroups.length"
            v-model="showUnchanged"
            :title="'未变化 (' + sameRows.length + ')'"
          >
            <div class="snapshot-table-wrap">
              <table class="snapshot-table" aria-label="未变化的项">
                <thead>
                  <tr>
                    <th scope="col">项目</th>
                    <th scope="col">{{ comparisonTargetLabel }}</th>
                    <th scope="col">当前环境</th>
                    <th scope="col">状态</th>
                  </tr>
                </thead>
                <tbody>
                  <template
                    v-for="group in sameGroups"
                    :key="'same-' + group.section"
                  >
                    <tr class="snapshot-table__section">
                      <th colspan="4">{{ group.section }}</th>
                    </tr>
                    <tr v-for="row in group.rows" :key="row.key">
                      <th scope="row">{{ row.label }}</th>
                      <td>{{ row.baseline ?? "未采集" }}</td>
                      <td>{{ row.current ?? "未采集" }}</td>
                      <td>
                        <span class="snapshot-status is-same">{{
                          diffStatusLabel(row.status)
                        }}</span>
                      </td>
                    </tr>
                  </template>
                </tbody>
              </table>
            </div>
          </BaseDisclosure>
        </template>
      </div>
    </BaseModal>

    <BaseModal
      :open="saveOpen"
      :title="isReplacing ? '替换参考快照' : '保存参考快照'"
      @update:open="
        (value) => {
          if (!value) saveOpen = false;
        }
      "
    >
      <div class="snapshot-form">
        <label for="snapshot-reference-label">名称</label>
        <BaseInput
          id="snapshot-reference-label"
          v-model="label"
          maxlength="32"
          placeholder="例如 更新驱动前"
        />
        <p class="field__hint">
          保存当前的系统, 显卡, 游戏和运行环境信息.
          参考快照不会随之后的启动自动变化.
        </p>
        <p v-if="isReplacing" class="field__hint">保存后原参考快照会被替换.</p>
        <p v-if="saveError" role="alert">{{ saveError }}</p>
        <div class="snapshot-modal-actions">
          <BaseButton
            variant="ghost"
            :disabled="saving"
            @click="saveOpen = false"
            >取消</BaseButton
          >
          <BaseButton
            data-testid="confirm-save-reference"
            :busy="saving"
            @click="saveReference"
            >保存</BaseButton
          >
        </div>
      </div>
    </BaseModal>

    <BaseModal
      :open="deleteOpen"
      title="删除参考快照"
      @update:open="
        (value) => {
          if (!value) deleteOpen = false;
        }
      "
    >
      <div class="snapshot-form">
        <p>确定删除“{{ overview?.reference?.label }}”? 此操作无法撤销.</p>
        <p class="field__hint">启动历史和运行环境不受影响.</p>
        <p v-if="deleteError" role="alert">{{ deleteError }}</p>
        <div class="snapshot-modal-actions">
          <BaseButton variant="danger" :busy="deleting" @click="deleteReference"
            >确定删除</BaseButton
          >
          <BaseButton
            variant="ghost"
            :disabled="deleting"
            @click="deleteOpen = false"
            >取消</BaseButton
          >
        </div>
      </div>
    </BaseModal>

    <BaseModal
      :open="clearOpen"
      title="清除启动历史"
      @update:open="
        (value) => {
          if (!value) clearOpen = false;
        }
      "
    >
      <div class="snapshot-form">
        <p>
          确定清除 {{ overview?.attempts ?? 0 }} 条启动历史和自动基准?
          参考快照会保留.
        </p>
        <p class="field__hint">此操作无法撤销, 不影响游戏和运行环境.</p>
        <p v-if="clearError" role="alert">{{ clearError }}</p>
        <div class="snapshot-modal-actions">
          <BaseButton variant="danger" :busy="clearing" @click="clearHistory"
            >确定清除</BaseButton
          >
          <BaseButton
            variant="ghost"
            :disabled="clearing"
            @click="clearOpen = false"
            >取消</BaseButton
          >
        </div>
      </div>
    </BaseModal>
  </div>
</template>

<style scoped>
.snapshot-state {
  padding: var(--space-16) 0;
  color: var(--muted);
  font-size: var(--text-sm);
}
.snapshot-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 8px;
}
.snapshot-footer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 10px 16px;
  padding-top: var(--space-16);
}
.snapshot-compare,
.snapshot-form {
  display: grid;
  gap: 14px;
}
.snapshot-targets,
.snapshot-modal-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.snapshot-modal-actions {
  justify-content: flex-end;
}
.snapshot-compare__summary {
  display: grid;
  gap: 4px;
  margin: 0;
  color: var(--muted);
  font-size: var(--text-sm);
}
.snapshot-compare__summary strong {
  color: var(--bone);
  font-size: var(--text-md);
  font-weight: var(--weight-medium);
}
.snapshot-table-wrap {
  overflow-x: auto;
}
.snapshot-table {
  width: 100%;
  min-width: 540px;
  border-collapse: collapse;
  font-size: var(--text-sm);
  line-height: 1.6;
}
.snapshot-table th,
.snapshot-table td {
  padding: 8px 10px 8px 0;
  border-bottom: 1px solid var(--line);
  text-align: left;
  vertical-align: top;
  overflow-wrap: anywhere;
}
.snapshot-table thead th {
  color: var(--muted);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
}
.snapshot-table tbody th[scope="row"] {
  color: var(--bone);
  font-weight: var(--weight-medium);
}
.snapshot-table__section th {
  padding-top: 14px;
  border-bottom: 0;
  color: var(--muted);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
}
.snapshot-table__title {
  margin: 0 0 6px;
  color: var(--muted);
  font-size: var(--text-sm);
  font-weight: var(--weight-medium);
}
.snapshot-table td.is-missing {
  color: var(--faint);
}
.snapshot-status {
  display: inline-block;
  color: var(--muted);
  font-size: var(--text-xs);
  white-space: nowrap;
}
.snapshot-status.is-changed,
.snapshot-status.is-added,
.snapshot-status.is-removed {
  color: var(--warn);
}
.snapshot-status.is-unknown {
  color: var(--faint);
}
.snapshot-status.is-same {
  color: var(--muted);
}
.snapshot-status + small {
  display: block;
  margin-top: 2px;
  color: var(--muted);
  font-size: var(--text-xs);
}
.snapshot-form label {
  color: var(--bone);
  font-size: var(--text-sm);
}
</style>
