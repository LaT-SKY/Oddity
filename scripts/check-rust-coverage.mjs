#!/usr/bin/env node
// 校验 Rust 代码总体覆盖率与 PR 增量覆盖率
import { execSync } from "node:child_process";
import { appendFileSync, existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const lcovPath = join(root, "launcher/lcov.info");

const TOTAL_THRESHOLD = 80;
const DIFF_THRESHOLD = 80;

function parseLcov(content) {
  const files = new Map();
  let currentFile = null;
  let currentLines = null;
  let totalLines = 0;
  let coveredLines = 0;

  for (const rawLine of content.split("\n")) {
    const line = rawLine.trim();
    if (line.startsWith("SF:")) {
      let sfPath = line.slice(3).trim().replace(/\\/g, "/");
      const rootNormalized = root.replace(/\\/g, "/");
      if (sfPath.startsWith(rootNormalized)) {
        sfPath = sfPath.slice(rootNormalized.length).replace(/^\/+/, "");
      }
      if (sfPath.startsWith("crates/")) {
        sfPath = "launcher/" + sfPath;
      }
      currentFile = sfPath;
      currentLines = new Map();
      files.set(currentFile, currentLines);
    } else if (line.startsWith("DA:") && currentLines) {
      const parts = line.slice(3).split(",");
      const lineNum = parseInt(parts[0], 10);
      const hitCount = parseInt(parts[1], 10);
      currentLines.set(lineNum, hitCount);
      totalLines += 1;
      if (hitCount > 0) {
        coveredLines += 1;
      }
    } else if (line === "end_of_record") {
      currentFile = null;
      currentLines = null;
    }
  }

  return { files, totalLines, coveredLines };
}

function parseDiff(diffText) {
  const changedFiles = new Map();
  let currentFile = null;
  let currentSet = null;

  for (const line of diffText.split("\n")) {
    if (line.startsWith("+++ b/")) {
      currentFile = line.slice(6).trim();
      currentSet = new Set();
      changedFiles.set(currentFile, currentSet);
    } else if (line.startsWith("@@ ") && currentSet) {
      const match = line.match(
        /@@\s+-[0-9]+(?:,[0-9]+)?\s+\+([0-9]+)(?:,([0-9]+))?\s+@@/,
      );
      if (match) {
        const start = parseInt(match[1], 10);
        const count = match[2] !== undefined ? parseInt(match[2], 10) : 1;
        for (let i = 0; i < count; i++) {
          currentSet.add(start + i);
        }
      }
    }
  }

  return changedFiles;
}

function resolveBaseCommit() {
  const args = process.argv.slice(2);
  const baseArgIndex = args.indexOf("--base");
  let targetRef =
    baseArgIndex !== -1 && args[baseArgIndex + 1]
      ? args[baseArgIndex + 1]
      : process.env.GITHUB_BASE_REF;

  if (!targetRef) {
    if (process.env.GITHUB_EVENT_NAME === "pull_request") {
      targetRef = "main";
    } else {
      // 本地运行若在非 main 分支，默认与 main 对比
      try {
        const currentBranch = execSync("git rev-parse --abbrev-ref HEAD", {
          cwd: root,
          encoding: "utf8",
        }).trim();
        if (currentBranch && currentBranch !== "main") {
          targetRef = "main";
        }
      } catch {}
    }
  }

  if (!targetRef) return null;

  const candidates = [`origin/${targetRef}`, targetRef, "origin/main", "main"];

  for (const candidate of candidates) {
    try {
      execSync(`git rev-parse --verify "${candidate}"`, {
        cwd: root,
        stdio: "ignore",
      });
      const mergeBase = execSync(`git merge-base "${candidate}" HEAD`, {
        cwd: root,
        encoding: "utf8",
      }).trim();
      if (mergeBase) return { commit: mergeBase, ref: candidate };
    } catch {}
  }

  return null;
}

function isProductionRustFile(filePath) {
  if (!filePath.endsWith(".rs")) return false;
  if (!filePath.startsWith("launcher/crates/")) return false;
  if (
    filePath.includes("/tests/") ||
    filePath.endsWith("_tests.rs") ||
    filePath.endsWith("/tests.rs") ||
    filePath.endsWith("/build.rs")
  ) {
    return false;
  }
  return true;
}

if (!existsSync(lcovPath)) {
  console.error(`无法找到覆盖率数据文件：${lcovPath}`);
  process.exit(1);
}

const lcovContent = readFileSync(lcovPath, "utf8");
const { files: lcovFiles, totalLines, coveredLines } = parseLcov(lcovContent);

if (totalLines === 0) {
  console.error("覆盖率文件中无任何可执行代码行。");
  process.exit(1);
}

const totalPct = (coveredLines / totalLines) * 100;
const failures = [];

if (totalPct < TOTAL_THRESHOLD) {
  failures.push(
    `总体行覆盖率不足：${totalPct.toFixed(2)}% < ${TOTAL_THRESHOLD}%`,
  );
}

const base = resolveBaseCommit();
let diffSummary = null;

if (base) {
  let diffText = "";
  try {
    diffText = execSync(
      `git diff -U0 "${base.commit}"...HEAD -- launcher/crates/`,
      { cwd: root, encoding: "utf8" },
    );
  } catch (error) {
    console.warn(`获取 git diff 失败：${error.message}`);
  }

  const diffData = parseDiff(diffText);
  const fileResults = [];
  let diffExecutableTotal = 0;
  let diffCoveredTotal = 0;

  for (const [filePath, changedLines] of diffData.entries()) {
    if (!isProductionRustFile(filePath)) continue;

    const fileCoverage = lcovFiles.get(filePath);
    let executable = 0;
    let covered = 0;
    const missed = [];

    if (fileCoverage) {
      for (const line of changedLines) {
        if (fileCoverage.has(line)) {
          executable += 1;
          if (fileCoverage.get(line) > 0) {
            covered += 1;
          } else {
            missed.push(line);
          }
        }
      }
    } else {
      executable = changedLines.size;
      missed.push(...changedLines);
    }

    if (executable > 0) {
      const pct = (covered / executable) * 100;
      fileResults.push({ filePath, executable, covered, pct, missed });
      diffExecutableTotal += executable;
      diffCoveredTotal += covered;

      if (covered === 0) {
        failures.push(
          `文件 ${filePath} 变更代码行覆盖率为 0%（共 ${executable} 行可执行代码未被覆盖）`,
        );
      }
    }
  }

  const diffPct =
    diffExecutableTotal === 0
      ? 100
      : (diffCoveredTotal / diffExecutableTotal) * 100;

  if (diffExecutableTotal > 0 && diffPct < DIFF_THRESHOLD) {
    failures.push(
      `PR 增量行覆盖率不足：${diffPct.toFixed(2)}% < ${DIFF_THRESHOLD}%`,
    );
  }

  diffSummary = {
    baseRef: base.ref,
    executable: diffExecutableTotal,
    covered: diffCoveredTotal,
    pct: diffPct,
    files: fileResults,
  };
}

// 生成报告
const reportLines = [
  "### Rust 代码覆盖率",
  "",
  "| 检查项 | 覆盖率 | 门槛要求 | 状态 |",
  "|---|---:|---|:---:|",
  `| 总体行覆盖率 | ${totalPct.toFixed(2)}% (${coveredLines}/${totalLines}) | ≥ ${TOTAL_THRESHOLD}% | ${totalPct >= TOTAL_THRESHOLD ? "✅ 通过" : "❌ 未达标"} |`,
];

if (diffSummary) {
  if (diffSummary.executable === 0) {
    reportLines.push(
      `| PR 增量覆盖率 | 100.00% (0/0) | ≥ ${DIFF_THRESHOLD}% (无业务代码变更) | ✅ 通过 |`,
    );
  } else {
    const diffPassed =
      diffSummary.pct >= DIFF_THRESHOLD &&
      diffSummary.files.every((f) => f.covered > 0);
    reportLines.push(
      `| PR 增量覆盖率 | ${diffSummary.pct.toFixed(2)}% (${diffSummary.covered}/${diffSummary.executable}) | ≥ ${DIFF_THRESHOLD}% 且单文件 > 0% | ${diffPassed ? "✅ 通过" : "❌ 未达标"} |`,
    );
  }

  if (diffSummary.files.length > 0) {
    reportLines.push(
      "",
      "#### 变更代码行覆盖明细",
      "",
      "| 文件 | 变更可执行行 | 已覆盖 | 增量覆盖率 | 未覆盖行号 |",
      "|---|---:|---:|---:|---|",
    );
    for (const f of diffSummary.files) {
      reportLines.push(
        `| \`${f.filePath}\` | ${f.executable} | ${f.covered} | ${f.pct.toFixed(2)}% | ${f.missed.length > 0 ? f.missed.join(", ") : "无"} |`,
      );
    }
  }
} else {
  reportLines.push(
    "| PR 增量覆盖率 | - | 非 PR 环境（未指定对比基准），跳过增量检查 | ⚪ 跳过 |",
  );
}

reportLines.push("");
if (failures.length === 0) {
  reportLines.push("🎉 Rust 覆盖率检查全部通过。");
} else {
  reportLines.push("⚠️ 覆盖率检查未通过：");
  for (const failure of failures) {
    reportLines.push(`- ${failure}`);
  }
}
reportLines.push("");

const reportText = reportLines.join("\n");
console.log(reportText);

if (process.env.GITHUB_STEP_SUMMARY) {
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, reportText);
}

if (failures.length > 0) {
  process.exit(1);
}
