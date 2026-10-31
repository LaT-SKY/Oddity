import { contextBridge, ipcRenderer } from "electron";
import type { DesktopBridge } from "../src/core/desktop";
// 首帧使用 Rust 配置, Chromium 新配置目录也保持用户原有外观.
const appearance = JSON.parse(
  process.argv
    .filter((value) => value.startsWith("--oddity-appearance="))
    .at(-1)!
    .slice("--oddity-appearance=".length),
) as { theme: string; reduceMotion: boolean };
localStorage.setItem("nte.theme", appearance.theme);
localStorage.setItem("nte.reduceMotion", appearance.reduceMotion ? "1" : "0");
const bridge: DesktopBridge = {
  invoke: async (command, args) => {
    const reply = await ipcRenderer.invoke("oddity:invoke", command, args);
    if (reply.error !== undefined) throw reply.error;
    return reply.result;
  },
  listen: (name, callback) => {
    const listener = (
      _event: Electron.IpcRendererEvent,
      event: string,
      payload: unknown,
    ) => {
      if (event === name) callback(payload);
    };
    ipcRenderer.on("oddity:event", listener);
    return () => ipcRenderer.removeListener("oddity:event", listener);
  },
  open: (options) => ipcRenderer.invoke("oddity:dialog", "open", options),
  save: (options) => ipcRenderer.invoke("oddity:dialog", "save", options),
  homeDir: () => ipcRenderer.invoke("oddity:home"),
  windowState: () => ipcRenderer.invoke("oddity:window-state"),
};
contextBridge.exposeInMainWorld("oddity", bridge);
