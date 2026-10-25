#!/usr/bin/env node
// 把 Tauri 的打包结果整理成发布资产, 并生成通用 tarball.
import { execFileSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { debVersion, projectVersion, rpmMetadata } from "./release-version.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconSizes = [16, 22, 24, 32, 48, 64, 128, 256, 512];

function usage() {
  console.log("用法: node scripts/stage-release.mjs [--version <版本>] [--out <目录>]");
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
    readFileSync(path);
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
    writeFileSync(control, text.replace(/^Version: .*$/m, `Version: ${version}`));
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
    mkdirSync(join(directory, "icons"), { recursive: true });
    for (const binary of ["oddity", "nte-cli"]) {
      const source = join(target, binary);
      requireFile(source);
      copyFileSync(source, join(directory, binary));
      chmodSync(join(directory, binary), 0o755);
    }

    copyFileSync(join(root, "launcher/packaging/oddity.desktop"), join(directory, "oddity.desktop"));
    for (const size of iconSizes) {
      copyFileSync(
        join(root, `launcher/crates/nte-app/icons/${size}x${size}.png`),
        join(directory, `icons/${size}x${size}.png`),
      );
    }
    for (const file of ["LICENSE", "THIRD_PARTY.md", "README.md"]) {
      copyFileSync(join(root, file), join(directory, file));
    }

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
  const target = join(root, "launcher/target/release");
  if (args.out === "/" || args.out === root) {
    throw new Error("拒绝把仓库根目录或文件系统根目录作为输出目录");
  }

  rmSync(args.out, { recursive: true, force: true });
  mkdirSync(args.out, { recursive: true });

  const debSource = requireFile(
    join(target, "bundle/deb", `oddity_${version}_amd64.deb`),
  );
  const debTarget = join(args.out, `oddity_${debVersion(version)}_amd64.deb`);
  if (debVersion(version) === version) {
    copyFileSync(debSource, debTarget);
  } else {
    rewriteDeb(debSource, debTarget, debVersion(version));
  }

  const rpm = rpmMetadata(version);
  const rpmSource = requireFile(
    join(target, "bundle/rpm", `oddity-${rpm.version}-${rpm.release}.x86_64.rpm`),
  );
  const rpmTarget = join(
    args.out,
    `oddity-${rpm.version}-${rpm.release}.x86_64.rpm`,
  );
  copyFileSync(rpmSource, rpmTarget);
  chmodSync(rpmTarget, 0o644);

  const appImageSource = requireFile(
    join(target, "bundle/appimage", `oddity_${version}_amd64.AppImage`),
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
