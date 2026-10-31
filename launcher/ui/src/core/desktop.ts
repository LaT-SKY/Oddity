import type { Commands, CommandArgs } from "../types/launcher";
export interface DialogOptions {
  title?: string;
  defaultPath?: string;
  directory?: boolean;
  multiple?: boolean;
  filters?: { name: string; extensions: string[] }[];
}
export type OpenDialogOptions = DialogOptions;
export type SaveDialogOptions = DialogOptions;
export interface WindowState {
  maximized: boolean;
  visible: boolean;
}
export interface DesktopBridge {
  invoke<K extends keyof Commands>(
    command: K,
    args?: CommandArgs[K],
  ): Promise<Commands[K]>;
  listen(event: string, callback: (payload: unknown) => void): () => void;
  open(options: OpenDialogOptions): Promise<string | null>;
  save(options: SaveDialogOptions): Promise<string | null>;
  homeDir(): Promise<string>;
  windowState(): Promise<WindowState>;
}
declare global {
  interface Window {
    oddity?: DesktopBridge;
  }
}
export const isDesktop = () => Boolean(window.oddity?.invoke);
export function nativeInvoke<K extends keyof Commands>(
  command: K,
  args?: CommandArgs[K],
) {
  return window.oddity!.invoke(command, args);
}
export async function listen<T>(
  event: string,
  callback: (event: { payload: T }) => void,
) {
  return window.oddity!.listen(event, (payload) =>
    callback({ payload: payload as T }),
  );
}
export const desktopWindow = {
  getState: () => window.oddity!.windowState(),
  onResized: async (callback: () => void) =>
    window.oddity!.listen("window-state", callback),
};
export const homeDir = () => window.oddity!.homeDir();
export const join = async (...parts: string[]) =>
  parts
    .map((part, index) =>
      index ? part.replace(/^\/+|\/+$/g, "") : part.replace(/\/+$/g, ""),
    )
    .join("/");
export const open = (options: OpenDialogOptions) =>
  window.oddity!.open(options);
export const save = (options: SaveDialogOptions) =>
  window.oddity!.save(options);
