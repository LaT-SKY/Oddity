# 第三方组件

Oddity 的源代码以 [GPL-3.0-or-later](LICENSE) 授权。下列组件保留各自的许可证，不属于该授权范围。

## 字体

| 组件 | 位置 | 许可证 |
|---|---|---|
| Space Grotesk | `launcher/ui/public/assets/fonts/SpaceGrotesk.ttf` | [SIL Open Font License 1.1](launcher/ui/public/assets/fonts/OFL.txt) |
| Noto Serif CJK 子集（重命名为 NTE Display） | `launcher/ui/public/assets/fonts/NTESerifSC-Black-subset.woff2` | [SIL Open Font License 1.1](launcher/ui/public/assets/fonts/OFL-NotoSerifCJK.txt) |

字体子集已按 OFL 要求重命名。

## 图标

| 组件 | 位置 | 许可证 |
|---|---|---|
| Phosphor Icons | `launcher/ui/src/components/icons/IconSprite.vue` | [MIT](launcher/ui/public/assets/icons/LICENSE) |

## 前端依赖

运行时使用 Vue 3 (MIT)、[Motion](https://github.com/motiondivision/motion) / [Motion for Vue](https://github.com/motiondivision/motion-vue) (MIT)、
[VueUse](https://github.com/vueuse/vueuse) (MIT) 与 `@tauri-apps/api` (Apache-2.0 OR MIT)。
启动页胶囊与游戏状态浮层使用 `motion-v` 2.8.0 的声明式 `motion` 组件、`AnimatePresence` 和 `MotionConfig`,
由 Motion 实现弹簧动画、退出和中断. `@vueuse/core` 14.4.0 为其对等依赖;
现有 `motion-dom` 与该版本共用 14.3.1. 库类型声明修正见 [patches/README.md](launcher/ui/patches/README.md).
直接依赖和开发工具见 [`launcher/ui/package.json`](launcher/ui/package.json), 完整版本与来源由 [`launcher/ui/pnpm-lock.yaml`](launcher/ui/pnpm-lock.yaml) 锁定。各组件保留自身许可证, pnpm 漏洞审计在 CI 中执行。

## Rust 依赖

依赖的许可证、版本与来源由 `cargo deny check` 在 CI 中校验，允许的许可证列表见 [`launcher/deny.toml`](launcher/deny.toml)。
游戏差分补丁使用固定提交的 [hdiffpatch-rs](https://github.com/TwintailTeam/hdiffpatch-rs)（MIT）；具体提交见 [`launcher/Cargo.lock`](launcher/Cargo.lock)。

## 美术与商标

- 主页主视觉（`launcher/ui/public/assets/hero-scene.svg` 及其导出的 jpg、mp4、webm）、应用图标、托盘图标与标志由本项目创作，随源代码以 GPL-3.0-or-later 授权。
- 《异环》/ Neverness To Everness 的名称、商标与游戏资源归其权利人所有。本仓库不包含游戏的美术、音频或视频素材。

## 塔吉多登录与签到

`launcher/crates/nte-core/src/attendance/` 的老虎账号密码登录、AES 表单字段编码、请求签名、用户中心会话、签到、
金币任务及云异环时长协议参考并改写自 [zzstar101/taygedo-auto-attendance](https://github.com/zzstar101/taygedo-auto-attendance),
参考版本 `cc8c036d0a66802a39b4ecc10f7095e38f695151`.
上游为 MIT 许可, 许可全文见 [taygedo-auto-attendance-MIT.txt](licenses/taygedo-auto-attendance-MIT.txt).

## 塔吉多游戏状态

`launcher/crates/nte-core/src/attendance/game_record.rs` 及 `client.rs` 的角色面板查询参考了
[tyql688/NTEUID](https://github.com/tyql688/NTEUID), 固定参考提交
`8fc59ff35b7fdbd1e31bf6b15ec4ba0c8d36b7cb` 中的 `NTEUID/utils/sdk/tajiduo.py` 和 `tajiduo_model.py`.
使用范围为官方 `roleHome` 接口路径、角色标识、体力、都市活力、活跃度与周本剩余次数字段的协议整理,
由本项目以 Rust 实现查询和解析. 上游为 GPL-3.0 许可, 许可全文见 [NTEUID-GPL-3.0.txt](licenses/NTEUID-GPL-3.0.txt).

调研时另核对了 [TaJiDuo-plugin](https://github.com/Entropy-Increase-Team/TaJiDuo-plugin/tree/4ea4c260dd6e59a1811961bbdbc13bb315a56fba)
的 `apps/yihuan/realtime.js` 与接口说明,
[NTE-Maid](https://github.com/leck995/NTE-Maid/tree/44a015858700136131e5669a35732874f651a5ee)
的 `TaygedoApi.java` 和 `RoleHome.java`, 以及
[AnimeGamesBar](https://github.com/MurasameGenos/AnimeGamesBar/tree/825610e790ab5f4d8c037904155409fb35adb655)
的 `TajiduoClient.cs`. 这些仓库仅用于交叉核对接口, 未复制其代码或素材.
Oddity 直接连接官方塔吉多服务, 不依赖第三方查询平台.
