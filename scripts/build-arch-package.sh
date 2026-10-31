#!/usr/bin/env bash
# 在干净的 Arch 容器中把发布 tarball 打成 .pkg.tar.zst.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
version=${1:?"用法: $0 <版本> <tarball> <输出目录>"}
tarball=$(realpath "${2:?"缺少 tarball"}")
out=$(realpath -m "${3:?"缺少输出目录"}")
mkdir -p "$out"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

node "$root/scripts/render-pkgbuild.mjs" --version "$version" \
  --tarball "$tarball" --out "$work"

if command -v makepkg >/dev/null 2>&1; then
  (cd "$work" && makepkg --clean --nodeps --noconfirm)
  cp "$work"/*.pkg.tar.zst "$out/"
  exit 0
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "error: 需要 docker 才能在非 Arch 环境构建 .pkg.tar.zst" >&2
  exit 1
fi

docker run --rm \
  --volume "$work:/pkg:ro" \
  --volume "$out:/out" \
  archlinux:base-devel bash -lc '
    set -euo pipefail
    useradd -m builder
    mkdir -p /tmp/build
    cp -a /pkg/. /tmp/build/
    chown -R builder:builder /tmp/build
    su builder -c "cd /tmp/build && makepkg --clean --nodeps --noconfirm"
    pacman -Qip /tmp/build/*.pkg.tar.zst
    pacman -Qlp /tmp/build/*.pkg.tar.zst > /tmp/package-files
    grep -q "usr/bin/oddity" /tmp/package-files
    grep -q "usr/bin/nte-cli" /tmp/package-files
    grep -q "usr/bin/oddity-recovery" /tmp/package-files
    grep -q "usr/share/applications/oddity-recovery.desktop" /tmp/package-files
    cp /tmp/build/*.pkg.tar.zst /out/
  '
