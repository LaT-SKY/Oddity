#!/usr/bin/env node
// 仓库卫生检查，无需联网和编译器
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
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
  if (!existsSync(join(root, relativePath))) {
    fail(`缺少必要文件: ${relativePath}`);
  }
}

function checkVersionConsistency() {
  const cargo = read("launcher/Cargo.toml").match(
    /\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m,
  );
  if (!cargo) {
    fail("在 launcher/Cargo.toml 中找不到 [workspace.package].version");
    return;
  }

  const version = cargo[1];
  const tauri = JSON.parse(read("launcher/crates/nte-app/tauri.conf.json")).version;
  const html = read("launcher/ui/index.html").match(
    /id="app-version">v([^<]+)</,
  );
  const api = read("launcher/ui/js/api.js").match(/appVersion:\s*"([^"]+)"/);

  const versions = {
    "launcher/Cargo.toml": version,
    "launcher/crates/nte-app/tauri.conf.json": tauri,
    "launcher/ui/index.html": html?.[1],
    "launcher/ui/js/api.js": api?.[1],
  };

  for (const [file, value] of Object.entries(versions)) {
    if (!value) {
      fail(`在 ${file} 中找不到版本号`);
    } else if (value !== version) {
      fail(`${file} 中的版本错误: 期望 ${version}, 实际是 ${value}`);
    }
  }
}

function collectMarkdown(directory) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.name === "node_modules" || entry.name === "target") {
      continue;
    }
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...collectMarkdown(path));
    } else if (entry.name.endsWith(".md")) {
      files.push(path);
    }
  }
  return files;
}

function checkMarkdownLinks() {
  const files = [
    join(root, "README.md"),
    join(root, "CONTRIBUTING.md"),
    join(root, "SECURITY.md"),
    ...collectMarkdown(join(root, "docs")),
  ];
  let checked = 0;

  for (const file of files) {
    const text = readFileSync(file, "utf8");
    const targets = [];

    for (const match of text.matchAll(/!?\[[^\]]*\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g)) {
      targets.push(match[1]);
    }
    for (const match of text.matchAll(/<img\b[^>]*\bsrc="([^"]+)"/gi)) {
      targets.push(match[1]);
    }

    for (const rawTarget of targets) {
      const target = rawTarget.replace(/^<|>$/g, "");
      if (
        target.startsWith("#") ||
        /^(?:https?:|mailto:|tel:|data:)/i.test(target)
      ) {
        continue;
      }

      const pathPart = decodeURIComponent(target.split(/[?#]/, 1)[0]);
      const resolved = pathPart.startsWith("/")
        ? resolve(root, pathPart.slice(1))
        : resolve(dirname(file), pathPart);

      checked += 1;
      if (!existsSync(resolved) || !statSync(resolved).isFile()) {
        fail(
          `在 ${relative(root, file)} 中有失效的链接: ${rawTarget} (解析到 ${relative(root, resolved)})`,
        );
      }
    }
  }

  return { files: files.length, checked };
}

function checkTrackedPaths() {
  const tracked = execFileSync("git", ["ls-files"], {
    cwd: root,
    encoding: "utf8",
  })
    .split("\n")
    .filter(Boolean);

  const privatePrefixes = ["internal/", "research/", "docs/evidence/"];
  for (const path of tracked) {
    if (privatePrefixes.some((prefix) => path.startsWith(prefix))) {
      fail(`私有路径被纳入了版本管理: ${path}`);
    }
  }

  return tracked.length;
}

requireFile("README.md");
requireFile("CONTRIBUTING.md");
requireFile("SECURITY.md");
requireFile("LICENSE");
requireFile("launcher/Cargo.toml");
requireFile("launcher/Cargo.lock");
requireFile("launcher/crates/nte-app/tauri.conf.json");
requireFile("docs/install.md");
requireFile("docs/troubleshooting.md");
requireFile("launcher/ui/assets/app-icon.svg");

const ignore = read(".gitignore");
for (const required of ["internal/", "research/"]) {
  if (!ignore.split("\n").includes(required)) {
    fail(`.gitignore 中必须忽略 ${required}`);
  }
}

checkVersionConsistency();
const links = checkMarkdownLinks();
const tracked = checkTrackedPaths();

if (failures.length > 0) {
  console.error(`仓库检查失败 (共 ${failures.length} 项):`);
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log("仓库检查通过");
console.log(`  MD 文件: ${links.files}`);
console.log(`  有效的本地链接: ${links.checked}`);
console.log(`  纳入版本管理的文件: ${tracked}`);
console.log(`  版本号: ${read("launcher/Cargo.toml").match(/^version\s*=\s*"([^"]+)"/m)?.[1] ?? "unknown"}`);
