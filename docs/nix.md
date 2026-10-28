# Nix

仓库 flake 为 `x86_64-linux` 提供源码构建的 Oddity 包, 并带上 `umu-run`, `xdg-utils` 与 GStreamer 插件. 需要已启用 `nix-command` 和 `flakes` 的 Nix 环境.

## 直接运行

```bash
nix run github:LaT-SKY/Oddity
```

也可以指定发布标签:

```bash
nix run github:LaT-SKY/Oddity/oddity-v0.1.0-alpha.2
```

## 加入 NixOS 配置

在现有 flake 的 `inputs` 中添加:

```nix
oddity.url = "github:LaT-SKY/Oddity/oddity-v0.1.0-alpha.2";
```

将 `oddity` 传入现有 `outputs` 参数, 再在 NixOS 模块中加入:

```nix
environment.systemPackages = [ oddity.packages.x86_64-linux.default ];
```

以上为配置片段, 需要合并到自己的 flake 中.

## 游戏环境

Nix 包会提供 `umu-run`, `xdg-utils` 和 GStreamer 插件. Proton 和游戏目录仍需按[安装指南](install.md#准备游戏环境)准备; Wine prefix 由 Oddity 的运行环境管理.

首次安装游戏资源后, 在 **设置 → 游戏与启动** 中选择该区服的安装目录即可启动.
