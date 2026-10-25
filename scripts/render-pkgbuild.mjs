#!/usr/bin/env node
// 从模板生成 Arch PKGBUILD, 供本地 .pkg.tar.zst 和 AUR 发布共用.
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { archVersion, projectVersion } from "./release-version.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function parseArgs(argv) {
  const args = { version: "", tarball: "", out: join(root, "dist/arch"), releaseUrl: "" };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--version") {
      const value = argv[++index];
      if (!value) throw new Error("--version 缺少值");
      args.version = value;
    } else if (arg === "--tarball") {
      const value = argv[++index];
      if (!value) throw new Error("--tarball 缺少值");
      args.tarball = resolve(value);
    } else if (arg === "--out") {
      const value = argv[++index];
      if (!value) throw new Error("--out 缺少值");
      args.out = resolve(value);
    } else if (arg === "--release-url") {
      const value = argv[++index];
      if (!value) throw new Error("--release-url 缺少值");
      args.releaseUrl = value;
    } else if (arg === "--help" || arg === "-h") {
      console.log("用法: node scripts/render-pkgbuild.mjs [--version <版本>] --tarball <文件> [--out <目录>] [--release-url <URL>]");
      process.exit(0);
    } else {
      throw new Error(`未知参数: ${arg}`);
    }
  }
  if (!args.tarball) {
    throw new Error("缺少 --tarball");
  }
  return args;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version || projectVersion();
  const tarballName = basename(args.tarball);
  const hash = createHash("sha256").update(readFileSync(args.tarball)).digest("hex");
  if (args.out === "/" || args.out === root) {
    throw new Error("拒绝把仓库根目录或文件系统根目录作为输出目录");
  }

  rmSync(args.out, { recursive: true, force: true });
  mkdirSync(args.out, { recursive: true });

  const source = args.releaseUrl
    ? `${tarballName}::${args.releaseUrl}`
    : tarballName;
  const template = readFileSync(join(root, "packaging/arch/PKGBUILD.in"), "utf8");
  const pkgbuild = template
    .replaceAll("@PKGVER@", archVersion(version))
    .replaceAll("@UPSTREAM@", version)
    .replaceAll("@SOURCE@", source)
    .replaceAll("@SHA256@", hash);
  writeFileSync(join(args.out, "PKGBUILD"), pkgbuild);
  copyFileSync(
    join(root, "packaging/arch/oddity-bin.install"),
    join(args.out, "oddity-bin.install"),
  );
  if (!args.releaseUrl) {
    copyFileSync(args.tarball, join(args.out, tarballName));
  }

  console.log(`已生成 ${join(args.out, "PKGBUILD")}`);
  console.log(`  pkgver: ${archVersion(version)}`);
  console.log(`  source: ${source}`);
}

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
