export type AttendanceTrigger = "off" | "launch" | "boot" | "daily" | "manual";
export interface AttendancePreferences {
  trigger: AttendanceTrigger;
  time: string;
}
export interface AttendanceFailure {
  kind:
    | "network"
    | "auth"
    | "verification"
    | "keyring"
    | "storage"
    | "busy"
    | "protocol"
    | "service";
  message: string;
}
export interface AttendanceReport {
  at: number;
  ok: boolean;
  steps: {
    label: string;
    ok: boolean;
    message: string;
    kind: AttendanceFailure["kind"] | null;
    optional?: boolean;
  }[];
}
export interface AttendanceAccount {
  id: string;
  maskedPhone: string;
  rememberPassword: boolean;
  signedToday: boolean;
  failedToday: boolean;
  lastReport: AttendanceReport | null;
  attention: AttendanceFailure | null;
}
export interface AttendanceStatus {
  accounts: AttendanceAccount[];
  preferences: AttendancePreferences;
}
export interface Settings {
  installRoot: string | null;
  globalInstallRoot: string | null;
  distribution: "cn" | "global";
  bridgeRoot: string | null;
  prefix: string | null;
  protonPath: string | null;
  launchMode: string;
  language: string;
  reduceMotion: boolean;
  theme: string;
  heroMedia: string;
  minimizeToTrayOnClose: boolean;
}
export interface CheckItem {
  label: string;
  ok: boolean;
  detail: string;
}
export interface Bootstrap {
  ok: boolean;
  error: string | null;
  installRoot: string;
  launcherRoot: string;
  bridgeRoot: string;
  bridgeReady: boolean;
  prefix: string;
  prefixExists: boolean;
  protonName: string;
  protonPath: string;
  umuAvailable: boolean;
  launcherVersion: string;
  launcherBuild: string;
  gameVersion: string;
  gameExe: string;
  gameExists: boolean;
  sdkPath: string;
  checks: CheckItem[];
  settings: Settings;
  appVersion: string;
  os: string;
}
export interface CheckResult {
  remoteVersion: string;
  localVersion: string;
  productVersion: string;
  files: number;
  ok: number;
  missing: number;
  changed: number;
  bytes: number;
  durationMs: number;
  missingPreview: string[];
  changedPreview: string[];
}
export interface LaunchResult {
  mode: string;
  pid: number;
  exe: string;
  prefix: string;
  log: string;
  startedAt: number;
}
export interface GameStatus {
  running: boolean;
  closing: boolean;
  pid: number;
  startedAt: number;
  exe: string;
  log: string;
}
export type DownloadPhase =
  | "idle"
  | "checking"
  | "needs-install"
  | "needs-update"
  | "needs-verify"
  | "launcher"
  | "game"
  | "ready"
  | "cancelled"
  | "failed";
export interface DownloadStatus {
  phase: DownloadPhase;
  distribution: "cn" | "global";
  installRoot: string;
  full: boolean;
  unverifiedFiles: number;
  currentFile: string;
  currentFileBytes: number;
  completedFiles: number;
  totalFiles: number;
  downloadedBytes: number;
  totalBytes: number;
  patchFiles: number;
  patchBytes: number;
  fullFiles: number;
  fullBytes: number;
  stage: string;
  remoteVersion: string;
  pendingFiles: number;
  error: string;
}
export type BulletinKind = "bilibili" | "preview" | "maintenance";
export interface BulletinItem {
  kind: BulletinKind;
  title: string;
  summary: string;
  url: string;
  imageUrl: string | null;
  publishedAt: string;
  source: string;
}
export interface BulletinFeed {
  items: BulletinItem[];
  errors: string[];
}
export interface Commands {
  attendance_status: AttendanceStatus;
  attendance_login: AttendanceStatus;
  attendance_send_code: string;
  attendance_login_sms: AttendanceStatus;
  attendance_configure: AttendanceStatus;
  attendance_run: AttendanceStatus;
  attendance_logout: AttendanceStatus;
  attendance_keyring_check: void;
  play_hero_video: null;
  bootstrap: Bootstrap;
  check_update: CheckResult;
  fetch_bulletins: BulletinFeed;
  save_settings: Bootstrap;
  open_url: null;
  launch_official: LaunchResult;
  game_status: GameStatus;
  game_download_status: DownloadStatus;
  game_download_start: DownloadStatus;
  game_download_cancel: DownloadStatus;
  close_game: GameStatus;
  win_minimize: null;
  win_toggle_maximize: null;
  win_close: null;
  win_hide: null;
  win_drag: null;
}
export interface CommandArgs {
  attendance_status: undefined;
  attendance_send_code: { phone: string };
  attendance_login_sms: { phone: string; code: string; deviceId: string };
  attendance_login: {
    phone: string;
    password: string;
    rememberPassword: boolean;
  };
  attendance_configure: { preferences: AttendancePreferences };
  attendance_run: undefined;
  attendance_logout: { accountId: string };
  attendance_keyring_check: undefined;
  play_hero_video: undefined;
  bootstrap: undefined;
  check_update: { full: boolean };
  fetch_bulletins: undefined;
  save_settings: { settings: Settings };
  open_url: { url: string };
  launch_official: undefined;
  game_status: undefined;
  game_download_status: undefined;
  game_download_start: { mode: "check" | "apply"; full: boolean };
  game_download_cancel: undefined;
  close_game: undefined;
  win_minimize: undefined;
  win_toggle_maximize: undefined;
  win_close: undefined;
  win_hide: undefined;
  win_drag: undefined;
}
export type PreviewPaths = Partial<
  Record<
    "home" | "dataHome" | "steamRoot" | "installRoot" | "protonName",
    string
  >
>;
declare global {
  const __APP_VERSION__: string;
  interface Navigator {
    readonly userAgentData?: { readonly platform: string };
  }
  interface Window {
    __NTE_PREVIEW__?: PreviewPaths;
  }
}
