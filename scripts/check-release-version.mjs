#!/usr/bin/env node
// 发布前校验 tag、tag 前缀和各版本文件中的版本号一致。
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const [version, tag] = process.argv.slice(2);
if (!version || !tag) {
  console.error("用法: node scripts/check-release-version.mjs <version> <tag>");
  process.exit(2);
}

const semver = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/;
if (!semver.test(version)) {
  console.error(`版本号不是合法的 SemVer: ${version}`);
  process.exit(1);
}
if (tag !== `oddity-v${version}`) {
  console.error(`tag 应为 oddity-v${version}，实际为 ${tag}`);
  process.exit(1);
}

const read = (path) => readFileSync(resolve(root, path), "utf8");
const versions = {};

const cargo = read("launcher/Cargo.toml").match(
  /\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m,
);
versions["launcher/Cargo.toml"] = cargo?.[1];

try {
  versions["launcher/crates/nte-app/tauri.conf.json"] = JSON.parse(
    read("launcher/crates/nte-app/tauri.conf.json"),
  ).version;
} catch (error) {
  console.error(`无法解析 tauri.conf.json: ${error.message}`);
  process.exit(1);
}

versions["launcher/ui/package.json"] = JSON.parse(read("launcher/ui/package.json")).version;

let failed = false;
for (const [file, value] of Object.entries(versions)) {
  if (value !== version) {
    console.error(`${file}: 期望 ${version}，实际 ${value ?? "未找到"}`);
    failed = true;
  }
}
if (failed) {
  process.exit(1);
}

console.log(`版本校验通过: ${tag} (${version})`);
