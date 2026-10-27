import { reactive } from "vue";

export type NotificationKind = "ok" | "error" | "info";
interface NotificationAction {
  label: string;
  run: () => void | Promise<unknown>;
}
export interface PrimaryNotificationAction extends NotificationAction {
  icon: string;
}
export interface NotificationOptions {
  key?: string;
  title?: string;
  primary?: PrimaryNotificationAction;
  secondary?: NotificationAction;
}
export interface NotificationItem extends NotificationOptions {
  id: number;
  revision: number;
  message: string;
  kind: NotificationKind;
  duration: number;
  remaining: number;
  count: number;
  busy: boolean;
  actionError: string;
  swiped?: boolean;
}
export const notifications = reactive<NotificationItem[]>([]);
let nextId = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let active: { id: number; since: number } | undefined;
const pauses = new Set<string>();

function stopClock() {
  if (timer !== undefined) clearTimeout(timer);
  timer = undefined;
  if (active) {
    const item = notifications.find((item) => item.id === active!.id);
    if (item)
      item.remaining = Math.max(
        0,
        item.remaining - (Date.now() - active.since),
      );
  }
  active = undefined;
}
function startClock() {
  const front = notifications[0];
  if (!front || pauses.size || front.busy || front.duration <= 0) return;
  active = { id: front.id, since: Date.now() };
  timer = setTimeout(() => dismissNotification(front.id), front.remaining);
}

export function notify(
  message: string,
  kind: NotificationKind = "ok",
  duration = 3400,
  options: NotificationOptions = {},
) {
  stopClock();
  const index = notifications.findIndex((item) =>
    options.key
      ? item.key === options.key
      : !item.key && item.message === message && item.kind === kind,
  );
  const previous = index >= 0 ? notifications.splice(index, 1)[0] : undefined;
  const item: NotificationItem = {
    ...options,
    id: previous?.id ?? ++nextId,
    revision: (previous?.revision ?? 0) + 1,
    message,
    kind,
    duration,
    remaining: duration,
    count: previous && previous.message === message ? previous.count + 1 : 1,
    busy: false,
    actionError: "",
  };
  notifications.unshift(item);
  startClock();
  return item.id;
}
export function dismissNotification(id: number, swiped = false) {
  stopClock();
  const index = notifications.findIndex((item) => item.id === id);
  if (index >= 0) {
    notifications[index].swiped = swiped;
    notifications.splice(index, 1);
  }
  startClock();
}
export function pauseNotifications(reason: string, paused: boolean) {
  stopClock();
  if (paused) pauses.add(reason);
  else pauses.delete(reason);
  startClock();
}
export function clearNotifications() {
  stopClock();
  notifications.splice(0);
}
export function resetNotifications() {
  clearNotifications();
  pauses.clear();
}
export async function runNotificationAction(
  id: number,
  which: "primary" | "secondary",
) {
  const item = notifications.find((item) => item.id === id);
  const action = item?.[which];
  if (!item || !action || item.busy) return;
  stopClock();
  const revision = item.revision;
  item.busy = true;
  item.actionError = "";
  try {
    await action.run();
    if (notifications.includes(item) && item.revision === revision)
      dismissNotification(id);
  } catch (error) {
    if (notifications.includes(item) && item.revision === revision)
      item.actionError = error instanceof Error ? error.message : String(error);
  } finally {
    item.busy = false;
    stopClock();
    startClock();
  }
}
