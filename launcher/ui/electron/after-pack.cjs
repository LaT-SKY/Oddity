const {
  rename,
  copyFile,
  chmod,
  mkdir,
  writeFile,
  symlink,
  readdir,
  lstat,
} = require("node:fs/promises");
const { join, resolve } = require("node:path");
module.exports = async ({ appOutDir }) => {
  await rename(join(appOutDir, "oddity"), join(appOutDir, "oddity-shell"));
  await copyFile(resolve("../packaging/oddity"), join(appOutDir, "oddity"));
  await chmod(join(appOutDir, "oddity"), 0o755);
  const bin = join(appOutDir, "resources/bin");
  await mkdir(bin, { recursive: true });
  await writeFile(
    join(bin, "oddity"),
    '#!/bin/sh\nexec "$(dirname "$(readlink -f "$0")")/../../oddity" "$@"\n',
    { mode: 0o755 },
  );
  for (const name of ["nte-cli", "oddity-recovery"])
    await symlink("resources/bin/" + name, join(appOutDir, name));
  const pending = [appOutDir];
  while (pending.length) {
    const path = pending.pop();
    const info = await lstat(path);
    if (info.isSymbolicLink()) continue;
    await chmod(path, info.mode & 0o755);
    if (info.isDirectory())
      for (const name of await readdir(path)) pending.push(join(path, name));
  }
};
