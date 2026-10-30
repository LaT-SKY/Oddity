<script setup lang="ts">
import type { SnapshotDiffRow } from "../types/launcher";

type RowGroup = { section: string; rows: SnapshotDiffRow[] };

const props = withDefaults(
  defineProps<{
    groups: RowGroup[];
    targetLabel: string;
    ariaLabel?: string;
  }>(),
  { ariaLabel: "环境差异" },
);

function statusLabel(status: SnapshotDiffRow["status"]) {
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

function identityChanged(row: SnapshotDiffRow) {
  return (
    row.status === "changed" &&
    row.baseline !== null &&
    row.baseline === row.current
  );
}
</script>

<template>
  <div class="snapshot-list" role="region" :aria-label="ariaLabel">
    <section
      v-for="group in groups"
      :key="group.section"
      class="snapshot-list__group"
    >
      <h3 class="snapshot-list__section">{{ group.section }}</h3>
      <ul class="snapshot-list__rows">
        <li v-for="row in group.rows" :key="row.key" class="snapshot-row">
          <div class="snapshot-row__head">
            <span class="snapshot-row__label">{{ row.label }}</span>
            <span class="snapshot-status" :class="'is-' + row.status">{{
              statusLabel(row.status)
            }}</span>
          </div>
          <div class="snapshot-row__values">
            <div class="snapshot-value">
              <span class="snapshot-value__label">{{ targetLabel }}</span>
              <span :class="{ 'is-missing': row.baseline === null }">{{
                row.baseline ?? "未采集"
              }}</span>
            </div>
            <span class="snapshot-value__arrow" aria-hidden="true">→</span>
            <div class="snapshot-value">
              <span class="snapshot-value__label">当前环境</span>
              <span :class="{ 'is-missing': row.current === null }">{{
                row.current ?? "未采集"
              }}</span>
            </div>
          </div>
          <small v-if="identityChanged(row)" class="snapshot-row__note"
            >标识已变化</small
          >
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
.snapshot-list {
  display: grid;
  gap: 12px;
}
.snapshot-list__group {
  display: grid;
  gap: 2px;
}
.snapshot-list__section {
  margin: 0;
  color: var(--muted);
  font-size: var(--text-xs);
  font-weight: var(--weight-medium);
}
.snapshot-list__rows {
  margin: 0;
  padding: 0;
  list-style: none;
}
.snapshot-row {
  padding: 10px 0;
  border-bottom: 1px solid var(--line);
  font-size: var(--text-sm);
  line-height: 1.6;
}
.snapshot-row:last-child {
  border-bottom: 0;
}
.snapshot-row__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}
.snapshot-row__label {
  color: var(--bone);
  font-weight: var(--weight-medium);
  overflow-wrap: anywhere;
}
.snapshot-row__values {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  gap: 8px;
  margin-top: 6px;
  align-items: start;
}
.snapshot-value {
  display: grid;
  gap: 2px;
  min-width: 0;
}
.snapshot-value__label {
  color: var(--muted);
  font-size: var(--text-xs);
}
.snapshot-value span:last-child {
  overflow-wrap: anywhere;
}
.snapshot-value .is-missing {
  color: var(--faint);
}
.snapshot-value__arrow {
  padding-top: 14px;
  color: var(--faint);
}
.snapshot-status {
  display: inline-block;
  flex: none;
  color: var(--muted);
  font-size: var(--text-xs);
  white-space: nowrap;
}
.snapshot-status.is-changed {
  color: var(--warn);
}
.snapshot-status.is-added {
  color: var(--bone);
}
.snapshot-status.is-removed {
  color: var(--danger);
}
.snapshot-status.is-unknown {
  color: var(--faint);
}
.snapshot-row__note {
  display: block;
  margin-top: 4px;
  color: var(--muted);
  font-size: var(--text-xs);
}
@media (max-width: 620px) {
  .snapshot-row__values {
    grid-template-columns: 1fr;
    gap: 4px;
  }
  .snapshot-value__arrow {
    display: none;
  }
}
</style>
