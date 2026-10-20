#!/usr/bin/env node
// 核心门槛由 Vitest 强制执行, 组件目标只写入报告。
import { spawnSync } from "node:child_process";
import { appendFileSync, readFileSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";

const ui = fileURLToPath(new URL("../launcher/ui/", import.meta.url));
const cli = fileURLToPath(
  new URL("../launcher/ui/node_modules/vitest/vitest.mjs", import.meta.url),
);
rmSync(`${ui}coverage/coverage-summary.json`, { force: true });
const result = spawnSync(process.execPath, [cli, "run", "--coverage"], {
  cwd: ui,
  stdio: "inherit",
});
let reportFailed = false;
try {
  const summary = JSON.parse(
    readFileSync(`${ui}coverage/coverage-summary.json`, "utf8"),
  );
  const groups = { core: [], ui: [] };
  for (const [file, metrics] of Object.entries(summary)) {
    if (file === "total") continue;
    groups[file.endsWith(".vue") ? "ui" : "core"].push(metrics);
  }
  if (!groups.core.length || !groups.ui.length)
    throw new Error("覆盖率报告缺少核心 TS 或 UI 组件");
  const percent = (files, metric) => {
    const total = files.reduce((sum, file) => sum + file[metric].total, 0);
    const covered = files.reduce((sum, file) => sum + file[metric].covered, 0);
    return total === 0 ? 100 : (covered / total) * 100;
  };
  const coreLines = percent(groups.core, "lines");
  const coreBranches = percent(groups.core, "branches");
  const uiLines = percent(groups.ui, "lines");
  const uiTarget = 60;
  const report = [
    "### TypeScript / Vue 覆盖率",
    "",
    "| 范围 | 行覆盖率 | 分支覆盖率 | 要求 |",
    "|---|---:|---:|---|",
    `| 核心 TS | ${coreLines.toFixed(2)}% | ${coreBranches.toFixed(2)}% | 每个文件行 ≥ 80%, 分支 ≥ 70%, 强制 |`,
    `| UI 组件 | ${uiLines.toFixed(2)}% | ${percent(groups.ui, "branches").toFixed(2)}% | 行覆盖率参考目标 ≥ ${uiTarget}%, 不阻断 |`,
    "",
    uiLines < uiTarget
      ? "UI 组件尚未达到参考目标, 本项不阻断检查。"
      : "UI 组件已达到参考目标。",
    "",
  ].join("\n");
  console.log(report);
  if (process.env.GITHUB_STEP_SUMMARY)
    appendFileSync(process.env.GITHUB_STEP_SUMMARY, report);
} catch (error) {
  console.error(`无法读取前端覆盖率报告: ${error.message}`);
  reportFailed = true;
}
if (result.error) console.error(result.error.message);
process.exit(result.status === 0 && !reportFailed ? 0 : result.status || 1);
