# 排障指南

先检查设置页顶部的检测项：它们会直接指出缺失的组件。大部分启动问题都能在这里定位。

## 快速检查

```bash
# umu-launcher 是否可用
command -v umu-run

# Proton 目录是否正确
ls /path/to/GE-Proton/proton /path/to/GE-Proton/toolmanifest.vdf

# 游戏目录是否正确
ls "$HOME/Games/Neverness To Everness/NTELauncher/Config/Config.ini"
```

## 常见问题

### 设置页提示“未找到启动器配置”

确认选择的目录正确。Oddity 同时接受安装根目录和 `NTELauncher` 目录，但该目录下必须存在：

```text
Config/Config.ini
```

### 提示“PATH 中未找到 umu-run”

`umu-launcher` 没有安装，或它不在 Oddity 启动时的 `PATH` 中。安装后从终端运行一次 Oddity，确认：

```bash
command -v umu-run
```

### 提示“未找到可用的 Proton”

Proton 路径应当指向 **安装目录**，而不是 `proton` 可执行文件。该目录中必须同时有：

```text
proton
toolmanifest.vdf
```

也可以把 GE-Proton 放到 `~/.local/share/Steam/compatibilitytools.d/`，让 Oddity 自动发现。

### 提示“Wine prefix 尚未初始化”

指定的前缀中没有 `drive_c`。请选择已经由 Proton 初始化过的游戏前缀，或先用 Proton 运行一次游戏/安装程序。

### 点击启动后立刻失败

先打开 `~/.local/share/oddity/logs/nte-app-launch.log`，拉到文件末尾。常见原因：

- `umu-run` 或 Proton 路径错误；
- Wine prefix 不可写；
- 游戏目录被移动或删除；
- 显卡驱动或 Vulkan 环境异常。

修复后重新启动 Oddity。

### 卡在启动画面或没有出现登录

- 首次启动的初始化可能需要数分钟，请先等待。
- 确认 **官方启动器桥接目录** 没有被删除或移动。
- 检查 `nte-app-launch.log` 与 `helper-window.log` 的最后几十行。
- 如果使用“研究直启”，请切回 **官方后台桥接**。

### 桌面启动时提示无法连接 127.0.0.1

桌面入口应指向内嵌前端资源的独立构建, 无需运行 Vite。请按[安装指南](install.md#更新-oddity)重新构建 Release 程序并更新桌面入口。

前端热更新开发使用 `tauri dev`, 它会自动启动 Vite。不要将开发模式生成的可执行文件安装为桌面入口。

### NVIDIA + Wayland 下界面黑屏、花屏或闪退

Oddity 默认开启硬件加速，并会自动为 NVIDIA Wayland 应用兼容设置。如果仍有问题，在 Oddity 所在目录用软件渲染启动：

```bash
NTE_SOFTWARE_RENDER=1 ./target/release/oddity
```

如果软件渲染正常，请附上显卡型号、驱动版本和桌面环境提交问题。

### 托盘图标不显示

重新执行一次桌面入口安装，然后注销并重新登录桌面：

```bash
cd Oddity/launcher
./scripts/install-desktop.sh ./target/release/oddity
```

KDE 与 GNOME 需要桌面入口才能正确显示任务栏图标。

## 日志与诊断

| 内容 | 路径 |
|---|---|
| 设置 | `~/.config/oddity/settings.json` |
| 启动日志 | `~/.local/share/oddity/logs/nte-app-launch.log` |
| 辅助日志 | `~/.local/share/oddity/logs/helper-window.log` |
| Wine prefix | 默认 `~/.local/share/oddity/prefix` |
| 官方启动器桥接副本 | `~/.local/share/oddity/winroot` |

提交问题前，建议先查看日志的最后几十行。请隐去账号、UID、手机号等个人信息。

### 命令行检查

仓库同时提供无界面的 `nte-cli`，适合在终端里定位问题：

```bash
cd Oddity/launcher
cargo build --release -p nte-cli

# 查看路径、版本与识别到的组件
./target/release/nte-cli doctor --root "$HOME/Games/Neverness To Everness/NTELauncher"

# 快速校验启动器文件
./target/release/nte-cli check --root "$HOME/Games/Neverness To Everness/NTELauncher" --quick
```

### 游戏资源检查

以下命令只检查或预演，不会修改游戏文件：

```bash
./target/release/nte-cli game-check --root "$HOME/Games/Neverness To Everness/NTELauncher"
./target/release/nte-cli game-apply --root "$HOME/Games/Neverness To Everness/NTELauncher" --dry-run
```

应用资源更新时会使用内置 HDiff 实现（HDIFF13、HDIFFSF20、HDIFFW26），无需安装外部 `hpatchz`：

```bash
./target/release/nte-cli game-apply --root "$HOME/Games/Neverness To Everness/NTELauncher"
```

不确定时，先用 `--dry-run` 查看将要下载的文件。

## 提交问题

请附上：

- Oddity 版本（设置页底部可以看到）；
- 发行版与桌面环境（Wayland / X11）；
- 显卡型号与驱动版本；
- 复现步骤与完整报错；
- 相关日志的最后几十行（请先隐去个人信息）。

[提交 Issue](https://github.com/LaT-SKY/Oddity/issues)

---

返回 [README](../README.md) · 安装步骤见[安装指南](install.md)。
