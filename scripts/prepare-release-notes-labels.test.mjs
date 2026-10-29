import assert from "node:assert/strict";
import test from "node:test";
import {
  calculateLabelActions,
  consolidateReleaseNotes,
  extractPrNumberFromSubject,
  extractPrNumbersFromCommitMessages,
} from "./prepare-release-notes-labels.mjs";

test("从提交首行准确提取 PR 编号，校验安全整数与非零约束，避免误判 Issue", () => {
  assert.equal(
    extractPrNumberFromSubject("feat(ui): 显示游戏下载空间预算 (#97)"),
    97,
  );
  assert.equal(
    extractPrNumberFromSubject("Merge pull request #105 from branch"),
    105,
  );
  assert.equal(extractPrNumberFromSubject("chore: 无效零编号 (#0)"), null);
  assert.equal(
    extractPrNumberFromSubject("chore: 超出安全整数 (#99999999999999999999)"),
    null,
  );
  assert.equal(extractPrNumberFromSubject("Refs #37"), null);
  assert.equal(extractPrNumberFromSubject("Closes #38\n\nFix #20"), null);
  assert.equal(extractPrNumberFromSubject(""), null);
});

test("批量提取多条提交消息中的 PR 编号并升序去重", () => {
  const messages = [
    "feat: 添加游玩记录与分享海报 (#100)\n\nCloses #38",
    "fix(updater): 统一下载进度的字节口径 (#99)",
    "feat(ui): 显示游戏下载空间预算 (#97)\n\nRefs #37",
    "feat: 添加游玩记录与分享海报 (#100)", // 重复
    "chore: 普通无 PR 提交",
  ];
  assert.deepEqual(extractPrNumbersFromCommitMessages(messages), [97, 99, 100]);
});

test("清洗规则：业务 PR 剥离 ci/question 排除标签，纯测试 PR 补齐 ci 标签且保持幂等", () => {
  const prList = [
    {
      number: 74,
      title: "feat(launcher): 添加独立应急恢复入口",
      labels: ["feature", "ci", "area: launcher"],
    },
    {
      number: 90,
      title: "test(ci): 修复 e2e 假时钟暂停时序",
      labels: ["chore", "area: ui"],
    },
    {
      number: 91,
      title: "test: 更新组件测试",
      labels: ["ci", "area: ui"], // 已带有 ci 的测试 PR，不应被移除 ci
    },
    {
      number: 75,
      title: "ci: 豁免当前调用路径不受影响的 zstd 公告",
      labels: ["ci"],
    },
    {
      number: 100,
      title: "feat: 添加游玩记录与分享海报",
      labels: ["feature", "area: ui"],
    },
    {
      number: 26,
      title: "feat(ui): 公告页自动拉取官方最新公告",
      labels: ["feature", "question"],
    },
  ];

  const actions = calculateLabelActions(prList);
  assert.deepEqual(actions, [
    {
      number: 74,
      title: "feat(launcher): 添加独立应急恢复入口",
      toRemove: ["ci"],
      toAdd: [],
    },
    {
      number: 90,
      title: "test(ci): 修复 e2e 假时钟暂停时序",
      toRemove: [],
      toAdd: ["ci"],
    },
    {
      number: 26,
      title: "feat(ui): 公告页自动拉取官方最新公告",
      toRemove: ["question"],
      toAdd: [],
    },
  ]);
});

test("收敛 Release 说明：将同周期跟随 PR 合并入父功能，移除孤立跟随条目，支持指定仓库且幂等", () => {
  const inputNotes = `## What's Changed
### 新增功能
* feat: 添加游玩记录与分享海报 by @LaT-SKY in https://github.com/LaT-SKY/Oddity/pull/100
### 体验优化
* perf: 优化记录页与设置页的加载开销 by @LaT-SKY in https://github.com/LaT-SKY/Oddity/pull/102
### 问题修复
* fix: 孤立修复父 PR 不在本次发布的跟随条目 by @LaT-SKY in https://github.com/LaT-SKY/Oddity/pull/200
`;

  const customMappings = {
    102: { parent: 100, note: "优化海报生成与图表渲染性能" },
    200: { parent: 999, note: "父 PR 未在本次范围中" },
  };

  const output = consolidateReleaseNotes(
    inputNotes,
    customMappings,
    "LaT-SKY/Oddity",
  );
  assert.ok(
    output.includes(
      "feat: 添加游玩记录与分享海报 by @LaT-SKY in https://github.com/LaT-SKY/Oddity/pull/100 (含 #102 优化海报生成与图表渲染性能; https://github.com/LaT-SKY/Oddity/pull/102)",
    ),
  );
  assert.ok(!output.includes("### 体验优化\n* perf: 优化记录页与设置页"));
  // 父 PR 不在本次发布范围内的跟随条目应保持原样，不被误删
  assert.ok(output.includes("https://github.com/LaT-SKY/Oddity/pull/200"));

  // 二次收敛保持幂等，不会重复追加 (含 ...)
  const doubleOutput = consolidateReleaseNotes(
    output,
    customMappings,
    "LaT-SKY/Oddity",
  );
  assert.equal(doubleOutput, output);
});
