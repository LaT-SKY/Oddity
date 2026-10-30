import type { FileListing } from "../core/filePicker";
import type {
  GamepadOptions,
  GamepadSnapshot,
  Profile,
  Feedback,
} from "./gamepad";
export type AttendanceTrigger = "off" | "launch" | "boot" | "daily" | "manual";
export interface AttendanceTasks {
  communitySign: boolean;
  roleSign: boolean;
  browsePosts: boolean;
  likePosts: boolean;
  sharePosts: boolean;
  cloudDuration: boolean;
}
export interface AttendancePreferences {
  trigger: AttendanceTrigger;
  time: string;
  tasks: AttendanceTasks;
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
  gameRecord?: GameRecord | null;
  gameRecordError?: AttendanceFailure | null;
}
export interface GameRecord {
  roleId: string;
  roleName: string;
  serverName: string;
  stamina: number | null;
  staminaMax: number | null;
  cityStamina: number | null;
  cityStaminaMax: number | null;
  activity: number | null;
  weeklyRemaining: number | null;
  updatedAt: number;
}
export interface AttendanceStatus {
  accounts: AttendanceAccount[];
  preferences: AttendancePreferences;
}
export interface GamescopeOptions {
  enabled: boolean;
  windowMode: "fullscreen" | "borderless" | "windowed";
  gameWidth: number;
  gameHeight: number;
  outputWidth: number;
  outputHeight: number;
  fps: number;
  scaling: "linear" | "nearest" | "fsr" | "nis" | "integer";
}
export interface GpuConfig {
  enabled: boolean;
  device: number;
  vendor: "nvidia" | "amd";
  coreOffset: number;
  memoryOffset: number;
  powerMode: number;
  amdLevel: "auto" | "high" | "low" | "profileStandard" | "profilePeak";
}
export interface GpuInfo {
  config: GpuConfig;
  devices: { id: number; vendor: GpuConfig["vendor"] }[];
}
export interface GameModeTuning {
  pinCores: string | null;
  parkCores: string | null;
  renice: number | null;
  ioprio: string | null;
}
export interface GameModeOptions {
  performance: boolean;
  inhibit: boolean;
  notifications: boolean;
  overlay: boolean;
  gamescope: GamescopeOptions;
}
export interface GameModeFeatureStatus {
  id: keyof GameModeOptions;
  available: boolean;
  state:
    | "ready"
    | "unavailable"
    | "disabled"
    | "waiting"
    | "active"
    | "requested"
    | "error"
    | "finished";
  detail: string;
}
export interface GameModeStatus {
  capabilities: GameModeFeatureStatus[];
  session: GameModeFeatureStatus[] | null;
}
export type MeteredPolicy = "ask" | "pause" | "allow";
export interface UpdateWindow {
  start: string;
  end: string;
}
export interface UpdateSettings {
  autoDownload: boolean;
  metered: MeteredPolicy;
  limitBytesPerSecond: number;
  window: UpdateWindow | null;
}
export interface Settings {
  installRoot: string | null;
  globalInstallRoot: string | null;
  distribution: "cn" | "global";
  runtimeEnvironments: RuntimeEnvironment[];
  activeEnvironment: string | null;
  stableEnvironment: string | null;
  launchMode: string;
  gameMode: GameModeOptions;
  gamepad: GamepadOptions;
  updates: UpdateSettings;
  language: string;
  reduceMotion: boolean;
  stackBulletins: boolean;
  theme: string;
  heroMedia: string;
  adaptiveTheme?: boolean;
  minimizeToTrayOnClose: boolean;
  showLauncherAfterGameExit: boolean;
  snapshotSuccessMinutes: number;
}
export interface PlaytimeSession {
  startedAt: number;
  endedAt: number;
  mode: string;
  distribution: "cn" | "global";
}
export interface ActivePlaytime {
  startedAt: number;
  lastSeenAt: number;
  mode: string;
  distribution: "cn" | "global";
}
export interface PlaytimeHistory {
  sessions: PlaytimeSession[];
  active: ActivePlaytime | null;
}
export interface ProtonPin {
  path: string;
  identity: string;
}
export interface RuntimeEnvironment {
  id: string;
  name: string;
  proton: ProtonPin;
  installRoot: string;
  distribution: "cn" | "global";
  createdAt: number;
}
export interface RuntimeInstallation {
  distribution: "cn" | "global";
  installRoot: string;
  installed: boolean;
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
  runtimeInstallations: RuntimeInstallation[];
  settings: Settings;
  appVersion: string;
  os: string;
}
export interface CheckResult {
  installed: boolean;
  distribution: "cn" | "global";
  installRoot: string;
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
  error?: string | null;
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
  mode: "check" | "apply";
  phase: DownloadPhase;
  /** manual 或 auto, 用于区分手动与自动任务. */
  origin: "manual" | "auto";
  /** 自动下载的等待原因, 空字符串表示没有等待. */
  autoWaiting: string;
  /** 最近一次资源检查完成时间 (Unix 毫秒), 0 表示尚未检查. */
  checkedAt: number;
  distribution: "cn" | "global";
  installRoot: string;
  /** 官方本地清单存在时视为已安装。 */
  installed: boolean;
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
  /** 检查时安装目录所在文件系统的可用空间, 读取失败时为 null. */
  diskAvailableBytes: number | null;
  /** 按当前资源计划估算的额外空间需求. */
  diskRequiredBytes: number;
  error: string;
}
export type BulletinKind = "bilibili" | "x" | "preview" | "maintenance";
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
export interface LauncherBackgroundMedia {
  video: string | null;
  image: string | null;
}
export interface LogPreview {
  id: string;
  fileCount: number;
  redactions: { home: number; phone: number; credentials: number; jwt: number };
  samples: { file: string; line: number; text: string }[];
  normalizedFiles: number;
}

export type SnapshotStatus =
  "starting" | "running" | "success" | "failed" | "interrupted";
export type SnapshotFailureReason = "spawn" | "earlyExit" | "interrupted";
export interface SnapshotFailure {
  reason: SnapshotFailureReason;
  exitCode: number | null;
  detail: string | null;
}
export interface SnapshotSummary {
  id: string;
  capturedAt: number;
  status: SnapshotStatus;
  durationMs: number | null;
  failure: SnapshotFailure | null;
  environment: string | null;
  proton: string | null;
  distribution: "cn" | "global";
  mode: string;
}
export interface SnapshotReference {
  label: string;
  savedAt: number;
  capturedAt: number;
  environment: string | null;
  proton: string | null;
  distribution: "cn" | "global";
  mode: string;
}
export interface SnapshotOverview {
  baseline: SnapshotSummary | null;
  reference: SnapshotReference | null;
  latest: SnapshotSummary | null;
  attempts: number;
}
export type SnapshotDiffStatus =
  "same" | "changed" | "unknown" | "added" | "removed";
export interface SnapshotDiffRow {
  key: string;
  section: string;
  label: string;
  baseline: string | null;
  current: string | null;
  status: SnapshotDiffStatus;
}
export interface SnapshotComparison {
  target: "baseline" | "reference";
  targetLabel: string;
  targetTime: number;
  targetEnvironment: string | null;
  currentEnvironment: string | null;
  rows: SnapshotDiffRow[];
  changed: number;
  unknown: number;
  unchanged: number;
}

export interface Commands {
  browse_files: FileListing;
  gamepad_poll: GamepadSnapshot;
  gamepad_release_idle: null;
  gamepad_feedback: null;
  gamepad_profile_validate: Profile;
  gamepad_profile_load: Profile;
  gamepad_profile_save: void;
  attendance_status: AttendanceStatus;
  attendance_refresh_game_records: AttendanceStatus;
  attendance_login: AttendanceStatus;
  attendance_send_code: string;
  attendance_login_sms: AttendanceStatus;
  attendance_configure: AttendanceStatus;
  attendance_run: AttendanceStatus;
  attendance_logout: AttendanceStatus;
  attendance_keyring_check: void;
  play_hero_video: null;
  launcher_background: LauncherBackgroundMedia;
  bootstrap: Bootstrap;
  check_update: CheckResult;
  fetch_bulletins: BulletinFeed;
  save_settings: Bootstrap;
  environment_create: Bootstrap;
  environment_action: Bootstrap;
  prepare_log_export: LogPreview;
  save_log_export: string;
  discard_log_export: void;
  environment_snapshot_overview: SnapshotOverview;
  environment_snapshot_compare: SnapshotComparison;
  environment_snapshot_save_reference: SnapshotReference;
  environment_snapshot_delete_reference: void;
  environment_snapshot_clear_history: void;
  open_directory: null;
  open_url: null;
  launch_official: LaunchResult;
  game_status: GameStatus;
  take_launch_error: string | null;
  playtime_history: PlaytimeHistory;
  playtime_export_poster: string;
  game_mode_status: GameModeStatus;
  game_mode_gpu_info: GpuInfo;
  game_mode_tuning_info: GameModeTuning;
  game_mode_tuning_apply: GameModeTuning;
  game_mode_gpu_apply: GpuInfo;
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
  browse_files: { path: string | null };
  gamepad_poll: undefined;
  gamepad_release_idle: undefined;
  gamepad_feedback: { deviceId: number; effect: Feedback | "stop" };
  gamepad_profile_validate: { profile: Profile };
  gamepad_profile_load: { path: string };
  gamepad_profile_save: { path: string; profile: Profile };
  attendance_status: undefined;
  attendance_refresh_game_records: undefined;
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
  launcher_background: undefined;
  bootstrap: undefined;
  check_update: { full: boolean };
  fetch_bulletins: { distribution: "cn" | "global" };
  save_settings: { settings: Settings };
  environment_create: {
    distribution: "cn" | "global";
    name: string;
    protonPath: string;
    sourceId: string | null;
  };
  environment_action: {
    action: "switch" | "keep" | "discard";
    id: string | null;
  };
  prepare_log_export: undefined;
  save_log_export: { id: string; destination: string };
  discard_log_export: { id: string };
  environment_snapshot_overview: undefined;
  environment_snapshot_compare: { target: "baseline" | "reference" };
  environment_snapshot_save_reference: {
    label: string | null;
    attemptId: string | null;
  };
  environment_snapshot_delete_reference: undefined;
  environment_snapshot_clear_history: undefined;
  open_directory: { path: string };
  open_url: { url: string };
  launch_official: undefined;
  game_status: undefined;
  take_launch_error: undefined;
  playtime_history: undefined;
  playtime_export_poster: { destination: string; data: string };
  game_mode_status: undefined;
  game_mode_gpu_info: undefined;
  game_mode_tuning_info: undefined;
  game_mode_tuning_apply: { config: GameModeTuning };
  game_mode_gpu_apply: { config: GpuConfig };
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
