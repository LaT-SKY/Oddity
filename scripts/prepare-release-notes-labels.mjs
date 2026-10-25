#!/usr/bin/env node
// 在生成 Release 说明前，清洗混合了业务标签的 PR 上的 ci 和 question 排除标签
import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const repo =
  process.env.GH_REPO || process.env.GITHUB_REPOSITORY || "LaT-SKY/Oddity";

const notesStart = process.env.NOTES_START || "";
const commit = process.env.COMMIT || "HEAD";
const dryRun = process.argv.includes("--dry-run");

const excludeLabels = new Set([
  "ci",
  "question",
  "duplicate",
  "invalid",
  "wontfix",
  "status: needs-info",
  "status: blocked",
]);

function runGh(args) {
  return execFileSync("gh", args, {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function collectPrNumbers() {
  const numbers = new Set();

  if (notesStart) {
    try {
      const compareJson = runGh([
        "api",
        `repos/${repo}/compare/${notesStart}...${commit}`,
      ]);
      const compare = JSON.parse(compareJson);
      for (const item of compare.commits || []) {
        const message = item.commit?.message || "";
        for (const match of message.matchAll(
          /(?:#|Merge pull request #)(\d+)/g,
        )) {
          numbers.add(parseInt(match[1], 10));
        }
      }
    } catch (error) {
      console.warn(
        `获取提交对比范围失败: ${error.message}，尝试按合并列表获取`,
      );
    }
  }

  if (numbers.size === 0) {
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
  }

  return [...numbers].sort((a, b) => a - b);
}

function fetchPrLabels(prNumbers) {
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
      "number,labels",
    ]);
    for (const item of JSON.parse(listJson)) {
      prMap.set(
        item.number,
        item.labels.map((l) => l.name),
      );
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
          "labels",
        ]);
        prMap.set(
          pr,
          JSON.parse(prJson).labels.map((l) => l.name),
        );
      } catch (error) {
        console.warn(`获取 PR #${pr} 标签失败: ${error.message}`);
      }
    }
  }

  return prMap;
}

function main() {
  console.log(
    `准备处理待发布 PR 标签 (基准: ${notesStart || "无 (首次发布)"}, 目标: ${commit}${dryRun ? ", 模拟运行" : ""})`,
  );
  const prNumbers = collectPrNumbers();
  console.log(
    `找到 ${prNumbers.length} 个相关 PR: ${prNumbers.join(", ") || "无"}`,
  );

  const labelMap = fetchPrLabels(prNumbers);
  let cleanedCount = 0;
  let skippedCount = 0;

  for (const pr of prNumbers) {
    const labels = labelMap.get(pr) || [];
    const hasCi = labels.includes("ci");
    const hasQuestion = labels.includes("question");

    if (!hasCi && !hasQuestion) {
      continue;
    }

    const otherLabels = labels.filter((l) => !excludeLabels.has(l));

    if (otherLabels.length > 0) {
      const toRemove = [];
      if (hasCi) toRemove.push("ci");
      if (hasQuestion) toRemove.push("question");

      console.log(
        `PR #${pr} 同时包含排除标签 [${toRemove.join(", ")}] 与其他标签 [${otherLabels.join(", ")}]，执行移除...`,
      );

      if (!dryRun) {
        try {
          const editArgs = ["pr", "edit", String(pr), "--repo", repo];
          for (const label of toRemove) {
            editArgs.push("--remove-label", label);
          }
          runGh(editArgs);
          cleanedCount += 1;
        } catch (error) {
          console.warn(`更新 PR #${pr} 标签失败: ${error.message}`);
        }
      } else {
        console.log(
          `[DRY RUN] 将对 PR #${pr} 移除标签: ${toRemove.join(", ")}`,
        );
        cleanedCount += 1;
      }
    } else {
      console.log(`PR #${pr} 仅包含排除标签，保持忽略状态`);
      skippedCount += 1;
    }
  }

  console.log(
    `PR 标签预处理完成：已清洗 ${cleanedCount} 个混合标签 PR，保持排除 ${skippedCount} 个纯基建/问答 PR。`,
  );
}

main();
