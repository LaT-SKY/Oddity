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
| **官方启动器桥接目录** | 一般保持默认。仅当官方启动器副本位于其他位置时才需要修改 |
| **Wine prefix** | 游戏使用的 Wine 前缀，目录中应包含 `drive_c`。留空时使用默认目录 |
| **Proton 路径** | Proton 安装目录，目录中应同时包含 `proton` 和 `toolmanifest.vdf` |
| **启动方式** | 日常使用请选择 **官方后台桥接** |

![设置页](assets/launcher-settings.png)

填写后点击 **保存设置**。设置页顶部的检测项会标出缺失的组件；全部通过后再回到主页。

### 启动方式说明

| 方式 | 适合场景 | 说明 |
|---|---|---|
| **官方后台桥接**（推荐） | 日常游玩 | 自动登录，隐藏官方启动器窗口 |
| **官方启动器桥接** | 需要查看官方启动器窗口时 | 显示官方窗口，同样支持自动登录 |
| **研究直启** | 仅调试 | 不经过启动器登录链路，不能用于正常游玩 |

## 第一次启动

1. 前往 **更新** 页，选择地区与安装目录，点击 **安装或更新**。Oddity 会获取官方启动器文件和游戏资源。
2. 更新完成后回到 **启动** 页，点击 **启动游戏**。Oddity 会通过 Proton 启动官方后台组件，完成登录与后续启动流程。
3. 启动成功后，Oddity 窗口会收起到系统托盘，游戏开始运行。

![检查更新](assets/launcher-update.png)

> **首次启动可能较慢。** 官方组件需要初始化登录环境，期间请不要重复点击启动。若长时间没有反应，请查看[排障指南](troubleshooting.md)。

### 资源检查与修复

- **检查资源** 使用本地资源记录和文件大小快速核对. 没有可信记录的大文件会显示为尚未验证, 不会直接报告就绪.
- **完整校验** 会计算文件哈希. 发现问题后, 点击 **完整校验并修复**; 当前会话内, 同一地区和安装目录会保留完整校验要求, 直至修复完成. 重新打开 Oddity 后可再次完整校验.
- **安装或更新** 遇到尚未验证的资源时, 会先完整核验, 再下载需要修复的文件. 已核验正确的文件会记录到本地资源账本.
- **暂停下载** 会保留未完成文件. 退出并重新打开 Oddity 后, 对相同地区和目录再次执行安装或更新即可继续.
- 进度条表示已处理完成的文件比例. 实际传输量单独显示; 补丁处理、校验和写入仍在进行时, 任务不会显示为完成.

当前验证范围见[游戏下载验证记录](download-verification.md).

## 更新 Oddity

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

> 删除 `~/.local/share/oddity` 会同时删除默认 Wine prefix、桥接副本和日志，登录状态可能需要重新建立。游戏本体不会受影响。

---

返回 [README](../README.md) · 遇到问题请查看[排障指南](troubleshooting.md)。
