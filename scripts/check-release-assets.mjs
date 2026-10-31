#!/usr/bin/env node
// 校验发布目录中的包名、内部版本、依赖和关键文件.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  mkdtempSync,
  lstatSync,
  readlinkSync,
  realpathSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, isAbsolute, sep } from "node:path";
import { fileURLToPath } from "node:url";

import {
  archVersion,
  debVersion,
  projectVersion,
  rpmMetadata,
} from "./release-version.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const iconSizes = [16, 22, 24, 32, 48, 64, 128, 256, 512];
let checks = 0;

function check(condition, message) {
  assert.ok(condition, message);
  checks += 1;
}

function have(command) {
  return (
    spawnSync("sh", ["-c", `command -v ${command}`], { stdio: "ignore" })
      .status === 0
  );
}

function output(command, args, options = {}) {
  return execFileSync(command, args, { encoding: "utf8", ...options });
}

function parseArgs(argv) {
  const args = { version: "", dir: join(root, "dist/release") };
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--version") {
      args.version = argv[++index] ?? "";
    } else if (argv[index] === "--dir") {
      args.dir = resolve(argv[++index] ?? "");
    } else if (argv[index] === "--help" || argv[index] === "-h") {
      console.log(
        "用法: node scripts/check-release-assets.mjs [--version <版本>] [--dir <目录>]",
      );
      process.exit(0);
    } else {
      throw new Error(`未知参数: ${argv[index]}`);
    }
  }
  return args;
}

function verifyChecksums(directory, files) {
  const text = readFileSync(join(directory, "SHA256SUMS"), "utf8").trim();
  const entries = new Map(
    text
      .split("\n")
      .filter(Boolean)
      .map((line) => {
        const match = /^([0-9a-f]{64})  (.+)$/.exec(line);
        assert.ok(match, `SHA256SUMS 行格式错误: ${line}`);
        return [match[2], match[1]];
      }),
  );
  for (const name of files) {
    const actual = createHash("sha256")
      .update(readFileSync(join(directory, name)))
      .digest("hex");
    check(entries.get(name) === actual, `${name} 的 SHA-256 不匹配`);
  }
  check(entries.size === files.length, "SHA256SUMS 包含未发布的文件");
}

function checkDesktop(text, label) {
  check(text.includes("Exec=oddity"), `${label}: 桌面入口缺少 Exec=oddity`);
  check(
    text.includes("TryExec=oddity"),
    `${label}: 桌面入口缺少 TryExec=oddity`,
  );
  check(text.includes("Icon=oddity"), `${label}: 桌面入口缺少 Icon=oddity`);
  check(
    text.includes("Categories=Game;"),
    `${label}: 桌面入口缺少 Categories=Game`,
  );
  check(
    text.includes("StartupWMClass=oddity"),
    `${label}: 桌面入口缺少 StartupWMClass=oddity`,
  );
}

function checkDeb(path, version) {
  if (!have("dpkg-deb")) {
    console.warn("跳过 .deb 内部检查: 找不到 dpkg-deb");
    return;
  }
  check(
    output("dpkg-deb", ["-f", path, "Version"]).trim() === debVersion(version),
    ".deb 的 Version 不正确",
  );
  const depends = output("dpkg-deb", ["-f", path, "Depends"]);
  for (const name of ["libsecret-1-0", "xdg-utils"]) {
    check(depends.includes(name), `.deb 缺少依赖 ${name}`);
  }
  const contents = output("dpkg-deb", ["-c", path]);
  for (const name of [
    "usr/bin/oddity",
    "usr/bin/nte-cli",
    "usr/bin/oddity-recovery",
    "usr/share/applications/oddity-recovery.desktop",
    "usr/share/applications/oddity.desktop",
  ]) {
    check(contents.includes(name), `.deb 缺少 ${name}`);
  }

  const temporary = mkdtempSync(join(tmpdir(), "oddity-check-deb-"));
  try {
    execFileSync("dpkg-deb", ["-x", path, temporary], { stdio: "ignore" });
    checkDesktop(
      readFileSync(
        join(temporary, "usr/share/applications/oddity.desktop"),
        "utf8",
      ),
      ".deb",
    );
    for (const size of iconSizes) {
      check(
        statSync(
          join(
            temporary,
            `usr/share/icons/hicolor/${size}x${size}/apps/oddity.png`,
          ),
        ).size > 0,
        `.deb 缺少 ${size}x${size} 图标`,
      );
    }
    for (const name of [
      "usr/share/licenses/oddity/LICENSE",
      "usr/share/licenses/oddity/THIRD_PARTY.md",
    ]) {
      check(statSync(join(temporary, name)).size > 0, `.deb 缺少 ${name}`);
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

function checkRpm(path, version) {
  if (!have("rpm")) {
    console.warn("跳过 .rpm 内部检查: 找不到 rpm");
    return;
  }
  const rpm = rpmMetadata(version);
  check(
    output("rpm", [
      "-qp",
      "--qf",
      "%{VERSION} %{RELEASE} %{ARCH}\n",
      path,
    ]).trim() === `${rpm.version} ${rpm.release} x86_64`,
    ".rpm 的 VERSION/RELEASE/ARCH 不正确",
  );
  const requires = output("rpm", ["-qpR", path]);
  for (const name of ["libsecret-1.so.0()(64bit)", "xdg-utils"]) {
    check(requires.includes(name), `.rpm 缺少依赖 ${name}`);
  }
  const files = output("rpm", ["-qpl", path]);
  for (const name of [
    "/usr/bin/oddity",
    "/usr/bin/nte-cli",
    "/usr/bin/oddity-recovery",
    "/usr/share/applications/oddity-recovery.desktop",
    "/usr/share/applications/oddity.desktop",
  ]) {
    check(files.includes(name), `.rpm 缺少 ${name}`);
  }
}

function checkArch(path, version) {
  if (!have("bsdtar")) {
    console.warn("跳过 .pkg.tar.zst 内部检查: 找不到 bsdtar");
    return;
  }
  checkArchiveLinks(path);
  const files = output("bsdtar", ["-tf", path]);
  for (const name of [
    ".PKGINFO",
    ".INSTALL",
    "usr/bin/oddity",
    "usr/bin/nte-cli",
    "usr/bin/oddity-recovery",
    "usr/share/applications/oddity-recovery.desktop",
    "usr/share/applications/oddity.desktop",
    "usr/share/licenses/oddity/LICENSE",
  ]) {
    check(files.includes(name), `.pkg.tar.zst 缺少 ${name}`);
  }
  const pkginfo = output("bsdtar", ["-xOf", path, ".PKGINFO"]);
  check(
    pkginfo.includes("pkgname = oddity-bin"),
    ".pkg.tar.zst 的 pkgname 不正确",
  );
  check(
    pkginfo.includes(`pkgver = ${archVersion(version)}`),
    ".pkg.tar.zst 的 pkgver 不正确",
  );
  const depends = pkginfo
    .split("\n")
    .filter((line) => line.startsWith("depend = "))
    .join("\n");
  for (const name of ["nss", "gtk3", "libsecret", "xdg-utils"]) {
    check(depends.includes(name), `.pkg.tar.zst 缺少依赖 ${name}`);
  }
  const desktop = output("bsdtar", [
    "-xOf",
    path,
    "usr/share/applications/oddity.desktop",
  ]);
  checkDesktop(desktop, ".pkg.tar.zst");
}

function checkArchiveLinks(archive, prefix = "") {
  const temporary = mkdtempSync(join(tmpdir(), "oddity-links-"));
  try {
    const extractor = have("bsdtar") ? "bsdtar" : "tar";
    execFileSync(extractor, ["-xf", archive, "-C", temporary]);
    const boundary = join(temporary, prefix);
    const pending = [boundary];
    while (pending.length) {
      const entry = pending.pop();
      const info = lstatSync(entry);
      if (info.isSymbolicLink()) {
        const target = readlinkSync(entry);
        check(!isAbsolute(target), `归档包含绝对链接: ${entry}: ${target}`);
        let resolved;
        try {
          resolved = realpathSync(entry);
        } catch {
          check(false, `归档链接目标不存在: ${entry}: ${target}`);
          continue;
        }
        check(
          resolved === boundary || resolved.startsWith(boundary + sep),
          `归档链接逃逸: ${entry}: ${target}`,
        );
      } else if (info.isDirectory()) {
        for (const name of readdirSync(entry)) pending.push(join(entry, name));
      }
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

function checkTarball(path, version) {
  const name = `oddity-${version}-linux-x86_64`;
  checkArchiveLinks(path, name);
  const files = output("tar", ["-tzf", path]);
  for (const file of [
    `${name}/oddity`,
    `${name}/nte-cli`,
    `${name}/oddity-recovery`,
    `${name}/oddity-recovery.desktop`,
    `${name}/oddity.desktop`,
    `${name}/LICENSE`,
    `${name}/THIRD_PARTY.md`,
    `${name}/LICENSE.oddity`,
    `${name}/resources/bin/oddity-backend`,
    `${name}/resources/app.asar`,
    `${name}/oddity-shell`,
    `${name}/libffmpeg.so`,
    `${name}/LICENSES.chromium.html`,
  ]) {
    check(files.includes(file), `tarball 缺少 ${file}`);
  }
  for (const size of iconSizes) {
    check(
      files.includes(`${name}/icons/${size}x${size}.png`),
      `tarball 缺少 ${size}x${size} 图标`,
    );
  }
}

function checkAppImage(path) {
  chmodSync(path, 0o755);
  const temporary = mkdtempSync(join(tmpdir(), "oddity-check-appimage-"));
  try {
    execFileSync(path, ["--appimage-extract"], {
      cwd: temporary,
      stdio: "ignore",
      env: { ...process.env, APPIMAGE_EXTRACT_AND_RUN: "1" },
    });
    const directory = join(temporary, "squashfs-root");
    for (const file of [
      "oddity",
      "oddity-shell",
      "resources/bin/oddity-backend",
      "resources/bin/nte-cli",
      "resources/bin/oddity-recovery",
      "resources/app.asar",
      "libffmpeg.so",
      "LICENSES.chromium.html",
    ]) {
      const info = statSync(join(directory, file));
      check(info.size > 0, `AppImage 缺少 ${file}`);
      check((info.mode & 0o022) === 0, `AppImage 的 ${file} 允许其他用户写入`);
    }

    for (const size of iconSizes) {
      check(
        statSync(
          join(
            directory,
            `usr/share/icons/hicolor/${size}x${size}/apps/oddity.png`,
          ),
        ).size > 0,
        `AppImage 缺少 ${size}x${size} 图标`,
      );
    }
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const version = args.version || projectVersion();
  const rpm = rpmMetadata(version);
  const expected = [
    `oddity_${debVersion(version)}_amd64.deb`,
    `oddity-${rpm.version}-${rpm.release}.x86_64.rpm`,
    `oddity-${version}-x86_64.AppImage`,
    `oddity-${version}-linux-x86_64.tar.gz`,
    `oddity-bin-${archVersion(version)}-1-x86_64.pkg.tar.zst`,
    "SHA256SUMS",
  ];

  const actual = readdirSync(args.dir).sort();
  check(
    JSON.stringify(actual) === JSON.stringify([...expected].sort()),
    `发布资产不完整, 实际为: ${actual.join(", ")}`,
  );
  for (const name of expected) {
    check(statSync(join(args.dir, name)).size > 0, `${name} 为空`);
  }

  const payload = expected.filter((name) => name !== "SHA256SUMS");
  verifyChecksums(args.dir, payload);
  checkDeb(join(args.dir, expected[0]), version);
  checkRpm(join(args.dir, expected[1]), version);
  checkAppImage(join(args.dir, expected[2]));
  checkTarball(join(args.dir, expected[3]), version);
  checkArch(join(args.dir, expected[4]), version);

  console.log(`发布资产检查通过 (${checks} 项)`);
}

try {
  main();
} catch (error) {
  console.error(`发布资产检查失败: ${error.message}`);
  process.exit(1);
}
