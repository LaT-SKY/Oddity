# Gentoo

Oddity 使用独立的 Gentoo overlay:

```text
https://github.com/LaT-SKY/oddity-overlay
```

当前提供二进制包 `games-util/oddity-bin`, overlay 只保留最新版本.

## 安装

```bash
eselect repository add oddity git https://github.com/LaT-SKY/oddity-overlay.git
emerge --sync oddity
emerge games-util/oddity-bin
```

`eselect repository` 来自 `app-eselect/eselect-repository`. 也可以用 layman 或手动 `repos.conf`.

## 运行时依赖

`umu-launcher` 不在 Gentoo 主树中, 在 GURU overlay 内:

```bash
eselect repository enable guru
emerge --sync guru
emerge games-util/umu-launcher
```

Proton 仍需由用户准备, 并在 Oddity 设置中选择 Proton 安装目录.

## 维护

- Ebuild 和 Manifest 由 Oddity 主仓库的 `gentoo.yml` 由发布工作流在 Release 发布成功后直接调用, 自动生成并推送.
- Overlay 只保留最新 ebuild; 升级时移除旧版本, 拒绝较旧版本覆盖, 同版本允许重试.
- 构建机需要 `GENTOO_OVERLAY_DEPLOY_KEY`, 这是一个只对 `oddity-overlay` 有写权限的 deploy key.
- Overlay 仓库的 `pkgcheck` workflow 会检查 ebuild QA.

## 手动生成本地 ebuild

```bash
node scripts/render-ebuild.mjs \
  --version 0.1.0-alpha.2 \
  --tarball oddity-0.1.0-alpha.2-linux-x86_64.tar.gz \
  --out /tmp/oddity-gentoo
```

然后在本地 overlay 中验证:

```bash
pkgdev manifest -d /path/to/distfiles
pkgcheck scan
```
