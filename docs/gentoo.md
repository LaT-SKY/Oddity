# Gentoo

[oddity-overlay](https://github.com/LaT-SKY/oddity-overlay) 提供 `games-util/oddity-bin` 二进制包.

## 安装

以下命令需要 `app-eselect/eselect-repository`:

```bash
pkexec eselect repository add oddity git https://github.com/LaT-SKY/oddity-overlay.git
pkexec emerge --sync oddity
pkexec emerge --ask games-util/oddity-bin
```

`umu-launcher` 可以从 [GURU](https://github.com/gentoo/guru/tree/master/games-util/umu-launcher) 安装:

```bash
pkexec eselect repository enable guru
pkexec emerge --sync guru
pkexec emerge --ask games-util/umu-launcher
```

## 游戏环境

Proton 和游戏目录仍需按[安装指南](install.md#准备游戏环境)准备; Wine prefix 由 Oddity 的运行环境管理.
