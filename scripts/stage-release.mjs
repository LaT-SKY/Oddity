#!/usr/bin/env node
// 整理 Electron 发行包和完整便携运行目录.
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  cpSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { debVersion, projectVersion, rpmMetadata } from "./release-version.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconSizes = [16, 22, 24, 32, 48, 64, 128, 256, 512];

function usage() {
  console.log(
    "用法: node scripts/stage-release.mjs [--version <版本>] [--out <目录>]",
  );
}

function parseArgs(argv) {
  const args = { version: "", out: join(root, "dist/release") };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--version") {
      const value = argv[++index];
      if (!value) throw new Error("--version 缺少值");
      args.version = value;
    } else if (arg === "--out") {
      const value = argv[++index];
      if (!value) throw new Error("--out 缺少值");
      args.out = resolve(value);
    } else if (arg === "--help" || arg === "-h") {
      usage();
      process.exit(0);
    } else {
      throw new Error(`未知参数: ${arg}`);
    }
  }
  return args;
}

function requireFile(path) {
  try {
    if (!statSync(path).isFile()) throw new Error(`构建产物不是文件: ${path}`);
  } catch {
    throw new Error(`缺少构建产物: ${path}`);
  }
  return path;
}

function rewriteDeb(source, destination, version) {
  const temporary = mkdtempSync(join(tmpdir(), "oddity-deb-"));
  try {
    execFileSync("dpkg-deb", ["-R", source, temporary], { stdio: "inherit" });
    const control = join(temporary, "DEBIAN/control");
    const text = readFileSync(control, "utf8");
    if (!/^Version: /m.test(text)) {
      throw new Error("Debian control 中没有 Version 字段");
    }
    writeFileSync(
      control,
      text.replace(/^Version: .*$/m, `Version: ${version}`),
    );
    execFileSync(
      "dpkg-deb",
      ["--build", "--root-owner-group", temporary, destination],
      { stdio: "inherit" },
    );
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

function stageTarball(version, target, out) {
  const stageRoot = mkdtempSync(join(tmpdir(), "oddity-stage-"));
  const name = `oddity-${version}-linux-x86_64`;
  const directory = join(stageRoot, name);
  try {
    cpSync(join(root, "dist/electron/linux-unpacked"), directory, {
      recursive: true,
    });
    for (const name of [
      "oddity",
      "oddity-shell",
      "resources/bin/oddity-backend",
      "resources/bin/nte-cli",
      "resources/bin/oddity-recovery",
      "resources/app.asar",
      "libffmpeg.so",
      "LICENSES.chromium.html",
    ])
      requireFile(join(directory, name));

    const archive = join(out, `${name}.tar.gz`);
    execFileSync(
      "tar",
      [
        "-C",
        stageRoot,
        "--owner=0",
        "--group=0",
        "--numeric-owner",
        "-czf",
        archive,
        name,
      ],
      { stdio: "inherit" },
    );
    return archive;
  } finally {
    rmSync(stageRoot, { recursive: true, force: true });
  }
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version || projectVersion();
  const target = join(root, "dist/electron");
  if (args.out === "/" || args.out === root) {
    throw new Error("拒绝把仓库根目录或文件系统根目录作为输出目录");
  }

  rmSync(args.out, { recursive: true, force: true });
  mkdirSync(args.out, { recursive: true });

  const debSource = requireFile(join(target, `oddity_${version}_amd64.deb`));
  const debTarget = join(args.out, `oddity_${debVersion(version)}_amd64.deb`);
  if (
    execFileSync("dpkg-deb", ["-f", debSource, "Version"], {
      encoding: "utf8",
    }).trim() === debVersion(version)
  ) {
    copyFileSync(debSource, debTarget);
  } else {
    rewriteDeb(debSource, debTarget, debVersion(version));
  }

  const rpm = rpmMetadata(version);
  const rpmSource = requireFile(
    join(target, `oddity-${rpm.version}-${rpm.release}.x86_64.rpm`),
  );
  const rpmTarget = join(
    args.out,
    `oddity-${rpm.version}-${rpm.release}.x86_64.rpm`,
  );
  copyFileSync(rpmSource, rpmTarget);
  chmodSync(rpmTarget, 0o644);

  const appImageSource = requireFile(
    join(target, `oddity-${version}-x86_64.AppImage`),
  );
  const appImageTarget = join(args.out, `oddity-${version}-x86_64.AppImage`);
  copyFileSync(appImageSource, appImageTarget);
  chmodSync(appImageTarget, 0o755);

  const archive = stageTarball(version, target, args.out);

  for (const file of [debTarget, rpmTarget, appImageTarget, archive]) {
    console.log(`staged: ${file}`);
  }
}

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
