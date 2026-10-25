#!/usr/bin/env node
// SemVer 与各 Linux 包格式版本字段的转换规则。
//
// SemVer 的预发布部分使用 "-", 但 Debian 用它分隔修订号, RPM 和 Arch
// 则直接禁止该字符出现在版本字段中. 发布脚本必须按格式转换, 不能只改文件名.

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SEMVER =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z.-]+))?(?:\+([0-9A-Za-z.-]+))?$/;

export function parseVersion(version) {
  const match = SEMVER.exec(String(version).trim());
  if (!match) {
    throw new Error(`不是合法的 SemVer 版本号: ${version}`);
  }
  return {
    core: `${match[1]}.${match[2]}.${match[3]}`,
    prerelease: match[4] ?? "",
    build: match[5] ?? "",
  };
}

export function isPrerelease(version) {
  return parseVersion(version).prerelease !== "";
}

// Debian 用 "~" 表示预发布, 否则 "0.1.0-alpha.2" 会被当作
// upstream 0.1.0 加修订号 alpha.2, 排在 0.1.0 之后.
export function debVersion(version) {
  const parsed = parseVersion(version);
  if (!parsed.prerelease) return parsed.build ? `${parsed.core}+${parsed.build}` : parsed.core;
  const prerelease = parsed.prerelease.replaceAll("-", ".");
  const build = parsed.build ? `+${parsed.build}` : "";
  return `${parsed.core}~${prerelease}${build}`;
}

// RPM 的 Version 不允许 "-", 预发布拆到 Release 的 0.* 段,
// 这样正式版的 Release 1 仍然可以正常升级.
export function rpmMetadata(version) {
  const parsed = parseVersion(version);
  if (!parsed.prerelease) {
    return { version: parsed.core, release: "1" };
  }
  return {
    version: parsed.core,
    release: `0.${parsed.prerelease.replaceAll("-", ".")}`,
  };
}

// Arch 的 vercmp 对 0.1.0alpha.2 和 0.1.0alpha2 都能正确排序,
// 这里保留 prerelease 的分段, 只替换非法的 "-".
export function archVersion(version) {
  const parsed = parseVersion(version);
  const build = parsed.build ? `+${parsed.build}` : "";
  if (!parsed.prerelease) return `${parsed.core}${build}`;
  return `${parsed.core}${parsed.prerelease.replaceAll("-", ".")}${build}`;
}

export function projectVersion() {
  const cargo = readFileSync(resolve(root, "launcher/Cargo.toml"), "utf8");
  const match = cargo.match(/\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error("在 launcher/Cargo.toml 中找不到 [workspace.package].version");
  }
  return match[1];
}

function main() {
  const args = process.argv.slice(2);
  let version = "";
  let field = "";
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--version") {
      version = args[++index] ?? "";
    } else if (arg === "--field") {
      field = args[++index] ?? "";
    } else if (arg === "--help" || arg === "-h") {
      console.log("用法: node scripts/release-version.mjs [--version <版本>] [--field deb|rpm-version|rpm-release|arch|prerelease]");
      return;
    } else {
      throw new Error(`未知参数: ${arg}`);
    }
  }

  version = version || projectVersion();
  switch (field) {
    case "":
      console.log(version);
      break;
    case "deb":
      console.log(debVersion(version));
      break;
    case "rpm-version":
      console.log(rpmMetadata(version).version);
      break;
    case "rpm-release":
      console.log(rpmMetadata(version).release);
      break;
    case "arch":
      console.log(archVersion(version));
      break;
    case "prerelease":
      console.log(isPrerelease(version) ? "true" : "false");
      break;
    default:
      throw new Error(`未知字段: ${field}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
