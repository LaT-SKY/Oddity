import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const script = join(
  dirname(fileURLToPath(import.meta.url)),
  "check-rust-coverage.mjs",
);

function checkCoverage(source, records = "") {
  const root = mkdtempSync(join(tmpdir(), "oddity-coverage-"));
  try {
    mkdirSync(join(root, "scripts"));
    mkdirSync(join(root, "launcher/crates/core/src"), { recursive: true });
    copyFileSync(script, join(root, "scripts/check-rust-coverage.mjs"));
    const git = (...args) =>
      execFileSync("git", args, { cwd: root, stdio: "pipe" });
    git("init", "-b", "main");
    git("config", "user.email", "test@example.invalid");
    git("config", "user.name", "Coverage Test");
    writeFileSync(join(root, "launcher/crates/core/src/lib.rs"), "");
    git("add", ".");
    git("commit", "-m", "base");
    git("switch", "-c", "feature");
    writeFileSync(join(root, "launcher/crates/core/src/lib.rs"), source);
    git("add", ".");
    git("commit", "-m", "change");
    writeFileSync(
      join(root, "launcher/lcov.info"),
      "SF:crates/core/src/covered.rs\nDA:1,1\nend_of_record\n" + records,
    );
    const env = { ...process.env };
    delete env.GITHUB_STEP_SUMMARY;
    return spawnSync(
      process.execPath,
      ["scripts/check-rust-coverage.mjs", "--base", "main"],
      {
        cwd: root,
        env,
        encoding: "utf8",
      },
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("没有 LCOV 记录的外部模块声明不计为可执行代码", () => {
  const result = checkCoverage(
    "pub mod logs;\nmod private;\npub(crate) mod shared;\n",
  );
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.match(result.stdout, /100\.00% \(0\/0\)/);
});

test("没有 LCOV 记录的实际代码仍然失败", () => {
  const result = checkCoverage("pub fn untested() {}\n");
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /变更代码行覆盖率为 0%/);
});

test("模块声明不能掩盖同文件中缺少覆盖率的函数", () => {
  const result = checkCoverage("pub mod logs;\npub fn untested() {}\n");
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /0\.00% \(0\/1\)/);
});

test("已有 LCOV 记录中的未覆盖代码仍计入门槛", () => {
  const result = checkCoverage(
    "pub fn untested() {}\n",
    "SF:crates/core/src/lib.rs\nDA:1,0\nend_of_record\n",
  );
  assert.equal(result.status, 1, result.stdout + result.stderr);
  assert.match(result.stdout, /变更代码行覆盖率为 0%/);
});
