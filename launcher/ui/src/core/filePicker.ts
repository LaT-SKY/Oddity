import { shallowRef } from "vue";
import {
  open as nativeOpen,
  save as nativeSave,
  type OpenDialogOptions,
  type SaveDialogOptions,
} from "./desktop";
import { gamepad } from "./gamepad";

export interface FileEntry {
  name: string;
  path: string;
  directory: boolean;
}
export interface FileListing {
  path: string;
  parent: string | null;
  home: string;
  entries: FileEntry[];
}
export interface FileRequest {
  title: string;
  directory: boolean;
  save: boolean;
  defaultPath?: string;
  extensions: string[];
  resolve: (path: string | null) => void;
}
export const fileRequest = shallowRef<FileRequest | null>(null);
function choose(options: OpenDialogOptions | SaveDialogOptions, save: boolean) {
  return new Promise<string | null>((resolve) => {
    fileRequest.value = {
      title: options.title ?? (save ? "保存文件" : "选择文件"),
      directory: "directory" in options && !!options.directory,
      save,
      defaultPath: options.defaultPath,
      extensions: options.filters?.flatMap((filter) => filter.extensions) ?? [],
      resolve,
    };
  });
}
export function open(options: OpenDialogOptions): Promise<string | null> {
  return gamepad.active
    ? choose(options, false)
    : (nativeOpen({ ...options, multiple: false }) as Promise<string | null>);
}
export function save(options: SaveDialogOptions): Promise<string | null> {
  return gamepad.active ? choose(options, true) : nativeSave(options);
}
