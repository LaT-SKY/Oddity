# NixOS 与 Nix

Oddity 提供仓库内的 flake. 默认包 `oddity` 从源码构建.

## 直接运行

```bash
nix run github:LaT-SKY/Oddity
```

锁定发布版本:

```bash
nix run github:LaT-SKY/Oddity/oddity-v0.1.0-alpha.2
```

## NixOS 配置

把仓库添加为 flake input:

```nix
{
  inputs.oddity.url = "github:LaT-SKY/Oddity/oddity-v0.1.0-alpha.2";

  outputs = { self, nixpkgs, oddity, ... }: {
    nixosConfigurations.host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        {
          environment.systemPackages = [
            oddity.packages.x86_64-linux.default
          ];
        }
      ];
    };
  };
}
```

也可以使用 overlay:

```nix
{
  nixpkgs.overlays = [ oddity.overlays.default ];
  environment.systemPackages = [ pkgs.oddity ];
}
```

## 运行时依赖

Nix 包会把 `xdg-utils` 和 `umu-launcher` 放进包装后的 `PATH`, 但 Proton 仍需由用户准备:

- `umu-launcher` 提供 `umu-run`.
- Proton 需要包含 `proton` 和 `toolmanifest.vdf` 的安装目录.
- 游戏安装目录必须可写.

如果主页视频无法播放, 检查系统 GStreamer 插件; Nix 包会带上基础插件, 但宿主或显卡驱动仍可能影响播放.

## 开发

```bash
nix develop
```

会提供 Rust、Node.js、pnpm、WebKitGTK 和 GTK 3 等依赖. 之后仍可使用项目原本的 `cargo` / `pnpm` 命令.

## CI 与缓存

发布工作流在 Release 发布成功后直接调用 `nix.yml`, 并锁定已校验的提交. `nix.yml` 会执行 `nix flake check` 和 `nix build .#oddity`. 配置了 Cachix `oddity` 缓存时, CI 会推送构建结果; 本地用户也可以:

```bash
cachix use oddity
```
