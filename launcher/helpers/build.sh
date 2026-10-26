#!/bin/sh
set -eu
cd "$(dirname "$0")"
# winegcc -b x86_64-windows produces real PE32+ executables (not Wine ELF).
# 窗口守卫在本轮官方 UI 线程中阻止主窗口首次映射.
winegcc -b x86_64-windows -shared -O2 -Wall -Wextra -Werror -o nte-window-guard.dll nte-window-guard.c -lcomctl32
file nte-window-guard.dll
# The production helper is a GUI-subsystem process: it must not create a
# console window in Wine when launched by umu-run.
for src in nte-launch-helper.c nte-session-helper.c; do
    exe=$(basename "$src" .c).exe
    echo "building $exe"
    winegcc -b x86_64-windows -mwindows -O2 -Wall -Wextra -Werror -o "$exe" "$src"
    file "$exe"
done

# The probes are diagnostic tools and intentionally keep a console.
for src in nte-pipe-probe.c nte-env-probe.c nte-window-probe.c; do
    exe=$(basename "$src" .c).exe
    echo "building $exe"
    winegcc -b x86_64-windows -mconsole -O2 -o "$exe" "$src"
    file "$exe"
done
