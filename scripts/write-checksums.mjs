#!/usr/bin/env node
// 为发布目录生成确定顺序的 SHA256SUMS.
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function main() {
  const args = process.argv.slice(2);
  let directory = join(root, "dist/release");
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--dir") {
      const value = args[++index];
      if (!value) throw new Error("--dir 缺少值");
      directory = resolve(value);
    } else {
      throw new Error(`未知参数: ${args[index]}`);
    }
  }

  const files = readdirSync(directory)
    .filter((name) => name !== "SHA256SUMS")
    .sort();
  if (files.length === 0) {
    throw new Error(`发布目录为空: ${directory}`);
  }

  const lines = files.map((name) => {
    const hash = createHash("sha256").update(readFileSync(join(directory, name))).digest("hex");
    return `${hash}  ${name}`;
  });
  writeFileSync(join(directory, "SHA256SUMS"), `${lines.join("\n")}\n`);
  console.log(`已写入 ${join(directory, "SHA256SUMS")} (${files.length} 个文件)`);
}

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
