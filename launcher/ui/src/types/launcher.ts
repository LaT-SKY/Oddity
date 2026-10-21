export interface Settings {
  installRoot: string | null;
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
export interface Commands {
  play_hero_video: null;
  bootstrap: Bootstrap;
  check_update: CheckResult;
  save_settings: Bootstrap;
  open_url: null;
  launch_official: LaunchResult;
  game_status: GameStatus;
  close_game: GameStatus;
  win_minimize: null;
  win_toggle_maximize: null;
  win_close: null;
  win_hide: null;
  win_drag: null;
}
export interface CommandArgs {
  play_hero_video: undefined;
  bootstrap: undefined;
  check_update: { full: boolean };
  save_settings: { settings: Settings };
  open_url: { url: string };
  launch_official: undefined;
  game_status: undefined;
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
