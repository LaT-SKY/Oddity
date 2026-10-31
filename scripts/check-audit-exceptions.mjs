import { readFileSync } from "node:fs";
const base = new URL("../launcher/ui/", import.meta.url);
const exceptions = JSON.parse(
  readFileSync(new URL("audit-exceptions.json", base), "utf8"),
);
const config = readFileSync(new URL("pnpm-workspace.yaml", base), "utf8");
for (const entry of exceptions) {
  const expires = Date.parse(`${entry.expires}T00:00:00Z`);
  if (
    typeof entry.reason !== "string" ||
    !entry.reason.trim() ||
    !/^\d{4}-\d{2}-\d{2}$/.test(entry.expires) ||
    !Number.isFinite(expires) ||
    new Date(expires).toISOString().slice(0, 10) !== entry.expires ||
    Date.now() >= expires
  )
    throw new Error(`审计豁免已到期或无效: ${entry.id}`);
  if (!new RegExp(`^\\s*-\\s*${entry.id}\\s*$`, "m").test(config))
    throw new Error(`缺少对应审计配置: ${entry.id}`);
}
const configured = config.match(/GHSA-[a-z0-9-]+/g) || [];
if (configured.some((id) => !exceptions.some((entry) => entry.id === id)))
  throw new Error("存在未记录原因和到期时间的审计豁免");
console.log("审计豁免原因与到期时间检查通过");
