import assert from "node:assert/strict";
import test from "node:test";

import {
  archVersion,
  debVersion,
  gentooVersion,
  isPrerelease,
  parseVersion,
  rpmMetadata,
} from "./release-version.mjs";

test("稳定版保持原版本号", () => {
  assert.equal(debVersion("0.1.0"), "0.1.0");
  assert.deepEqual(rpmMetadata("0.1.0"), { version: "0.1.0", release: "1" });
  assert.equal(archVersion("0.1.0"), "0.1.0");
  assert.equal(isPrerelease("0.1.0"), false);
});

test("预发布版本按 Debian 规则转换", () => {
  assert.equal(debVersion("0.1.0-alpha.2"), "0.1.0~alpha.2");
  assert.equal(debVersion("0.1.0-rc.1"), "0.1.0~rc.1");
  assert.equal(isPrerelease("0.1.0-alpha.2"), true);
});

test("预发布版本按 RPM 规则拆分 Version 和 Release", () => {
  assert.deepEqual(rpmMetadata("0.1.0-alpha.2"), {
    version: "0.1.0",
    release: "0.alpha.2",
  });
  assert.deepEqual(rpmMetadata("0.1.0-beta.10"), {
    version: "0.1.0",
    release: "0.beta.10",
  });
});

test("预发布版本按 Arch 规则保留可排序分段", () => {
  assert.equal(archVersion("0.1.0-alpha.2"), "0.1.0alpha.2");
  assert.equal(archVersion("0.1.0-rc.1"), "0.1.0rc.1");
});

test("预发布版本按 Gentoo 规则转换", () => {
  assert.equal(gentooVersion("0.1.0-alpha.2"), "0.1.0_alpha2");
  assert.equal(gentooVersion("0.1.0-beta.10"), "0.1.0_beta10");
  assert.equal(gentooVersion("0.1.0-rc.1"), "0.1.0_rc1");
  assert.equal(gentooVersion("0.1.0"), "0.1.0");
});

test("预发布标识中的连字符会被替换", () => {
  assert.equal(debVersion("0.1.0-alpha-2"), "0.1.0~alpha.2");
  assert.deepEqual(rpmMetadata("0.1.0-alpha-2"), {
    version: "0.1.0",
    release: "0.alpha.2",
  });
  assert.equal(archVersion("0.1.0-alpha-2"), "0.1.0alpha.2");
});

test("构建元数据不会破坏包版本", () => {
  assert.deepEqual(parseVersion("0.1.0+build.7"), {
    core: "0.1.0",
    prerelease: "",
    build: "build.7",
  });
  assert.equal(debVersion("0.1.0+build.7"), "0.1.0+build.7");
  assert.equal(archVersion("0.1.0+build.7"), "0.1.0+build.7");
});

test("非法版本直接报错", () => {
  assert.throws(() => parseVersion("0.1"), /SemVer/);
  assert.throws(() => parseVersion("v0.1.0"), /SemVer/);
});
