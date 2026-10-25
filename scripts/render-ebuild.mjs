#!/usr/bin/env node
// 从 release tarball 生成 Gentoo 二进制 ebuild、metadata 和 Manifest.
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { gentooVersion, projectVersion } from "./release-version.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function parseArgs(argv) {
  const args = { version: "", tarball: "", out: join(root, "dist/gentoo"), url: "" };
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
    } else if (arg === "--url") {
      const value = argv[++index];
      if (!value) throw new Error("--url 缺少值");
      args.url = value;
    } else if (arg === "--help" || arg === "-h") {
      console.log("用法: node scripts/render-ebuild.mjs [--version <版本>] --tarball <文件> [--out <目录>] [--url <下载地址>]");
      process.exit(0);
    } else {
      throw new Error(`未知参数: ${arg}`);
    }
  }
  if (!args.tarball) throw new Error("缺少 --tarball");
  return args;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version || projectVersion();
  const tarballName = basename(args.tarball);
  const data = readFileSync(args.tarball);
  const size = statSync(args.tarball).size;
  const url =
    args.url ||
    `https://github.com/LaT-SKY/Oddity/releases/download/oddity-v${version}/${tarballName}`;
  const pv = gentooVersion(version);

  if (args.out === "/" || args.out === root) {
    throw new Error("拒绝把仓库根目录或文件系统根目录作为输出目录");
  }

  rmSync(args.out, { recursive: true, force: true });
  const packageDir = join(args.out, "games-util/oddity-bin");
  mkdirSync(packageDir, { recursive: true });

  const template = readFileSync(join(root, "packaging/gentoo/oddity-bin.ebuild.in"), "utf8");
  const ebuild = template
    .replaceAll("@UPSTREAM@", version)
    .replaceAll("@URL@", url);
  writeFileSync(join(packageDir, `oddity-bin-${pv}.ebuild`), ebuild);
  copyFileSync(join(root, "packaging/gentoo/metadata.xml"), join(packageDir, "metadata.xml"));

  const blake2b = createHash("blake2b512").update(data).digest("hex");
  const sha512 = createHash("sha512").update(data).digest("hex");
  writeFileSync(
    join(packageDir, "Manifest"),
    `DIST ${tarballName} ${size} BLAKE2B ${blake2b} SHA512 ${sha512}\n`,
  );

  console.log(`已生成 ${join(packageDir, `oddity-bin-${pv}.ebuild`)}`);
  console.log(`  PV: ${pv}`);
  console.log(`  URL: ${url}`);
}

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
