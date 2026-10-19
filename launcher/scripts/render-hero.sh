#!/bin/sh
# 由 hero-scene.svg 导出主页素材。依赖 rsvg-convert、ImageMagick 与 ffmpeg。
set -eu
cd "$(dirname "$0")/../ui/assets"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

im="$(command -v magick || command -v convert)"

rsvg-convert -w 2400 -h 1350 hero-scene.svg -o "$tmp/keyart.png"
"$im" "$tmp/keyart.png" -quality 90 -strip hero-keyart.jpg

rsvg-convert -w 1920 -h 1080 hero-scene.svg -o "$tmp/poster.png"
"$im" "$tmp/poster.png" -quality 86 -strip hero-poster.jpg

# 4K 输入再做缩放，避免 zoompan 取整带来的抖动
rsvg-convert -w 3840 -h 2160 hero-scene.svg -o "$tmp/art.png"
ffmpeg -hide_banner -loglevel error -y -i "$tmp/art.png" \
  -vf "zoompan=z='1.05-0.05*cos(2*PI*on/360)':x='iw/2-(iw/zoom/2)+70*sin(2*PI*on/360)':y='ih/2-(ih/zoom/2)-40*cos(2*PI*on/360)':d=360:s=1920x1080:fps=30" \
  -frames:v 360 -c:v libx264 -preset slow -crf 24 -pix_fmt yuv420p -movflags +faststart -an "$tmp/loop.mp4"
mv "$tmp/loop.mp4" hero-loop.mp4

ffmpeg -hide_banner -loglevel error -y -i hero-loop.mp4 \
  -c:v libvpx-vp9 -b:v 0 -crf 36 -row-mt 1 -an hero-loop.webm

echo "已重新导出 hero-keyart.jpg、hero-poster.jpg、hero-loop.mp4、hero-loop.webm"
