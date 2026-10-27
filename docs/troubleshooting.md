# 排障指南

先在 **设置** 页底部查看 **运行环境**, 点击顶部的 **重新检测** 可以刷新检测结果. 下面按现象分类.

| 现象                       | 先检查         |
|--------------------------|-------------|
| 提示找不到游戏或启动器配置            | 地区与安装目录     |
| 提示找不到 `umu-run` 或 Proton | 路径与 `PATH`  |
| 提示 Wine prefix 尚未初始化     | `drive_c`   |
| 窗口黑屏, 花屏或闪退              | 软件渲染开关与显卡驱动 |
| 游戏启动失败或卡住                | 日志, 权限与启动方式 |
| 下载中断或校验失败                | 磁盘空间, 网络与续传 |

## 三项快速检查

在终端执行:

```bash
command -v umu-run
ls /path/to/Proton/proton /path/to/Proton/toolmanifest.vdf
ls -d ~/.local/share/oddity/environments/环境ID/prefix/drive_c
```

请将示例中的 Proton 路径和 prefix 路径替换为实际路径. 三项都存在时, 再往下看具体现象.

## 启动器

### 找不到启动器配置

已有安装请选择安装根目录或启动器目录, 并确认对应配置存在:

- 国服: `<安装根目录>/NTELauncher/Config/Config.ini`.
- 国际服: `<安装根目录>/NTEGlobal/Config/Config.ini`.

新安装先在 **更新** 页点击 **检查资源**, 再按提示安装.

如果目录里有游戏文件但 Oddity 仍识别不到, 确认设置中的 **游戏地区** 与目录一致, 不要在一个目录里混放两个区服.

### 找不到 umu-run 或 Proton

若终端能找到 `umu-run` 而 Oddity 找不到, 请检查启动 Oddity 时的 `PATH`.

创建运行环境时应选择 Proton 安装目录, 不要选择 `proton` 文件. 该目录需要同时包含 `proton` 和 `toolmanifest.vdf`.

### Wine prefix 尚未初始化

请先按[安装指南](install.md#wine-prefix)在设置中创建运行环境. 首次启动时由 Proton 初始化前缀. 若环境目录损坏或被移除, 请创建新环境后切换使用.

### 窗口黑屏, 花屏或闪退

主界面无法操作时, 从应用菜单打开 **Oddity 应急恢复**, 尝试 **兼容启动**; 仍有问题可导出诊断包或恢复设置. 恢复前退出主程序, 原设置会备份.

也可运行 `oddity-recovery`; AppImage 加上 `--recovery`.

可以从终端尝试软件渲染开关:

```bash
NTE_SOFTWARE_RENDER=1 oddity
```

将 `oddity` 换成实际可执行文件路径. 若两种方式结果不同, 提交问题时附上显卡型号, 驱动版本和 Wayland/X11 会话类型.

### 主页视频无法播放或窗口启动后冻结

首页视频需要 GStreamer 的 `autodetect` 插件. 缺少 `gst-plugins-good` 时, WebKit 可能在建立视频管线后退出, 表现为窗口冻结在最后一帧或直接消失, 主进程与托盘仍在.

先确认插件可用:

```bash
gst-inspect-1.0 autoaudiosink
```

如果提示找不到元素或命令, 按发行版安装:

```bash
# Arch
sudo pacman -S --needed gst-plugins-good
# Debian / Ubuntu
sudo apt install gstreamer1.0-plugins-good gstreamer1.0-tools
# Fedora
sudo dnf install gstreamer1-plugins-good
# openSUSE
sudo zypper install gstreamer-plugins-good
```

如果只是视频无法播放, 界面仍可操作, 也可以在 **设置 → 主页主视觉** 中选择 **静态照片** 临时规避.

Debian, Arch 和 Gentoo 安装包会声明该依赖; AppImage, RPM 和源码构建不保证自带该插件, 需要结合系统环境确认. Nix 包已包含插件.

### 关闭窗口后 Oddity 没有退出

如果开启了 **关闭窗口时最小化到托盘**, 关闭主窗口只会隐藏到后台. 可以从托盘菜单选择 **退出 Oddity**, 或在设置中关闭该选项.

### 桌面入口打开后提示无法连接 127.0.0.1

这通常出现在从源码运行后直接安装桌面入口的情况. 发布版会内嵌前端文件, 开发模式则依赖 Vite 服务. 处理方式见[安装指南](install.md#从源码构建).

## 游戏

### 游戏启动失败或卡住

1. 查看启动日志和辅助日志的末尾, 详细位置见[数据位置](#数据位置).
2. 若日志提示运行配置不可写, 优先检查游戏目录中 `GameLauncher.config` 和 `patchersdk_serverlist` 的权限.
3. 若日志提示找不到可用服务器, 优先检查游戏目录的权限.
4. 若后台启动失败或提示组件版本尚未验证, 切换 **前台启动** 查看官方窗口.
5. 若怀疑文件损坏, 在 **更新** 页执行 **完整校验**, 再修复资源.

### 下载中断或校验失败

重新打开 Oddity 后, 对同一地区和目录重新点击 **检查资源**, 再继续安装或更新, 已经下载的文件会尽量复用.

若反复中断, 检查磁盘剩余空间和网络连接, 或先暂停下载, 等网络稳定后继续. 完整校验失败时, 按界面提示修复对应资源.

### 游戏退出后没有回到 Oddity

确认设置中的 **游戏退出后打开启动器** 已开启. 若关闭了该选项, Oddity 会留在托盘, 可以从托盘菜单打开主界面.

## 日志与数据

### 导出日志

在 **设置 → 日志 → 导出日志** 中选择保存目录, Oddity 会把启动及后台会话日志打包为 ZIP.

提交日志前请移除账号, 手机号, 令牌和不希望公开的本机路径.

### 数据位置

以下为未设置 XDG 目录变量时的路径:

| 内容          | 默认位置                                            |
|-------------|-------------------------------------------------|
| 设置          | `~/.config/oddity/settings.json`                |
| 签到状态        | `~/.config/oddity/taygedo/state.json`           |
| 日志目录        | `~/.local/share/oddity/logs/`                   |
| 启动日志        | `~/.local/share/oddity/logs/environments/<环境 ID>/nte-app-launch.log` |
| 辅助日志        | `~/.local/share/oddity/logs/environments/<环境 ID>/helper-window.log`  |
| Wine prefix | `~/.local/share/oddity/environments/<环境 ID>/prefix`                  |

配置和数据目录分别跟随 `XDG_CONFIG_HOME`, `XDG_DATA_HOME`.

Wine prefix 由所选运行环境决定. 设置页会显示当前 prefix 的实际位置.

### 命令行诊断

安装包提供 `nte-cli`, 可以在不打开界面的情况下检查配置和资源:

| 命令                                            | 用途               |
|-----------------------------------------------|------------------|
| `nte-cli recovery diagnose`                    | 检查 Oddity 环境, 无需游戏目录. |
| `nte-cli doctor --root <启动器目录>`               | 显示启动器配置和运行环境.    |
| `nte-cli game-check --root <启动器目录> --full`    | 完整检查游戏资源.        |
| `nte-cli game-apply --root <启动器目录> --dry-run` | 预览更新计划, 不写入游戏文件. |

示例中的 `<启动器目录>` 请替换为实际路径. 以上命令只读取配置或预览更新, 不会写入游戏文件.

## 提交问题

请在 [Issues](https://github.com/LaT-SKY/Oddity/issues) 提供:

- Oddity 版本.
- 发行版和桌面环境.
- 显卡型号与驱动版本.
- Wayland 或 X11 会话类型.
- 复现步骤与脱敏日志.

安全漏洞请按[安全政策](../SECURITY.md)私下报告.
