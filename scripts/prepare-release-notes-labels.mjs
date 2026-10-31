#!/usr/bin/env node
// 在生成 Release 说明前，清洗待发布 PR 的标签：
// 1. 清洗混合了业务标签的 PR 上的 ci 和 question 排除标签，避免被 GitHub Release 说明遗漏；
// 2. 为漏标 ci 的纯内部测试/CI 维护 PR 补齐排除标签，避免污染用户发布日志；
// 3. 支持识别并收敛同发布周期内的跟随修复/优化（Follow-up PR）。
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const defaultRepo =
  process.env.GH_REPO || process.env.GITHUB_REPOSITORY || "LaT-SKY/Oddity";

export const excludeLabels = new Set([
  "ci",
  "question",
  "duplicate",
  "invalid",
  "wontfix",
  "status: needs-info",
  "status: blocked",
]);

export const userFacingLabels = new Set([
  "feature",
  "bug",
  "crash",
  "security",
  "enhancement",
  "performance",
  "docs",
  "breaking",
]);

export function extractPrNumberFromSubject(subject) {
  if (!subject) return null;
  const firstLine = subject.split("\n", 1)[0].trim();
  const match =
    firstLine.match(/\(#(\d+)\)$/) ||
    firstLine.match(/^Merge pull request #(\d+)/);
  if (!match) return null;
  const number = parseInt(match[1], 10);
  return Number.isSafeInteger(number) && number > 0 ? number : null;
}

export function extractPrNumbersFromCommitMessages(messages) {
  const numbers = new Set();
  for (const message of messages) {
    const pr = extractPrNumberFromSubject(message);
    if (pr !== null) {
      numbers.add(pr);
    }
  }
  return [...numbers].sort((a, b) => a - b);
}

export function calculateLabelActions(prList) {
  const actions = [];

  for (const pr of prList) {
    const labels = pr.labels || [];
    const hasCi = labels.includes("ci");
    const hasQuestion = labels.includes("question");
    const otherLabels = labels.filter((l) => !excludeLabels.has(l));

    const toRemove = [];
    const toAdd = [];

    const title = pr.title || "";
    const isPureTestOrCi =
      /^(?:test|ci|chore\(ci\))(?:\([^)]+\))?:/i.test(title) &&
      !otherLabels.some((l) => userFacingLabels.has(l));

    // 1. 业务 PR 带有 ci 或 question：移除排除标签，使其进入 Release 说明（纯测试/CI PR 不移除 ci）
    if ((hasCi || hasQuestion) && otherLabels.length > 0) {
      if (hasCi && !isPureTestOrCi) toRemove.push("ci");
      if (hasQuestion) toRemove.push("question");
    }

    // 2. 纯测试/CI 维护 PR（如 test:、test(ci):）且无用户面向标签：补齐 ci 排除标签
    if (isPureTestOrCi && !hasCi) {
      toAdd.push("ci");
    }

    if (toRemove.length > 0 || toAdd.length > 0) {
      actions.push({
        number: pr.number,
        title: pr.title,
        toRemove,
        toAdd,
      });
    }
  }

  return actions;
}

// 已知同发布周期内的跟随修复与性能优化映射（收敛至主 PR）
export const knownFollowupMappings = {
  // alpha.2 周期
  50: { parent: 30, note: "修复更新与签到状态并发" },
  60: { parent: 30, note: "兼容金币状态字段" },
  45: { parent: 40, note: "按游戏主进程判断退出时机" },
  36: { parent: 33, note: "统一安装目录刷新并优化下载" },
  // alpha.3 周期
  102: { parent: 100, note: "优化海报生成与图表渲染性能" },
};

export function extractLinePrNumber(line) {
  if (!/^\s*[*•-]\s+/.test(line)) return null;
  // 匹配列表项归属的 PR 链接（优先匹配 "in https://.../pull/数字" 或末尾的 "/pull/数字"）
  const inMatch = line.match(/\bin\s+https?:\/\/[^\s]+\/pull\/(\d+)/i);
  if (inMatch) {
    const number = parseInt(inMatch[1], 10);
    return Number.isSafeInteger(number) && number > 0 ? number : null;
  }
  const allMatches = [...line.matchAll(/\/pull\/(\d+)/g)];
  if (allMatches.length > 0) {
    const lastMatch = allMatches[allMatches.length - 1];
    const number = parseInt(lastMatch[1], 10);
    return Number.isSafeInteger(number) && number > 0 ? number : null;
  }
  return null;
}

export function consolidateReleaseNotes(
  markdown,
  mappings = knownFollowupMappings,
  repo = defaultRepo,
) {
  const lines = markdown.split("\n");
  const result = [];
  const removedPrs = new Set();
  const presentPrs = new Set();

  for (const line of lines) {
    const prNumber = extractLinePrNumber(line);
    if (prNumber !== null) {
      presentPrs.add(prNumber);
    }
  }

  for (const line of lines) {
    const prNumber = extractLinePrNumber(line);
    if (prNumber !== null) {
      const mapping = mappings[prNumber];
      if (mapping && presentPrs.has(mapping.parent)) {
        removedPrs.add(prNumber);
        continue;
      }
    }
    result.push(line);
  }

  // 为父 PR 条目补充跟随说明与 PR 引用（防重复追加）
  for (let i = 0; i < result.length; i++) {
    const line = result[i];
    const parentPr = extractLinePrNumber(line);
    if (parentPr === null) continue;

    const children = Object.entries(mappings).filter(
      ([childPr, item]) =>
        item.parent === parentPr && removedPrs.has(Number(childPr)),
    );
    if (children.length > 0) {
      // 检查当前行是否已追加过这些跟随 PR（避免重复追加，保证幂等）
      const missingChildren = children.filter(
        ([childPr]) => !line.includes(`/pull/${childPr}`),
      );
      if (missingChildren.length > 0) {
        const notes = missingChildren
          .map(([childPr, item]) => `#${childPr} ${item.note}`)
          .join("、");
        const extraLinks = missingChildren
          .map(([childPr]) => `https://github.com/${repo}/pull/${childPr}`)
          .join(", ");
        result[i] = `${line} (含 ${notes}; ${extraLinks})`;
      }
    }
  }

  // 清理空分类标题（例如某分类下所有 PR 均被收敛或排除后，不留孤立标题）
  const finalLines = [];
  for (let i = 0; i < result.length; i++) {
    const current = result[i];
    if (current.startsWith("### ")) {
      let hasItems = false;
      for (let j = i + 1; j < result.length; j++) {
        const next = result[j];
        if (next.startsWith("#")) break;
        if (
          next.trim().startsWith("*") ||
          next.trim().startsWith("-") ||
          next.trim().startsWith("•")
        ) {
          hasItems = true;
          break;
        }
      }
      if (!hasItems) {
        continue;
      }
    }
    finalLines.push(current);
  }

  return finalLines.join("\n");
}

function runGh(args) {
  return execFileSync("gh", args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function collectPrNumbers(repo, notesStart, commit) {
  if (notesStart) {
    try {
      const compareJson = runGh([
        "api",
        `repos/${repo}/compare/${notesStart}...${commit}`,
      ]);
      const compare = JSON.parse(compareJson);
      const messages = (compare.commits || []).map(
        (item) => item.commit?.message || "",
      );
      return extractPrNumbersFromCommitMessages(messages);
    } catch (error) {
      console.warn(
        `获取提交对比范围失败: ${error.message}，尝试按合并列表获取`,
      );
    }
  }

  const numbers = new Set();
  try {
    const listJson = runGh([
      "pr",
      "list",
      "--repo",
      repo,
      "--state",
      "merged",
      "--limit",
      "100",
      "--json",
      "number",
    ]);
    for (const item of JSON.parse(listJson)) {
      numbers.add(item.number);
    }
  } catch (error) {
    console.warn(`获取 PR 列表失败: ${error.message}`);
  }

  return [...numbers].sort((a, b) => a - b);
}

function fetchPrInfo(repo, prNumbers) {
  const prMap = new Map();
  try {
    const listJson = runGh([
      "pr",
      "list",
      "--repo",
      repo,
      "--state",
      "merged",
      "--limit",
      "100",
      "--json",
      "number,title,labels",
    ]);
    for (const item of JSON.parse(listJson)) {
      prMap.set(item.number, {
        number: item.number,
        title: item.title,
        labels: item.labels.map((l) => l.name),
      });
    }
  } catch (error) {
    console.warn(`批量获取 PR 标签失败: ${error.message}`);
  }

  for (const pr of prNumbers) {
    if (!prMap.has(pr)) {
      try {
        const prJson = runGh([
          "pr",
          "view",
          String(pr),
          "--repo",
          repo,
          "--json",
          "number,title,labels",
        ]);
        const item = JSON.parse(prJson);
        prMap.set(pr, {
          number: item.number,
          title: item.title,
          labels: item.labels.map((l) => l.name),
        });
      } catch (error) {
        console.warn(`获取 PR #${pr} 标签失败: ${error.message}`);
      }
    }
  }

  return [...prMap.values()].filter((item) => prNumbers.includes(item.number));
}

export function main() {
  const repo = defaultRepo;
  const notesStart = process.env.NOTES_START || "";
  const commit = process.env.COMMIT || "HEAD";
  const dryRun = process.argv.includes("--dry-run");

  const notesFileIndex = process.argv.indexOf("--clean-notes");
  if (notesFileIndex !== -1 && process.argv[notesFileIndex + 1]) {
    const filePath = resolve(root, process.argv[notesFileIndex + 1]);
    console.log(`正在收敛整理 Release 说明文件: ${filePath}`);
    const original = readFileSync(filePath, "utf8");
    const cleaned = consolidateReleaseNotes(original);
    writeFileSync(filePath, cleaned, "utf8");
    console.log("Release 说明收敛整理完成。");
    return;
  }

  console.log(
    `准备处理待发布 PR 标签 (基准: ${notesStart || "无 (首次发布)"}, 目标: ${commit}${dryRun ? ", 模拟运行" : ""})`,
  );
  const prNumbers = collectPrNumbers(repo, notesStart, commit);
  console.log(
    `找到 ${prNumbers.length} 个相关 PR: ${prNumbers.join(", ") || "无"}`,
  );

  const prList = fetchPrInfo(repo, prNumbers);
  const actions = calculateLabelActions(prList);

  let cleanedCount = 0;
  for (const action of actions) {
    const changes = [];
    if (action.toRemove.length > 0) {
      changes.push(`移除 [${action.toRemove.join(", ")}]`);
    }
    if (action.toAdd.length > 0) {
      changes.push(`添加 [${action.toAdd.join(", ")}]`);
    }

    console.log(
      `PR #${action.number} (${action.title}): 执行 ${changes.join("，")}`,
    );

    if (!dryRun) {
      try {
        const editArgs = ["pr", "edit", String(action.number), "--repo", repo];
        for (const label of action.toRemove) {
          editArgs.push("--remove-label", label);
        }
        for (const label of action.toAdd) {
          editArgs.push("--add-label", label);
        }
        runGh(editArgs);
        cleanedCount += 1;
      } catch (error) {
        console.warn(`更新 PR #${action.number} 标签失败: ${error.message}`);
      }
    } else {
      console.log(
        `[DRY RUN] PR #${action.number} 待变更: ${changes.join("，")}`,
      );
      cleanedCount += 1;
    }
  }

  console.log(
    `PR 标签预处理完成：已规划处理 ${cleanedCount} 个 PR，保持 ${prList.length - actions.length} 个 PR 状态不变。`,
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
