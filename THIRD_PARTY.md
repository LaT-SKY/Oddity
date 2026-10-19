# 第三方组件

Oddity 的源代码以 [GPL-3.0-or-later](LICENSE) 授权。下列组件保留各自的许可证，不属于该授权范围。

## 字体

| 组件 | 位置 | 许可证 |
|---|---|---|
| Space Grotesk | `launcher/ui/assets/fonts/SpaceGrotesk.ttf` | [SIL Open Font License 1.1](launcher/ui/assets/fonts/OFL.txt) |
| Noto Serif CJK 子集（重命名为 NTE Display） | `launcher/ui/assets/fonts/NTESerifSC-Black-subset.woff2` | [SIL Open Font License 1.1](launcher/ui/assets/fonts/OFL-NotoSerifCJK.txt) |

字体子集已按 OFL 要求重命名。

## 图标

| 组件 | 位置 | 许可证 |
|---|---|---|
| Phosphor Icons | `launcher/ui/assets/icons.svg` | [MIT](launcher/ui/assets/icons/LICENSE) |

## Rust 依赖

依赖的许可证、版本与来源由 `cargo deny check` 在 CI 中校验，允许的许可证列表见 [`launcher/deny.toml`](launcher/deny.toml)。

## 美术与商标

- 主页主视觉（`launcher/ui/assets/hero-scene.svg` 及其导出的 jpg、mp4、webm）、应用图标、托盘图标与标志由本项目创作，随源代码以 GPL-3.0-or-later 授权。
- 《异环》/ Neverness To Everness 的名称、商标与游戏资源归其权利人所有。本仓库不包含游戏的美术、音频或视频素材。
