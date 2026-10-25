# 安装指南

本页说明如何准备环境、安装 Oddity，并完成第一次启动。

## 前置条件

| 项目 | 要求 |
|---|---|
| 系统 | Linux x86_64，Wayland 或 X11 桌面 |
| 图形 | 支持 Vulkan 的显卡驱动；NVIDIA 用户建议使用较新的专有驱动 |
| 依赖 | WebKitGTK 4.1、GTK 3、`umu-launcher`（`umu-run`） |
| 兼容层 | Proton；推荐 **GE-Proton 11-5** 或更新版本 |
| 游戏 | 可写的安装目录，或已有的《异环》安装目录 |
| 构建 | Rust 稳定版工具链 (edition 2021), Node.js 24 LTS 与 pnpm 12.14.0 |

Oddity 可以从官方 CDN 安装游戏资源，也可以检查已有安装。已有安装的目录结构示例：

```text
Neverness To Everness/
├── NTELauncher/
│   └── Config/Config.ini
└── Client/WindowsNoEditor/HT/Binaries/Win64/HTGame.exe
```

## 安装构建依赖

仅从源码构建 Oddity 时需要这些依赖; 使用 Release 中的包可以跳过本节.

### Debian / Ubuntu

```bash
sudo apt install build-essential curl wget file libssl-dev libxdo-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

### Fedora

```bash
sudo dnf install gcc gcc-c++ make curl wget file openssl-devel \
  webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel librsvg2-devel
```

### Arch Linux

```bash
sudo pacman -S --needed base-devel curl wget file openssl \
  webkit2gtk-4.1 libappindicator-gtk3 librsvg
```

## 准备 umu-launcher 与 Proton

1. 安装 [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher)，并确认终端中可以运行：

   ```bash
   command -v umu-run
   ```

2. 安装 GE-Proton。可以使用 ProtonUp-Qt 等工具，也可以把解压后的 GE-Proton 放到：

   ```text
   ~/.local/share/Steam/compatibilitytools.d/
   ```

Oddity 会自动查找 Steam 的兼容工具目录。如果 Proton 放在其他位置，可以在设置中手动指定。

> Proton 路径必须指向安装目录，该目录中应同时包含 `proton` 和 `toolmanifest.vdf`。不要选择 `proton` 可执行文件本身。

## 安装 Oddity

### 从 Release 安装

在 [Releases](https://github.com/LaT-SKY/Oddity/releases) 下载对应发行版的包. 每个包都会安装 Oddity、`nte-cli`、桌面入口和图标.

| 系统 | 包 | 安装命令 |
|---|---|---|
| Debian / Ubuntu | `.deb` | `sudo apt install ./oddity_*_amd64.deb` |
| Fedora | `.rpm` | `sudo dnf install ./oddity-*.x86_64.rpm` |
| openSUSE | `.rpm` | `sudo zypper install ./oddity-*.x86_64.rpm` |
| Arch Linux | `.pkg.tar.zst` | `sudo pacman -U oddity-bin-*.pkg.tar.zst` |
| Arch Linux (AUR) | `oddity-bin` | `yay -S oddity-bin` 或 `paru -S oddity-bin` |
| 通用 Linux | `.AppImage` | `chmod +x oddity-*.AppImage && ./oddity-*.AppImage` |

AUR 的 `oddity-bin` 与 Release 中的 `.pkg.tar.zst` 内容一致, 都通过系统包管理器处理依赖. 原生包会处理 WebKitGTK 4.1、GTK 3、`libsecret` 和 `xdg-utils` 等依赖. `umu-launcher` 作为推荐依赖安装; Proton 仍需按上面的步骤准备.

AppImage 需要系统提供 FUSE 2. 如果无法运行, 可以使用 `./oddity-*.AppImage --appimage-extract-and-run`, 或改用对应发行版的原生包. 主页视频还依赖系统的 GStreamer 插件; 缺少时仍可使用照片模式.

### 从源码构建

```bash
git clone https://github.com/LaT-SKY/Oddity.git
cd Oddity/launcher

pnpm --dir ui install --frozen-lockfile
pnpm --dir ui run build
cargo build --release --locked -p nte-app
./scripts/install-desktop.sh ./target/release/oddity
./target/release/oddity
```

`install-desktop.sh` 只把图标和桌面入口安装到当前用户目录，不需要 `sudo`。安装后也可以从应用菜单启动 Oddity。

> 不需要桌面入口时，直接运行 `./target/release/oddity` 即可。

## 首次配置

打开左侧 **设置**：

| 设置项 | 说明 |
|---|---|
| **游戏地区与安装目录** | 国服和国际服分别设置安装根目录，例如 `~/Games/Neverness To Everness`；两个地区不能共用目录 |
| **Wine prefix** | 游戏使用的 Wine 前缀，目录中应包含 `drive_c`。留空时使用默认目录 |
| **Proton 路径** | Proton 安装目录，目录中应同时包含 `proton` 和 `toolmanifest.vdf` |
| **启动方式** | 已验证的国服启动器可选择 **后台启动** |
| **游戏退出后打开启动器** | 默认开启。检测到游戏进程结束后，自动显示 Oddity 主界面 |

![设置页](assets/launcher-settings.png)

填写后会自动保存。游戏资源下载或安装期间，游戏地区与两个安装目录会暂时禁用，其他设置仍可修改。设置页顶部的检测项会标出缺失的组件；
全部通过后再回到主页。启动器文件和游戏资源都安装在所选目录中, 切换目录后需要在新目录重新检查和安装。

### 启动方式说明

| 方式           | 适合场景         | 说明 |
|--------------|--------------|---|
| **后台启动**（推荐） | 已验证的国服启动器版本 | 等待官方资源初始化, 自动触发正常启动并登录, 隐藏官方主窗口 |
| **前台启动**     | 查看官方窗口或启动器版本尚未验证时 | 显示官方窗口, 请从窗口中点击启动游戏 |
| **直接启动**     | 开发中          | 不经过启动器登录链路，不能用于正常游玩 |

后台模式保留官方资源检查和登录交接, 由官方组件创建游戏进程. 当前自动操作仅适用于已验证的国服组件; 官方组件更新后若提示版本尚未验证, 请切换前台启动. 初始化超过等待时限时会终止本轮启动并提示失败. 关闭游戏只清理本轮后台会话, 不关闭共享 Wine 服务. 官方主窗口仍可能短暂闪现, 登录遮罩和游戏窗口保持正常显示.

Windows 与 Linux 共用安装时, 请确认 Windows 更新后, 当前 Linux 用户仍可写入 `GameLauncher.config` 和 `patchersdk_serverlist` 目录. Oddity 会在启动前检查这些运行配置的权限; 若不可写, 请先修复对应路径的所有者和权限.

## 第一次启动

1. 前往 **更新** 页，选择地区与安装目录，点击 **检查资源**。检查通过时按钮会显示 **检查通过**；发现缺失或变更时，按钮会显示 **安装游戏** 或 **更新游戏**，点击后从官方 CDN 获取启动器文件和游戏资源。
2. 更新完成后回到 **启动** 页，点击 **启动游戏**。Oddity 会通过 Proton 启动官方后台组件，完成登录与后续启动流程。
3. 启动成功后，Oddity 窗口会收起到系统托盘，游戏开始运行。游戏退出后，Oddity 默认重新显示主界面；可在设置中关闭 **游戏退出后打开启动器**。

![检查更新](assets/launcher-update.png)

> **首次启动可能较慢。** 官方组件需要初始化登录环境，期间请不要重复点击启动。若长时间没有反应，请查看[排障指南](troubleshooting.md)。

### 资源检查与修复

- **检查资源** 是更新页的主按钮. 它使用本地资源记录和文件大小快速核对; 没有可信记录的大文件会显示为尚未验证, 不会直接报告就绪. 检查通过后按钮显示 **检查通过**.
- **启动快速检查** 在每次打开启动器时静默执行一次, 用于尽早发现小版本更新; 检查失败不会打断启动, 可在更新页手动重试.
- **切换地区或安装目录** 后会同时刷新启动器文件与游戏资源状态; 若新目录尚未安装启动器, 两张状态卡都会提示安装.
- **安装游戏 / 更新游戏** 由官方本地资源清单 `ResList.xml` 是否存在来决定. 清单存在时视为已安装并显示 **更新游戏**, 否则显示 **安装游戏**. 点击后会先重新检查资源, 再下载需要处理的文件.
- **首次安装分为两步**: 先下载并安装启动器文件 (国服约 238 MiB, 国际服约 262 MiB 下载量), 完成后才会检查并下载游戏资源. 因此更新页在第一步显示的体积只包含启动器文件, 不是完整游戏体积.
- **完整校验** 会计算文件哈希. 发现问题后, 主按钮会保留完整校验要求并继续修复; 当前会话内, 同一地区和安装目录会保留完整校验要求, 直至修复完成. 重新打开 Oddity 后可再次完整校验.
- **暂停下载** 仅在下载或写入资源时出现, 会保留未完成文件. 退出并重新打开 Oddity 后, 对相同地区和目录再次检查资源并点击 **更新游戏** / **安装游戏** 即可继续.
- 进度条表示已处理完成的文件比例. 实际传输量单独显示; 补丁处理、校验和写入仍在进行时, 任务不会显示为完成.

当前验证范围见[游戏下载验证记录](download-verification.md).

## 更新 Oddity

使用发行版包时, 通过系统包管理器更新即可:

```bash
sudo apt update && sudo apt upgrade      # Debian / Ubuntu
sudo dnf upgrade                          # Fedora
sudo zypper update                        # openSUSE
sudo pacman -Syu                          # Arch Linux
```

AppImage 没有自动更新, 需要重新下载新版本并替换旧文件.

从源码构建时按下面的步骤更新:

```bash
cd Oddity
git pull
cd launcher
pnpm --dir ui install --frozen-lockfile
pnpm --dir ui run build
cargo build --release --locked -p nte-app
./scripts/install-desktop.sh ./target/release/oddity
```

然后退出并重新启动 Oddity。

## 卸载

删除桌面入口与图标：

```bash
rm -f ~/.local/share/applications/oddity.desktop
rm -f ~/.local/share/icons/hicolor/scalable/apps/oddity.svg
rm -f ~/.local/share/icons/hicolor/*/apps/oddity.png
```

再删除 Oddity 的配置与数据：

```bash
rm -rf ~/.config/oddity ~/.local/share/oddity
```

> 删除 `~/.local/share/oddity` 会同时删除默认 Wine prefix 和日志，登录状态可能需要重新建立。游戏本体不会受影响。

---

返回 [README](../README.md) · 遇到问题请查看[排障指南](troubleshooting.md)。
