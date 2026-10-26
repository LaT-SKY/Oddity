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

## 图像与素材

内置背景为项目自制素材. 运行时可能读取本地官方启动器的主页素材.

游戏名称, 商标和官方素材的权利归各自权利人.

## 塔吉多协议参考

| 用途               | 来源与参考提交                                                                                                                       | 许可证                                             |
|------------------|-------------------------------------------------------------------------------------------------------------------------------|-------------------------------------------------|
| 登录, 签到, 金币与云异环任务 | [taygedo-auto-attendance](https://github.com/zzstar101/taygedo-auto-attendance/tree/cc8c036d0a66802a39b4ecc10f7095e38f695151) | [MIT](licenses/taygedo-auto-attendance-MIT.txt) |
| 游戏状态查询与字段解析      | [NTEUID](https://github.com/tyql688/NTEUID/tree/8fc59ff35b7fdbd1e31bf6b15ec4ba0c8d36b7cb)                                     | [GPL-3.0](licenses/NTEUID-GPL-3.0.txt)          |
