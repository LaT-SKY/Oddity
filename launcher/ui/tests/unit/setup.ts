import { afterEach, beforeEach, vi } from "vitest";

const resizeCallbacks = new Map<Element, ResizeObserverCallback>();
export function resizeElement(element: Element, width: number, height: number) {
  resizeCallbacks.get(element)?.(
    [
      {
        target: element,
        contentRect: { width, height },
      } as ResizeObserverEntry,
    ],
    {} as ResizeObserver,
  );
}
beforeEach(() => {
  resizeCallbacks.clear();
  vi.stubGlobal(
    "DOMMatrixReadOnly",
    class {
      m41 = 0;
      constructor(value: string) {
        this.m41 = Number(
          /matrix\([^,]+,[^,]+,[^,]+,[^,]+,\s*([-.\d]+)/.exec(value)?.[1] ?? 0,
        );
      }
    },
  );
  // jsdom 没有原生动画时钟, 控制器测试只推进完成与取消边界.
  Object.defineProperty(HTMLElement.prototype, "animate", {
    configurable: true,
    value: function (
      this: HTMLElement,
      frames: Keyframe[],
      options: KeyframeAnimationOptions,
    ) {
      let reject!: (reason: Error) => void;
      const finished = new Promise<Animation>((resolve, fail) => {
        reject = fail;
        setTimeout(() => resolve(animation), Number(options.duration) || 0);
      });
      const animation = {
        finished,
        cancel: () => reject(new Error("cancelled")),
        commitStyles: () => {
          const last = frames.at(-1);
          if (!last) return;
          for (const [name, value] of Object.entries(last))
            if (name !== "offset" && typeof value !== "undefined")
              this.style.setProperty(
                name.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase()),
                String(value),
              );
        },
      } as unknown as Animation;
      return animation;
    },
  });
  vi.resetModules();
  vi.useFakeTimers();
  localStorage.clear();
  history.replaceState(null, "", "/");
  document.documentElement.className = "";
  document.body.className = "";
  document.body.innerHTML = "";
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({
      matches: false,
      media: "",
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  );
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(private callback: ResizeObserverCallback) {}
      observe(element: Element) {
        resizeCallbacks.set(element, this.callback);
      }
      unobserve(element: Element) {
        resizeCallbacks.delete(element);
      }
      disconnect() {}
    },
  );
  HTMLVideoElement.prototype.requestVideoFrameCallback = (callback) =>
    requestAnimationFrame((time) =>
      callback(time, { mediaTime: 0 } as VideoFrameCallbackMetadata),
    );
  HTMLVideoElement.prototype.cancelVideoFrameCallback = (id) =>
    cancelAnimationFrame(id);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "canPlayType").mockReturnValue(
    "probably",
  );
  vi.stubGlobal(
    "fetch",
    vi.fn(
      async (url: string) =>
        new Response(url.includes("hero-loop") ? "video" : ""),
    ),
  );
  vi.stubGlobal(
    "URL",
    class extends URL {
      static createObjectURL() {
        return "blob:http://localhost/hero";
      }
      static revokeObjectURL() {}
    },
  );
});

afterEach(() => {
  vi.clearAllTimers();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
