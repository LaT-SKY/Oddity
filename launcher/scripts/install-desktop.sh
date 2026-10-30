#!/usr/bin/env bash
# Install the local development build's desktop entry + icon theme assets.
#
# KDE/GNOME resolve a Wayland window's task icon through the matching
# `oddity.desktop` entry, not through the icon embedded in the binary.
# Without this step the compositor falls back to the generic Wayland logo.
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
bin=${1:-"$root/target/debug/oddity"}
data=${XDG_DATA_HOME:-"$HOME/.local/share"}
icons="$data/icons/hicolor"
apps="$data/applications"

if [[ ! -x "$bin" ]]; then
  echo "error: executable not found: $bin" >&2
  echo "usage: $0 [path/to/oddity]" >&2
  exit 1
fi
bin=$(realpath "$bin")

# Drop the pre-rename desktop entry/icons so the app does not show up twice
# under the old name after the rebrand.
rm -f "$apps/thebetternte.desktop"
rm -f "$icons/scalable/apps/thebetternte.svg"
for size in 16 22 24 32 48 64 128 256 512; do
  rm -f "$icons/${size}x${size}/apps/thebetternte.png"
done

install -Dm644 "$root/ui/public/assets/app-icon.svg" "$icons/scalable/apps/oddity.svg"
for size in 16 22 24 32 48 64 128 256 512; do
  install -Dm644 "$root/crates/nte-app/icons/${size}x${size}.png" \
    "$icons/${size}x${size}/apps/oddity.png"
done

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
sed -e "s|^Exec=oddity$|Exec=$bin|" \
  -e "s|^Exec=oddity --recovery$|Exec=$bin --recovery|" \
  -e "s|^Exec=oddity --launch --region cn --launch-mode background$|Exec=$bin --launch --region cn --launch-mode background|" \
  -e "s|^Exec=oddity --launch --region global --launch-mode background$|Exec=$bin --launch --region global --launch-mode background|" \
  -e "s|^TryExec=oddity$|TryExec=$bin|" \
  "$root/packaging/oddity.desktop" >"$tmp"
install -Dm644 "$tmp" "$apps/oddity.desktop"
recovery="$(dirname "$bin")/oddity-recovery"
if [[ -x "$recovery" ]]; then
  sed -e "s|^Exec=oddity-recovery$|Exec=$recovery|" -e "s|^TryExec=oddity-recovery$|TryExec=$recovery|" \
    "$root/packaging/oddity-recovery.desktop" >"$tmp"
  install -Dm644 "$tmp" "$apps/oddity-recovery.desktop"
else
  echo "warning: build nte-recovery to enable emergency recovery" >&2
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$apps" >/dev/null 2>&1 || true
fi
if [[ -f "$icons/index.theme" ]] && command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$icons" >/dev/null 2>&1 || true
fi
if command -v kbuildsycoca6 >/dev/null 2>&1; then
  kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
fi

echo "installed: $apps/oddity.desktop"
echo "installed: $icons/*/apps/oddity.png"
echo "launch:    $bin"
