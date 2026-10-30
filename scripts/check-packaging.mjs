#!/usr/bin/env node
// 打包相关的静态检查，包括桌面入口、图标和打包配置
import { existsSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const failures = [];

function fail(message) {
  failures.push(message);
}

function read(relativePath) {
  return readFileSync(join(root, relativePath), "utf8");
}

function requireFile(relativePath) {
  if (!existsSync(join(root, relativePath)) || !statSync(join(root, relativePath)).isFile()) {
    fail(`missing file: ${relativePath}`);
  }
}

function parseDesktopEntry(text) {
  return parseDesktopSections(text).get("Desktop Entry") ?? new Map();
}

function parseDesktopSections(text) {
  const sections = new Map();
  let current = null;
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed.startsWith("[") && trimmed.endsWith("]")) {
      current = new Map();
      sections.set(trimmed.slice(1, -1), current);
      continue;
    }
    if (!current || !trimmed || trimmed.startsWith("#")) continue;
    const index = trimmed.indexOf("=");
    if (index > 0) {
      current.set(trimmed.slice(0, index), trimmed.slice(index + 1));
    }
  }
  return sections;
}

const desktopPath = "launcher/packaging/oddity.desktop";
requireFile(desktopPath);
const desktop = parseDesktopEntry(read(desktopPath));

for (const [key, expected] of [
  ["Type", "Application"],
  ["Name", "Oddity"],
  ["Exec", "oddity"],
  ["TryExec", "oddity"],
  ["Icon", "oddity"],
  ["StartupWMClass", "oddity"],
]) {
  if (desktop.get(key) !== expected) {
    fail(`${desktopPath}: ${key} 应该是 "${expected}", 实际是 "${desktop.get(key) ?? ""}"`);
  }
}

if (!desktop.get("Categories")?.split(";").includes("Game")) {
  fail(`${desktopPath}: Categories 里应该包含 Game`);
}

const desktopSections = parseDesktopSections(read(desktopPath));
const desktopActions = (desktop.get("Actions") ?? "").split(";").filter(Boolean);
for (const [action, name, exec] of [
  ["LaunchCn", "异环", "oddity --launch --region cn --launch-mode background"],
  ["LaunchGlobal", "NTE", "oddity --launch --region global --launch-mode background"],
]) {
  if (!desktopActions.includes(action)) fail(`${desktopPath}: Actions 中缺少 ${action}`);
  const values = desktopSections.get(`Desktop Action ${action}`);
  if (!values) {
    fail(`${desktopPath}: 缺少 [Desktop Action ${action}]`);
    continue;
  }
  if (values.get("Name") !== name) fail(`${desktopPath}: ${action} 的 Name 应该是 "${name}"`);
  if (values.get("Exec") !== exec) fail(`${desktopPath}: ${action} 的 Exec 应该是 "${exec}"`);
}

const recoveryDesktopPath = "launcher/packaging/oddity-recovery.desktop";
requireFile(recoveryDesktopPath);
const recoveryDesktop = parseDesktopEntry(read(recoveryDesktopPath));
for (const [key, expected] of [["Type", "Application"], ["Exec", "oddity-recovery"], ["TryExec", "oddity-recovery"], ["Terminal", "false"]]) {
  if (recoveryDesktop.get(key) !== expected) fail(`${recoveryDesktopPath}: ${key} 应为 ${expected}`);
}

const iconSizes = [16, 22, 24, 32, 48, 64, 128, 256, 512];
for (const size of iconSizes) {
  requireFile(`launcher/crates/nte-app/icons/${size}x${size}.png`);
}
for (const icon of [
  "launcher/crates/nte-app/icons/icon.png",
  "launcher/crates/nte-app/icons/tray-32.png",
  "launcher/crates/nte-app/icons/tray-64.png",
  "launcher/ui/public/assets/app-icon.svg",
  "launcher/ui/public/assets/tray-icon.svg",
]) {
  requireFile(icon);
}

let config;
try {
  config = JSON.parse(read("launcher/crates/nte-app/tauri.conf.json"));
} catch (error) {
  fail(`解析 tauri.conf.json 失败: ${error.message}`);
}

if (config) {
  if (config.productName !== "oddity") {
    fail(`tauri.conf.json：productName 应该是 "oddity"`);
  }
  if (config.identifier !== "io.github.oddity.Launcher") {
    fail(`tauri.conf.json：意外的 identifier 值，实际是 "${config.identifier}"`);
  }
  if (config.bundle?.active !== true) {
    fail("tauri.conf.json：bundle.active 应该是 true");
  }
  for (const icon of config.bundle?.icon ?? []) {
    requireFile(`launcher/crates/nte-app/${icon}`);
  }
}

const tauriDir = "launcher/crates/nte-app";
const configFile = (relativePath) => resolve(root, tauriDir, relativePath);

if (config) {
  const bundle = config.bundle ?? {};
  const linux = bundle.linux ?? {};

  if (!bundle.publisher?.includes("LaT-SKY")) {
    fail("tauri.conf.json: bundle.publisher 应该包含 LaT-SKY");
  }
  if (bundle.homepage !== "https://github.com/LaT-SKY/Oddity") {
    fail("tauri.conf.json: bundle.homepage 不正确");
  }
  if (bundle.category !== "Game") {
    fail('tauri.conf.json: bundle.category 应该是 "Game"');
  }
  if (!bundle.shortDescription || !bundle.longDescription) {
    fail("tauri.conf.json: 缺少 shortDescription 或 longDescription");
  }
  if (bundle.license !== "GPL-3.0-or-later") {
    fail("tauri.conf.json: bundle.license 应该是 GPL-3.0-or-later");
  }

  for (const size of iconSizes) {
    if (!(bundle.icon ?? []).some((icon) => icon.endsWith(`/${size}x${size}.png`))) {
      fail(`tauri.conf.json: bundle.icon 缺少 ${size}x${size}.png`);
    }
  }

  for (const section of ["deb", "rpm"]) {
    const template = linux[section]?.desktopTemplate;
    if (!template) {
      fail(`tauri.conf.json: 缺少 linux.${section}.desktopTemplate`);
    } else if (!existsSync(configFile(template))) {
      fail(`tauri.conf.json: linux.${section}.desktopTemplate 找不到 ${template}`);
    }
  }

  for (const section of ["deb", "rpm", "appimage"]) {
    const files = linux[section]?.files ?? {};
    for (const name of ["/usr/bin/nte-cli", "/usr/bin/oddity-recovery", "/usr/share/applications/oddity-recovery.desktop"]) {
      if (!files[name]) fail(`tauri.conf.json: linux.${section}.files 缺少 ${name}`);
    }
  }

  // 首页视频需要 GStreamer 的 autodetect 插件, 缺少时 WebKitGTK 会直接退出.
  for (const dependency of ["gstreamer1.0-plugins-good", "libsecret-1-0", "xdg-utils"]) {
    if (!(linux.deb?.depends ?? []).includes(dependency)) {
      fail(`tauri.conf.json: linux.deb.depends 缺少 ${dependency}`);
    }
  }
  for (const dependency of ["libsecret-1.so.0()(64bit)", "xdg-utils"]) {
    if (!(linux.rpm?.depends ?? []).includes(dependency)) {
      fail(`tauri.conf.json: linux.rpm.depends 缺少 ${dependency}`);
    }
  }
  for (const section of ["deb", "rpm"]) {
    if (!(linux[section]?.recommends ?? []).includes("umu-launcher")) {
      fail(`tauri.conf.json: linux.${section}.recommends 缺少 umu-launcher`);
    }
  }
}

for (const file of [
  "packaging/arch/PKGBUILD.in",
  "packaging/arch/oddity-bin.install",
  "scripts/release-version.mjs",
  "scripts/stage-release.mjs",
  "scripts/render-pkgbuild.mjs",
  "scripts/write-checksums.mjs",
  "scripts/check-release-assets.mjs",
  "scripts/build-arch-package.sh",
  ".github/workflows/aur.yml",
  "flake.nix",
  "nix/package.nix",
  ".github/workflows/nix.yml",
  ".github/workflows/gentoo.yml",
  "packaging/gentoo/oddity-bin.ebuild.in",
  "packaging/gentoo/metadata.xml",
  "scripts/render-ebuild.mjs",
]) {
  requireFile(file);
}

const aurWorkflow = read(".github/workflows/aur.yml");
for (const needle of [
  "oddity-bin",
  "github-actions-deploy-aur@v4.1.1",
  "updpkgsums",
  "AUR_SSH_PRIVATE_KEY",
  "render-pkgbuild.mjs",
  "published",
]) {
  if (!aurWorkflow.includes(needle)) {
    fail(`AUR workflow 里没有引用 ${needle}`);
  }
}

const archTemplate = read("packaging/arch/PKGBUILD.in");
for (const needle of [
  "pkgname=oddity-bin",
  "webkit2gtk-4.1",
  "gst-plugins-good",
  "libsecret",
  "xdg-utils",
  "source_x86_64",
  "@PKGVER@",
  "@UPSTREAM@",
  "@SOURCE@",
  "@SHA256@",
]) {
  if (!archTemplate.includes(needle)) {
    fail(`PKGBUILD.in 里没有引用 ${needle}`);
  }
}

for (const [file, needles] of [
  [
    "flake.nix",
    ["packages =", "oddity", "overlays.default"],
  ],
  [
    "nix/package.nix",
    ["cargoRoot", "pnpmRoot", "webkitgtk_4_1", "installPhase"],
  ],
  [
    "packaging/gentoo/oddity-bin.ebuild.in",
    ["QA_PREBUILT", "net-libs/webkit-gtk:4.1", "media-libs/gst-plugins-good", "games-util/umu-launcher", "@UPSTREAM@", "@URL@"],
  ],
  [
    ".github/workflows/nix.yml",
    ["nix flake check", "cachix/cachix-action", "oddity"],
  ],
  [
    ".github/workflows/gentoo.yml",
    ["oddity-overlay", "render-ebuild.mjs", "GENTOO_OVERLAY_DEPLOY_KEY", "Manifest"],
  ],
]) {
  const text = read(file);
  for (const needle of needles) {
    if (!text.includes(needle)) {
      fail(`${file} 里没有引用 ${needle}`);
    }
  }
}

const installer = read("launcher/scripts/install-desktop.sh");
for (const needle of [
  "oddity.desktop",
  "--launch --region cn",
  "--launch --region global",
  "app-icon.svg",
  "crates/nte-app/icons",
]) {
  if (!installer.includes(needle)) {
    fail(`install-desktop.sh 里没有引用 ${needle}`);
  }
}

if (failures.length > 0) {
  console.error(`打包检查失败 (共 ${failures.length} 项):`);
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log("打包检查通过");
console.log(`  桌面入口: ${desktopPath}`);
console.log(`  图标大小: ${iconSizes.join(", ")}`);
console.log(`  图标数量: ${(config.bundle?.icon ?? []).length}`);
