import { matchesGlob } from "node:path";

const pathRules = {
  "area: ui": ["launcher/ui/**"],
  "area: cli": ["launcher/crates/nte-cli/**"],
  "area: launcher": ["launcher/crates/nte-service/**"],
  "area: packaging": [
    "launcher/packaging/**",
    "launcher/scripts/install-desktop.sh",
    "launcher/ui/electron-builder.cjs",
    "packaging/**",
    "scripts/release-version.mjs",
    "scripts/stage-release.mjs",
    "scripts/render-pkgbuild.mjs",
    "scripts/write-checksums.mjs",
    "scripts/check-release-assets.mjs",
    "scripts/build-arch-package.sh",
    "scripts/render-ebuild.mjs",
    "flake.nix",
    "flake.lock",
    "nix/**",
  ],
  "area: updater": [
    "launcher/crates/nte-core/src/game/**",
    "launcher/crates/nte-core/src/update.rs",
    "launcher/crates/nte-core/src/manifest.rs",
  ],
  "area: proton": [
    "launcher/crates/nte-core/src/launch.rs",
    "launcher/crates/nte-core/src/config.rs",
  ],
  ci: [".github/**", "scripts/**"],
};

const typeLabels = {
  feat: "feature", fix: "bug", perf: "performance", security: "security",
  docs: "docs", refactor: "refactor", ci: "ci", release: "ci", build: "ci",
  chore: "chore", test: "chore", style: "chore", deps: "dependencies",
};
const areas = new Set(["ui", "cli", "launcher", "updater", "proton", "login", "packaging"]);
const managed = new Set([...Object.keys(pathRules), "area: login", "docs", "breaking"]);

export function calculateLabels(title, files, current) {
  const desired = new Set();
  for (const [label, patterns] of Object.entries(pathRules)) {
    if (files.some((file) => patterns.some((pattern) => matchesGlob(file, pattern)))) {
      desired.add(label);
    }
  }
  if (files.length > 0 && files.every((file) => matchesGlob(file, "{docs/**,**/*.md}"))) {
    desired.add("docs");
  }

  const match = /^([a-zA-Z]+)(?:\(([^)]+)\))?(!)?:/.exec(title);
  if (match) {
    const [, rawType, rawScope, breaking] = match;
    const type = rawType.toLowerCase();
    const scope = rawScope?.toLowerCase();
    if (typeLabels[type]) desired.add(typeLabels[type]);
    for (const area of [type, scope]) {
      if (area === "app") desired.add("area: launcher");
      else if (areas.has(area)) desired.add(`area: ${area}`);
    }
    if (breaking) desired.add("breaking");
  }

  // 类型标签保留，范围标签按标题和路径的并集同步。
  for (const label of current) {
    if (!managed.has(label)) desired.add(label);
  }
  return [...desired].sort();
}

export async function syncPrLabels({ github, context }) {
  const repo = { ...context.repo, pull_number: context.payload.pull_request.number };
  const { data: pr } = await github.rest.pulls.get(repo);
  const files = await github.paginate(github.rest.pulls.listFiles, { ...repo, per_page: 100 });
  const current = pr.labels.map((label) => label.name).sort();
  const labels = calculateLabels(pr.title, files.map((file) => file.filename), current);
  if (JSON.stringify(current) === JSON.stringify(labels)) return;
  await github.rest.issues.setLabels({
    ...context.repo,
    issue_number: repo.pull_number,
    labels,
  });
}
