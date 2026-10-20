#!/bin/sh
# 由 hero-scene.svg 导出主页素材。依赖 rsvg-convert、ImageMagick 与 ffmpeg。
set -eu
cd "$(dirname "$0")/../ui/public/assets"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

im="$(command -v magick || command -v convert)"

rsvg-convert -w 2400 -h 1350 hero-scene.svg -o "$tmp/keyart.png"
"$im" "$tmp/keyart.png" -quality 90 -strip hero-keyart.jpg

# 内容周期为 12 秒, 末尾另留 1 秒静帧供播放器提前回跳。
# 首尾使用同一构图, 4K 输入避免 zoompan 取整抖动。
rsvg-convert -w 3840 -h 2160 hero-scene.svg -o "$tmp/art.png"
ffmpeg -hide_banner -loglevel error -y -i "$tmp/art.png" \
  -vf "zoompan=z='1.05-0.05*cos(2*PI*on/359)':x='iw/2-(iw/zoom/2)+70*sin(2*PI*on/359)':y='ih/2-(ih/zoom/2)-40*cos(2*PI*on/359)':d=360:s=1920x1080:fps=30,tpad=stop_mode=clone:stop=30,setpts=N/(30*TB)" \
  -frames:v 390 -c:v libx264 -preset slow -crf 24 -pix_fmt yuv420p -movflags +faststart -an "$tmp/loop.mp4"
mv "$tmp/loop.mp4" hero-loop.mp4

# 封面直接取视频首帧, 保持从静态画面开始播放时的构图一致。
ffmpeg -hide_banner -loglevel error -y -i hero-loop.mp4 \
  -frames:v 1 -update 1 -q:v 2 hero-poster.jpg

ffmpeg -hide_banner -loglevel error -y -i hero-loop.mp4 \
  -c:v libvpx-vp9 -b:v 0 -crf 36 -row-mt 1 -an hero-loop.webm

echo "已重新导出 hero-keyart.jpg、hero-poster.jpg、hero-loop.mp4、hero-loop.webm"
