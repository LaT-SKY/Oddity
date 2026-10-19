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
  const values = new Map();
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#") || trimmed.startsWith("[")) {
      continue;
    }
    const index = trimmed.indexOf("=");
    if (index > 0) {
      values.set(trimmed.slice(0, index), trimmed.slice(index + 1));
    }
  }
  return values;
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

const iconSizes = [16, 22, 24, 32, 48, 64, 128, 256, 512];
for (const size of iconSizes) {
  requireFile(`launcher/crates/nte-app/icons/${size}x${size}.png`);
}
for (const icon of [
  "launcher/crates/nte-app/icons/icon.png",
  "launcher/crates/nte-app/icons/tray-32.png",
  "launcher/crates/nte-app/icons/tray-64.png",
  "launcher/ui/assets/app-icon.svg",
  "launcher/ui/assets/tray-icon.svg",
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
  if (config.productName !== "Oddity") {
    fail(`tauri.conf.json：productName 应该是 "Oddity"`);
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

const installer = read("launcher/scripts/install-desktop.sh");
for (const needle of [
  "oddity.desktop",
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
