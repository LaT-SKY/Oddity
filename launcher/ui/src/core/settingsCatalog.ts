import { reactive } from "vue";

export const settingsCategories = [
  { id: "appearance", label: "外观与交互", icon: "i-monitor" },
  { id: "game", label: "游戏与启动", icon: "i-game-controller" },
  { id: "runtime", label: "运行环境", icon: "i-hard-drives" },
  { id: "account", label: "账号与签到", icon: "i-users" },
  { id: "maintenance", label: "诊断与维护", icon: "i-shield-check" },
] as const;
export type SettingsCategory = (typeof settingsCategories)[number]["id"];
export const settingsNavigation = reactive<{
  category: SettingsCategory;
  query: string;
}>({ category: "appearance", query: "" });

export const settingsEntries = [
  {
    id: "theme",
    category: "appearance",
    label: "主题",
    description: "选择启动器的亮色或暗色外观.",
    keywords: "亮色 暗色 深色 浅色 颜色",
  },
  {
    id: "media",
    category: "appearance",
    label: "主页主视觉",
    description: "选择动态视频或静态照片.",
    keywords: "壁纸 背景 视频 照片 封面",
  },
  {
    id: "bulletins",
    category: "appearance",
    label: "堆叠公告",
    description: "将同一来源的公告合并为卡叠, 点击展开.",
    keywords: "公告 卡片 堆叠 列表 显示",
  },
  {
    id: "motion",
    category: "appearance",
    label: "减少动效",
    description: "关闭入场与页面过渡, 保留状态反馈.",
    keywords: "动画 运动 无障碍",
  },
  {
    id: "tray",
    category: "appearance",
    label: "关闭窗口时最小化到托盘",
    description: "关闭主窗口后保持后台运行.",
    keywords: "窗口 关闭 后台 最小化 托盘",
  },
  {
    id: "return",
    category: "appearance",
    label: "游戏退出后打开启动器",
    description: "游戏进程结束后显示 Oddity 主界面.",
    keywords: "窗口 返回 退出 启动器",
  },
  {
    id: "distribution",
    category: "game",
    label: "游戏地区",
    description: "选择国服或国际服.",
    keywords: "区服 国际服 国服 NTE 异环",
  },
  {
    id: "install",
    category: "game",
    label: "国服安装目录",
    description: "指定国服游戏的安装位置.",
    keywords: "路径 文件夹 磁盘 位置",
  },
  {
    id: "global-install",
    category: "game",
    label: "国际服安装目录",
    description: "指定国际服游戏的安装位置.",
    keywords: "路径 文件夹 磁盘 位置 NTE",
  },
  {
    id: "launch",
    category: "game",
    label: "启动方式",
    description: "选择后台启动、前台启动或直接启动.",
    keywords: "官方 登录 后台 前台 开发",
  },
  {
    id: "runtime",
    category: "runtime",
    label: "Proton 运行环境",
    description: "创建、切换、复制或删除独立的运行环境.",
    keywords: "Proton Wine prefix 副本 稳定 环境名称",
  },
  {
    id: "account",
    category: "account",
    label: "塔吉多签到",
    description: "管理账号、自动签到、签到时机与每日时间.",
    keywords: "账号 登录 密码 验证码 签到 账号管理 社区",
  },
  {
    id: "logs",
    category: "maintenance",
    label: "导出日志",
    description: "导出 Oddity 启动与后台会话日志.",
    keywords: "日志 ZIP 导出 排查",
  },
  {
    id: "diagnostics",
    category: "maintenance",
    label: "运行诊断",
    description: "检查启动器配置、游戏文件与运行依赖.",
    keywords: "检测 检查 umu prefix Proton 故障 配置",
  },
] as const satisfies readonly {
  id: string;
  category: SettingsCategory;
  label: string;
  description: string;
  keywords: string;
}[];
export type SettingsEntry = (typeof settingsEntries)[number];
