import assert from "node:assert/strict";
import test from "node:test";
import { calculateLabels, syncPrLabels } from "./sync-pr-labels.mjs";

test("标题范围和路径范围取并集", () => {
  assert.deepEqual(calculateLabels("feat(proton): 添加运行环境", [
    "launcher/crates/nte-core/src/environment.rs",
    "launcher/crates/nte-app/src/environment_commands.rs",
    "launcher/ui/src/components/RuntimeEnvironments.vue",
  ], []), ["area: launcher", "area: proton", "area: ui", "feature"]);
});

test("同步失效范围和 breaking，保留人工标签及类型标签", () => {
  assert.deepEqual(calculateLabels("fix(cli): 修复参数", ["launcher/crates/nte-cli/src/main.rs"],
    ["area: proton", "area: login", "ci", "docs", "breaking", "feature", "status: blocked"]),
  ["area: cli", "bug", "feature", "status: blocked"]);
});

test("标题类型可命中 ci、docs 和范围，app 映射到 launcher", () => {
  assert.deepEqual(calculateLabels("ci: 更新构建", ["README.md"], []), ["ci", "docs"]);
  assert.deepEqual(calculateLabels("docs: 更新说明", ["launcher/ui/src/main.ts"], []), ["area: ui", "docs"]);
  assert.deepEqual(calculateLabels("app!: 调整接口", [], []), ["area: launcher", "breaking"]);
  assert.deepEqual(calculateLabels("fix(login): 修复登录", [], []), ["area: login", "bug"]);
});

test("文档仅在全部文件匹配时按路径命中，空列表不命中", () => {
  assert.deepEqual(calculateLabels("chore: 整理文件", ["README.md", "docs/example.txt"], []), ["chore", "docs"]);
  assert.deepEqual(calculateLabels("chore: 整理文件", ["README.md", "Cargo.toml"], []), ["chore"]);
  assert.deepEqual(calculateLabels("chore: 整理文件", [], []), ["chore"]);
});

test("路径规则覆盖隐藏目录、嵌套文件和精确文件名", () => {
  assert.deepEqual(calculateLabels("fix: 修复构建", [
    ".github/workflows/example.yml", "packaging/gentoo/oddity.ebuild",
    "launcher/crates/nte-core/src/game/download/mod.rs",
    "launcher/crates/nte-core/src/config.rs",
  ], []), ["area: packaging", "area: proton", "area: updater", "bug", "ci"]);
});

test("读取最新标题、分页获取路径，一次写入且重复同步不写入", async () => {
  let labels = ["status: blocked"];
  const writes = [];
  const listFiles = Symbol("listFiles");
  const github = {
    rest: {
      pulls: {
        get: async () => ({ data: {
          title: "feat(proton): 添加运行环境", labels: labels.map((name) => ({ name })),
        } }),
        listFiles,
      },
      issues: {
        setLabels: async (request) => {
          writes.push(request);
          labels = request.labels;
        },
      },
    },
    paginate: async (method, request) => {
      assert.equal(method, listFiles);
      assert.deepEqual(request, { owner: "owner", repo: "repo", pull_number: 76, per_page: 100 });
      return [{ filename: "launcher/ui/src/main.ts" }];
    },
  };
  const context = {
    repo: { owner: "owner", repo: "repo" },
    payload: { pull_request: { number: 76, title: "过期标题" } },
  };
  await syncPrLabels({ github, context });
  await syncPrLabels({ github, context });
  assert.deepEqual(writes, [{ owner: "owner", repo: "repo", issue_number: 76,
    labels: ["area: proton", "area: ui", "feature", "status: blocked"] }]);
});
