<div align="center">

<img src="launcher/ui/public/assets/app-icon.svg" width="76" alt="Oddity 图标">

# Oddity

**《异环》的 Linux 启动器**

游戏资源的安装, 更新和启动, 都在同一个窗口里完成.

[![Release](https://img.shields.io/github/v/release/LaT-SKY/Oddity?include_prereleases&label=release)](https://github.com/LaT-SKY/Oddity/releases)
[![Platform](https://img.shields.io/badge/platform-Linux-informational)](https://github.com/LaT-SKY/Oddity)
[![License](https://img.shields.io/github/license/LaT-SKY/Oddity)](LICENSE)

</div>

<p align="center">
  <img src="docs/assets/launcher-home.png" width="1280" alt="Oddity 启动界面">
</p>

> 截图取自 Oddity 实际运行. 账号与部分游戏数据为示例. 图中官方美术素材版权归其权利人所有, 详见[美术素材声明](ASSETS.md).

Oddity 不附带游戏文件, 资源来自官方服务. 它把检查, 下载, 校验和启动放在一起, 让 Linux 上的《异环》不必在多个窗口之间来回切换.

## 能做什么

| 能力        | 你会得到什么                                  |
|-----------|-----------------------------------------|
| **安装与更新** | 检查官方资源清单, 只下载缺失或变化的文件; 下载可以暂停, 下次接着来.   |
| **两个区服**  | 国服与国际服使用独立目录, 资源来源和启动配置, 互不覆盖.          |
| **完整校验**  | 怀疑文件损坏时逐个核对哈希, 再按结果修复.                  |
| **后台启动**  | 等官方资源检查完成再自动登录并启动, 隐藏官方主窗口; 需要时也可以切到前台. |
| **国服工具**  | 塔吉多登录, 自动签到, 体力与周本查询.                   |
| **公告聚合**  | 汇总官方动态, 版本资讯与维护公告, 支持分类筛选和同源堆叠.         |
| **游玩记录**  | 统计实际游戏时长, 展示近 12 个月热力图与月度记录, 可生成分享海报.      |
| **桌面细节**  | 亮色/暗色主题, 动态/静态主视觉, 通知中心与玩家信息卡片.           |

## 开始使用

1. **安装 Oddity**. 从 [Releases](https://github.com/LaT-SKY/Oddity/releases) 选择发行版安装包, 各发行版的命令见[安装指南](docs/install.md#安装-oddity). 需要确认安装包来源时, 见[验证安装包](docs/install.md#验证安装包).
2. **准备运行环境**. 安装 `umu-launcher`, 准备 Proton. 不需要手动设置 Wine prefix; 创建 Proton 运行环境后由 Oddity 创建和管理, 见[环境准备](docs/install.md#准备游戏环境).
3. **完成首次设置**. 在 **设置 → 游戏与启动** 选择地区并填写安装目录, 在 **运行环境** 创建 Proton 环境.
4. **检查并启动**. 在 **更新** 点击 **检查资源**, 按提示安装或更新, 完成后回到 **启动**.

> 首次安装会先处理启动器文件, 再下载游戏资源. 资源体积较大, 建议留出足够空间和时间.

> 主界面无法打开或无法操作时, 从应用菜单打开 **Oddity 应急恢复**, 点击 **兼容启动** 以应急模式启动. 详见[排障指南](docs/troubleshooting.md#使用应急模式).

## 系统要求

| 项目   | 要求                                                       |
|------|----------------------------------------------------------|
| 系统   | Linux x86_64, Wayland 或 X11                              |
| 运行库  | GTK 3, WebKitGTK 4.1, GStreamer (`gst-plugins-good`)     |
| 图形   | 支持 Vulkan 的显卡驱动                                          |
| 游戏环境 | `umu-launcher`, Proton (推荐 GE-Proton); Wine prefix 由 Oddity 创建和管理 |
| 存储   | 可写的游戏安装目录, 首次安装需要完整资源                                    |

## 文档

| 文档                                            | 内容                         |
|-----------------------------------------------|----------------------------|
| [安装指南](docs/install.md)                       | 发行版安装, 游戏环境, 首次设置, 公告, 更新与卸载. |
| [排障指南](docs/troubleshooting.md)               | 启动失败, 黑屏, 下载中断, 日志导出和数据位置. |
| [外部启动入口](docs/external-launch.md)             | 应用菜单, 桌面快捷方式, Steam 和命令行启动. |
| [塔吉多指南](docs/taygedo.md)                      | 国服账号登录, 自动签到, 体力与周本.       |
| [Nix](docs/nix.md) / [Gentoo](docs/gentoo.md) | 对应发行版的安装方式.                |
| [贡献指南](CONTRIBUTING.md)                       | 代码风格, 目录结构, 检查与 PR 约定.        |
| [安全政策](SECURITY.md)                           | 私密报告漏洞的范围与方式.              |
| [第三方组件](THIRD_PARTY.md)                       | 字体, 图标, 依赖和协议参考的许可证.       |
| [美术素材声明](ASSETS.md)                           | 官方美术素材与项目自制素材的授权范围.       |

没有找到答案? 感谢您移步至 [Issues](https://github.com/LaT-SKY/Oddity/issues) 提交问题, 并附上 Oddity 版本和脱敏日志.

## 许可证

项目代码使用 [GPL-3.0-or-later](LICENSE); 第三方组件见[许可说明](THIRD_PARTY.md). 官方游戏美术素材不在 GPL 授权范围内, 授权范围见[美术素材声明](ASSETS.md).

<div align="center">
<sub>Oddity 是第三方项目, 与《异环》开发方和发行方无隶属关系. 游戏名称, 商标与官方美术素材归其权利人所有.</sub>
</div>
