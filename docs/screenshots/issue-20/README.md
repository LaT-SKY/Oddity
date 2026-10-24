# 启动页游戏状态截图

以下截图用于 Issue #20 的 PR 展示, 均为浏览器预览的模拟账号和模拟数据, 界面中已标明来源.

| 场景 | 截图 |
|---|---|
| 默认收起的胶囊, 亮色, 1180×760 | [home-capsule-light.png](home-capsule-light.png) |
| 没有账号的展开浮层, 亮色, 1180×760 | [home-empty-light.png](home-empty-light.png) |
| 第一个账号, 亮色, 1180×760 | [home-panel-light.png](home-panel-light.png) |
| 第二个账号, 亮色, 1180×760 | [home-second-account-light.png](home-second-account-light.png) |
| 展开浮层, 暗色, 1180×760 | [home-panel-dark.png](home-panel-dark.png) |
| 展开浮层, 暗色, 最小窗口 980×640 | [home-panel-compact-dark.png](home-panel-compact-dark.png) |

浮层默认收起为胶囊, 点击胶囊、面板外部或按 Esc 可收起, 底部左右按钮切换全部已登录账号. 卡片高度随内容收紧, 仅在出现错误时增加提示区域. 面板和内容不支持横向或竖向滚动,
最小窗口下完整展示当前账号的体力、周本与其他数值, 启动按钮仍然可见. 自动刷新间隔为 30 分钟, 面板不展示更新时间或刷新间隔提示.
截图使用系统减少动态效果偏好, 避免截入过渡帧; 默认弹性动画另有浏览器帧采样验收.

在 `launcher/ui` 构建前端后, 可运行以下命令重新生成:

```sh
ODDITY_PREVIEW_PORT=4183 \
ODDITY_SCREENSHOT_DIR="$PWD/../../docs/screenshots/issue-20" \
pnpm exec playwright test tests/e2e/game-records.spec.ts
```

原生程序另已读取两个已保存账号的官方角色面板, 两个账号均成功返回并保存记录, 未报告查询错误.
本文档与截图不包含真实账号的角色信息、凭据或完整手机号.

## 本地验证 (2026-10-24)

- 基于 main `dcbc276` 验证, Rust 工作区测试 269 项通过, Clippy 和格式检查通过; Rust 行覆盖率 87.36%, 达到 80% 门槛.
- 前端单元测试 106 项通过, TypeScript、构建、格式与覆盖率门槛检查通过.
- 完整浏览器套件 8 项通过, 覆盖胶囊开合、账号切换、双向无滚动、弹簧过冲、快速中断及现有启动、设置与媒体流程.
- 新增首页流程验证全部账号切换、手动刷新、零次周本、最小窗口边界、键盘操作、圆形按钮的 SVG 居中和移除账号后的同步.
- pnpm 冻结安装和漏洞审计、cargo-deny、仓库链接、打包配置、Actions 工作流与差异空白检查通过.

Motion for Vue 2.8.0 使用声明式 `motion`、`AnimatePresence` 和 `MotionConfig`, 由库负责弹簧、退出和中断.
实测缩放从小于 0.98 进入、超过 1.001 后稳定回到 1, 连续快速开合后节点数量和可交互状态均正确.
