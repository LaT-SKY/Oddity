# 第三方组件

Oddity 的代码使用 [GPL-3.0-or-later](LICENSE). 下列组件保留各自许可证.

## 字体与图标

| 组件                   | 用途   | 许可证                                                             |
|----------------------|------|-----------------------------------------------------------------|
| Space Grotesk        | 界面字体 | [OFL 1.1](launcher/ui/public/assets/fonts/OFL.txt)              |
| Noto Serif CJK SC 子集 | 标题字体 | [OFL 1.1](launcher/ui/public/assets/fonts/OFL-NotoSerifCJK.txt) |
| Phosphor Icons       | 图标   | [MIT](launcher/ui/public/assets/icons/LICENSE)                  |

## 依赖

前端依赖见 [package.json](launcher/ui/package.json), 锁定版本见 [pnpm-lock.yaml](launcher/ui/pnpm-lock.yaml).

Rust 依赖见 [Cargo.lock](launcher/Cargo.lock), 许可证检查配置见 [deny.toml](launcher/deny.toml).

游戏差分补丁使用 [hdiffpatch-rs](https://github.com/TwintailTeam/hdiffpatch-rs), 许可证为 MIT.

## 颜色计算

| 用途       | 来源                                                                                                              | 许可证          |
|----------|-----------------------------------------------------------------------------------------------------------------|--------------|
| Oklab 颜色空间 | [Björn Ottosson, A perceptual color space for image processing (2020)](https://bottosson.github.io/posts/oklab/) | Public domain |

主视觉取色在 `launcher/ui/src/core/palette.ts` 中实现, 只使用 Oklab 的公开转换矩阵, 不引入额外依赖.

## 图像与素材

| 素材 | 位置 | 授权 |
| --- | --- | --- |
| 项目自制背景, 应用图标与标志 | `launcher/ui/public/assets/` | [GPL-3.0-or-later](LICENSE) |
| 官方游戏美术素材 | 文档截图, 公告预览, 运行时读取的官方客户端素材 | 版权归权利人所有, 不适用 GPL |

内置背景为项目自制素材. Oddity 会读取本机官方客户端的主页素材, 文档截图与公告预览中也包含官方游戏美术素材. 这些素材仅用于识别游戏和展示界面, 不属于 GPL 授权范围.

官方素材的授权范围与移除方式见[美术素材声明](ASSETS.md).

## 塔吉多协议参考

| 用途               | 来源与参考提交                                                                                                                       | 许可证                                             |
|------------------|-------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| 登录, 签到, 金币与云异环任务 | [taygedo-auto-attendance](https://github.com/zzstar101/taygedo-auto-attendance/tree/cc8c036d0a66802a39b4ecc10f7095e38f695151) | [MIT](licenses/taygedo-auto-attendance-MIT.txt) |
| 游戏状态查询与字段解析      | [NTEUID](https://github.com/tyql688/NTEUID/tree/8fc59ff35b7fdbd1e31bf6b15ec4ba0c8d36b7cb)                                     | [GPL-3.0](licenses/NTEUID-GPL-3.0.txt)          |

## Electron 运行时

图形界面使用 Electron 44 (MIT), 其中的 Chromium、Node.js 与 FFmpeg 使用各自许可. 发行目录保留 Electron 的 `LICENSE` 与完整 `LICENSES.chromium.html`; Oddity 的 GPL-3.0-or-later 许可证位于 `LICENSE.oddity`.
