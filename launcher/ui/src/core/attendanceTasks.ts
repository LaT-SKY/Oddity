import type { AttendanceAccount, AttendanceTasks } from "../types/launcher";

export const defaultAttendanceTasks: AttendanceTasks = {
  communitySign: true,
  roleSign: true,
  browsePosts: true,
  likePosts: true,
  sharePosts: true,
  cloudDuration: true,
};
export const attendanceTaskOptions: {
  key: keyof AttendanceTasks;
  label: string;
  description: string;
}[] = [
  {
    key: "communitySign",
    label: "塔吉多社区签到",
    description: "完成社区签到与 BBS 金币签到",
  },
  {
    key: "roleSign",
    label: "NTE 角色签到",
    description: "为账号下的异环角色签到",
  },
  {
    key: "browsePosts",
    label: "浏览社区帖子",
    description: "浏览推荐帖子以完成金币任务",
  },
  {
    key: "likePosts",
    label: "点赞社区帖子",
    description: "为推荐帖子点赞; 必要时读取帖子详情以确认点赞状态",
  },
  {
    key: "sharePosts",
    label: "分享社区帖子",
    description: "调用社区分享接口以完成金币任务",
  },
  {
    key: "cloudDuration",
    label: "领取云异环时长",
    description: "领取每日赠送时长",
  },
];
export function attendanceTasksComplete(
  account: AttendanceAccount,
  tasks: AttendanceTasks,
): boolean {
  const steps = account.lastReport?.steps ?? [];
  return attendanceTaskOptions.every(({ key }) => {
    if (!tasks[key]) return true;
    const labels =
      key === "communitySign"
        ? ["社区签到", "BBS 金币签到"]
        : key === "browsePosts"
          ? ["浏览帖子"]
          : key === "likePosts"
            ? ["点赞帖子"]
            : key === "sharePosts"
              ? ["分享帖子"]
              : key === "cloudDuration"
                ? ["云异环时长"]
                : [];
    return (
      account.signedToday &&
      (key === "roleSign"
        ? steps.some((s) => s.label.startsWith("角色 ") && s.ok) &&
          !steps.some((s) => !s.optional && !s.ok)
        : labels.every((label) => steps.some((s) => s.label === label && s.ok)))
    );
  });
}
