# 外部启动入口

Oddity 提供不需要先打开主界面的启动命令, 供应用菜单, 桌面快捷方式, Steam 游戏库或其他前端调用.

## 命令

```bash
oddity --launch
oddity --launch --region cn
oddity --launch --region global
oddity --launch --region global --environment "GE-Proton 9"
oddity --launch --region cn --launch-mode official
oddity --launch --help
```

`--launch --help` 只打印用法, 不会启动游戏.

| 参数 | 说明 |
| --- | --- |
| `--launch` | 启动游戏, 必填. |
| `--region cn\|global` | 指定国服或国际服. 省略时使用当前设置中的地区. |
| `--launch-mode official\|background\|direct` | 指定本次启动方式. 省略时固定使用 `background`. |
| `--environment <名称或 ID>` | 指定运行环境. 省略时自动选择当前地区可用的环境. |

规则:

- `--region` 和 `--environment` 会写回设置, 下次打开 Oddity 时仍是该地区和运行环境.
- `--launch-mode` 只对本次启动生效, 不修改设置中的启动方式.
- 只给 `--region` 时, 优先使用当前环境; 其次使用稳定环境; 再次使用该地区唯一的匹配环境. 有多个候选时会报错并提示使用 `--environment`.
- 同时给出 `--region` 和 `--environment` 时, 两者必须属于同一地区.
- 参数错误会在启动 Oddity 主程序前结束, 退出码为 2.

## 应用菜单

安装包会在 `oddity.desktop` 中注册两个动作:

| 动作 | 命令 |
| --- | --- |
| 异环 | `oddity --launch --region cn --launch-mode background` |
| NTE | `oddity --launch --region global --launch-mode background` |

动作位于 Oddity 图标的右键菜单或应用菜单的动作菜单中. 主入口 Oddity 仍用于打开启动器界面.

## Steam

1. 在 Steam 中添加非 Steam 游戏, 目标选择 `oddity`.
2. 在启动选项里填入 `--launch --region cn --launch-mode background`.
3. 国际服使用 `--launch --region global --launch-mode background`.

参数要放在启动选项里, 不要直接拼在可执行文件路径后面. Steam 的快捷方式名称可以写成 `异环` 或 `NTE`.

## 启动结果与错误

冷启动且启动成功时, Oddity 保持隐藏并常驻托盘; 游戏退出后按设置决定是否显示主界面.

启动失败时:

- Oddity 通过桌面通知显示 `异环启动失败` 或 `NTE 启动失败`, 正文是具体原因.
- 通知服务不可用时退回主界面提示, 并写入启动日志.
- 冷启动失败且通知发送成功时, 进程以退出码 1 结束, 不会留下空的托盘图标.
- 已经运行的 Oddity 收到转发请求时不会退出, 只显示通知.

转发请求的退出码固定为 0, 只表示参数已被主实例接收, 不表示游戏已经成功启动. 需要可靠结果时请查看桌面通知或启动日志.

## 已知限制

- 已运行 Oddity 时, `--launch` 会把请求转发给现有实例, 游戏不是 Steam 的子进程. Steam Overlay 和 Steam 运行时长在这条路径上不保证可用.
- 冷启动时 Oddity 作为 Steam 的子进程常驻, Steam 可能在游戏退出后仍认为快捷方式正在运行.
- Steam 的停止按钮不保证结束游戏进程组, 建议从游戏内正常退出.
- 没有 D-Bus 会话总线时, 单实例转发不可用, 可能启动第二个 Oddity.
- 更新 Oddity 后如果旧实例仍在运行, 新的桌面动作会转发给旧实例. 旧版本不认识 `--launch`, 只会显示主界面; 更新后请重启 Oddity.

## 日志

启动日志位于:

```text
~/.local/share/oddity/logs/
```

详细排查见[排障指南](troubleshooting.md).
