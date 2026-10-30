import { computed, nextTick, reactive, shallowRef, watch } from "vue";
import { invoke, isTauri } from "./api";
import {
  form,
  state,
  showPage,
  handleLaunchAction,
  openSettings,
} from "./controller";
import { playerExpanded } from "../composables/useSceneMotion";
import { settingsNavigation } from "./settingsCatalog";
import type {
  Feedback,
  GamepadButton,
  GamepadDevice,
  GamepadSnapshot,
} from "../types/gamepad";
import { notifications, pauseNotifications } from "./notifications";

export const gamepad = reactive({
  device: null as GamepadDevice | null,
  devices: [] as GamepadDevice[],
  active: false,
  pressing: false,
  deviceError: "",
  feedbackError: "",
  idle: { requested: false, error: null as string | null },
  notification: false,
  windowControls: false,
  navigation: false,
  editing: null as HTMLInputElement | HTMLTextAreaElement | null,
});
export const gamepadTarget = shallowRef<HTMLElement | null>(null);
export const gamepadFamily = computed(() =>
  form.gamepad.icons === "auto"
    ? (gamepad.device?.family ?? "generic")
    : form.gamepad.icons,
);
export const confirmButton = computed(() =>
  gamepad.device?.family === "switch" ? 1 : 0,
);
export const backButton = computed(() =>
  gamepad.device?.family === "switch" ? 0 : 1,
);

export function gamepadGlyph(button: GamepadButton) {
  const family = gamepadFamily.value;
  const prefix =
    family === "dualsense" || family === "ds4"
      ? "ps"
      : family === "switch"
        ? "ns"
        : "xbox";
  const face =
    prefix === "ps"
      ? ["cross", "circle", "square", "triangle"]
      : prefix === "ns"
        ? ["b", "a", "y", "x"]
        : ["a", "b", "x", "y"];
  if (["confirm", "back", "west", "north"].includes(button))
    return `${prefix}-${face[button === "confirm" ? confirmButton.value : button === "back" ? backButton.value : button === "west" ? 2 : 3]}`;
  return `${prefix}-${button}`;
}

export function gamepadViewport(target: HTMLElement) {
  let top = 0,
    bottom = innerHeight;
  for (
    let parent = target.parentElement;
    parent && parent !== document.body;
    parent = parent.parentElement
  ) {
    const overflow = getComputedStyle(parent).overflowY;
    if (overflow === "visible" || !overflow) continue;
    const bounds = parent.getBoundingClientRect();
    top = Math.max(top, bounds.top);
    bottom = Math.min(bottom, bounds.bottom);
  }
  return { top, bottom };
}

export function gamepadFeedback(effect: Feedback | "stop") {
  if (
    !gamepad.device ||
    (effect !== "stop" && (!gamepad.active || !form.gamepad.vibration))
  )
    return Promise.resolve();
  return invoke("gamepad_feedback", {
    deviceId: gamepad.device.id,
    effect,
  })
    .then(() => {
      if (effect !== "stop") gamepad.feedbackError = "";
    })
    .catch((error) => {
      gamepad.feedbackError = String(error);
    });
}

export function createGamepadNavigation() {
  const positions = new Map<string, HTMLElement>();
  const scopedPositions = new WeakMap<Element, HTMLElement>();
  let previousKey = "";
  let page: string | undefined;
  let first = true;
  let listScroll = 0;
  let windowReturn: HTMLElement | null = null;
  const navigationBar = () =>
    document.querySelector<HTMLElement>(
      ".page.is-active .settings-categories,.page.is-active .control-tabs--navigation",
    );
  const categoryButtons = () =>
    [
      ...(navigationBar()?.querySelectorAll<HTMLElement>("button") ?? []),
    ].filter(visible);
  const modal = () =>
    document.querySelector<HTMLElement>(".pad-keyboard-backdrop") ??
    [
      ...document.querySelectorAll<HTMLElement>(
        ".modal:not([inert]), [data-pad-modal]",
      ),
    ]
      .filter(visible)
      .at(-1) ??
    null;
  const navigating = () =>
    gamepad.navigation &&
    !gamepad.windowControls &&
    !modal() &&
    !gamepad.notification &&
    !playerExpanded.value;
  const key = () =>
    `${state.page}/${settingsNavigation.category}/${gamepad.navigation ? "navigation" : "content"}/${playerExpanded.value ? "player" : "page"}/${gamepad.windowControls ? "window" : "page"}`;
  function visible(element: Element): boolean {
    return (
      !element.matches(':disabled,[aria-disabled="true"]') &&
      !element.closest('[hidden],[inert],[aria-hidden="true"]') &&
      element.getClientRects().length > 0
    );
  }
  function root(): Element | null {
    const dialog = modal();
    if (dialog) return dialog.querySelector(".modal__panel") ?? dialog;
    if (gamepad.windowControls) return document.querySelector(".win-controls");
    if (gamepad.notification)
      return document.querySelector(".notification-stack");
    if (playerExpanded.value) return document.querySelector(".game-records");
    const page = document.querySelector(".page.is-active");
    return gamepad.navigation
      ? page
      : (page?.querySelector(".settings-panel.is-current,.bulletin-content") ??
          page);
  }
  function targets() {
    const current = root();
    const navigation =
      gamepad.navigation &&
      !gamepad.windowControls &&
      !modal() &&
      !gamepad.notification &&
      !playerExpanded.value;
    return [
      ...(current?.querySelectorAll<HTMLElement>(
        'button,input:not([type="hidden"]),textarea,a[href],[data-pad-region]',
      ) ?? []),
    ].filter((item) => {
      if (
        !visible(item) ||
        item.matches(
          "[data-page],.drawer-dismiss,.drawer-close,select,[data-pad-skip]",
        )
      )
        return false;
      if (navigation)
        return !!item.closest(
          ".settings-categories,.control-tabs--navigation,.settings-search,.settings-search-results",
        );
      return (
        !item.closest(
          ".settings-categories,.control-tabs--navigation,.settings-search",
        ) &&
        !item.matches("[data-category]") &&
        (!item.closest(".page-head") ||
          item.matches('[data-pad-primary],[role="combobox"]'))
      );
    });
  }
  function point(target: HTMLElement | null | undefined) {
    const previous = gamepadTarget.value;
    if (previous) previous.removeAttribute("data-pad-selected");
    gamepadTarget.value = target ?? null;
    target?.setAttribute("data-pad-selected", "true");
    target?.focus({ preventScroll: true });
    // 只滚动内容容器的纵轴, 避免把入场中的抽屉和窗口一起推移.
    for (
      let parent = target?.parentElement;
      target && parent && parent !== document.body;
      parent = parent.parentElement
    ) {
      const style = getComputedStyle(parent);
      if (
        !["auto", "scroll"].includes(style.overflowY) ||
        parent.scrollHeight <= parent.clientHeight
      )
        continue;
      const bounds = parent.getBoundingClientRect();
      const rect = target.getBoundingClientRect();
      const top =
        bounds.top +
        parent.clientTop +
        (parseFloat(style.scrollPaddingTop) || 0) +
        8;
      const bottom =
        bounds.top +
        parent.clientTop +
        parent.clientHeight -
        (parseFloat(style.scrollPaddingBottom) || 0) -
        8;
      const delta =
        rect.top < top
          ? rect.top - top
          : rect.bottom > bottom
            ? Math.min(rect.bottom - bottom, rect.top - top)
            : 0;
      if (delta) parent.scrollBy({ top: delta, behavior: "instant" });
    }
    if (!navigating() && target?.getAttribute("role") === "tab") target.click();
    const current = root();
    if (target && current) scopedPositions.set(current, target);
    if (target && !modal() && !gamepad.notification)
      positions.set(key(), target);
  }
  function refresh() {
    if (gamepad.notification && !notifications.length) {
      gamepad.notification = false;
      pauseNotifications("gamepad", false);
    }
    if (!gamepad.active) return;
    if (page !== state.page) {
      page = state.page;
      gamepad.navigation = !!navigationBar();
      first = true;
    }
    const current = root();
    const items = targets();
    const samePage = previousKey === key();
    previousKey = key();
    if (
      !first &&
      samePage &&
      gamepadTarget.value &&
      items.includes(gamepadTarget.value)
    )
      return;
    const previous = gamepadTarget.value;
    const actions =
      !first && samePage && previous?.isConnected && current?.contains(previous)
        ? previous.closest("[data-pad-actions]")
        : null;
    if (actions) {
      const replacement = items.find((item) => actions.contains(item));
      if (replacement) point(replacement);
      // 请求启动期间先保留原操作, 等待取消或暂停按钮出现。
      return;
    }
    const remembered = first
      ? undefined
      : current && (modal() || gamepad.notification)
        ? scopedPositions.get(current)
        : positions.get(key());
    const category = navigating()
      ? (categoryButtons().find(
          (item) =>
            item.getAttribute("aria-current") === "true" ||
            item.getAttribute("aria-selected") === "true",
        ) ?? categoryButtons()[0])
      : null;
    const launch =
      !modal() &&
      !gamepad.notification &&
      !gamepad.windowControls &&
      state.page === "home" &&
      !playerExpanded.value
        ? document.getElementById(
            state.game === "launching" ? "launch-region" : "launch-btn",
          )
        : null;
    point(
      remembered && items.includes(remembered)
        ? remembered
        : (category ??
            launch ??
            items.find(
              (item) =>
                item.getAttribute("aria-label") !== "关闭弹窗" &&
                !item.hasAttribute("data-pad-close") &&
                !item.closest(".page-head"),
            ) ??
            items[0]),
    );
    first = false;
  }

  function dispatch(key: string) {
    const event = new KeyboardEvent("keydown", {
      key,
      bubbles: true,
      cancelable: true,
    });
    gamepadTarget.value?.dispatchEvent(event);
    const focus = document.activeElement;
    if (
      focus instanceof HTMLElement &&
      focus !== gamepadTarget.value &&
      root()?.contains(focus) &&
      visible(focus) &&
      focus.matches("button,input,textarea,a[href],[data-pad-region]")
    )
      point(focus);
    return event.defaultPrevented;
  }
  async function changed() {
    await nextTick();
    refresh();
  }
  function scroll(amount: number) {
    refresh();
    const target = gamepadTarget.value;
    if (target?.matches('[role="combobox"][aria-expanded="true"]')) {
      listScroll += amount;
      if (Math.abs(listScroll) >= 80) {
        dispatch(listScroll > 0 ? "ArrowDown" : "ArrowUp");
        listScroll = 0;
      }
      return;
    }
    listScroll = 0;
    const current = navigating() ? navigationBar() : root();
    if (!target || !current?.contains(target)) return;
    const boundary = navigating()
      ? current
      : (current.closest(".page") ?? current);
    let container: HTMLElement | null = target;
    while (container && boundary.contains(container)) {
      if (
        ["auto", "scroll"].includes(getComputedStyle(container).overflowY) &&
        container.scrollHeight > container.clientHeight
      )
        break;
      if (container === boundary) return;
      container = container.parentElement;
    }
    if (!container) return;
    const scroller = container;
    const anchor = target.getBoundingClientRect();
    scroller.scrollBy({ top: amount, behavior: "instant" });
    const viewport = gamepadViewport(target);
    const frame = scroller.getBoundingClientRect(),
      style = getComputedStyle(scroller);
    const top =
      Math.max(
        viewport.top,
        frame.top +
          scroller.clientTop +
          (parseFloat(style.scrollPaddingTop) || 0),
      ) + 8;
    const bottom =
      Math.min(
        viewport.bottom,
        frame.top +
          scroller.clientTop +
          scroller.clientHeight -
          (parseFloat(style.scrollPaddingBottom) || 0),
      ) - 8;
    const moved = target.getBoundingClientRect();
    if (moved.top >= top && moved.bottom <= bottom) return;
    const candidates = targets().filter((item) => {
      const bounds = item.getBoundingClientRect();
      return (
        scroller.contains(item) && bounds.top >= top && bounds.bottom <= bottom
      );
    });
    if (!candidates.length) return;
    const y = Math.max(top, Math.min(bottom, anchor.top + anchor.height / 2));
    const score = (item: HTMLElement) => {
      const bounds = item.getBoundingClientRect();
      return (
        Math.abs(bounds.top + bounds.height / 2 - y) +
        0.35 *
          Math.abs(
            bounds.left + bounds.width / 2 - anchor.left - anchor.width / 2,
          )
      );
    };
    point(
      candidates.reduce((best, item) =>
        score(item) < score(best) ? item : best,
      ),
    );
  }
  async function press(button: number) {
    refresh();
    const target = gamepadTarget.value;
    if (!modal() && !gamepad.windowControls && state.page === "home") {
      if (button === 2) {
        void gamepadFeedback("confirm");
        await handleLaunchAction();
        return;
      }
      if (button === 3) {
        if (target?.matches('[role="combobox"][aria-expanded="true"]'))
          dispatch("Escape");
        playerExpanded.value = !playerExpanded.value;
        await changed();
        return;
      }
    }
    const openSelect = target?.matches(
      '[role="combobox"][aria-expanded="true"]',
    );
    if (openSelect && [4, 6, 8, 9, 10].includes(button)) dispatch("Escape");
    else if (openSelect) {
      if (button === backButton.value) dispatch("Escape");
      else if (button === confirmButton.value) dispatch("Enter");
      else if (button === 11 || button === 12)
        dispatch(button === 11 ? "ArrowUp" : "ArrowDown");
      return;
    }
    if (button === backButton.value) {
      if (gamepad.windowControls) {
        gamepad.windowControls = false;
        const previous = windowReturn;
        windowReturn = null;
        if (previous && visible(previous) && root()?.contains(previous))
          point(previous);
        else {
          first = true;
          refresh();
        }
        previousKey = key();
        return;
      }
      if (target?.matches("[data-pad-region]") && dispatch("Escape")) return;
      if (gamepad.editing) {
        gamepad.editing = null;
        return;
      }
      if (gamepad.notification) {
        gamepad.notification = false;
        pauseNotifications("gamepad", false);
        await changed();
        return;
      }
      const dialog = modal();
      if (dialog) {
        if (!dispatch("Escape"))
          dialog
            .querySelector<HTMLElement>(
              '[aria-label="关闭弹窗"],[data-pad-close]',
            )
            ?.click();
        await nextTick();
        refresh();
      } else if (playerExpanded.value) {
        playerExpanded.value = false;
        await changed();
      } else if (!gamepad.navigation && navigationBar()) {
        gamepad.navigation = true;
        first = true;
        await changed();
      } else {
        showPage("home");
        await changed();
      }
      return;
    }
    if (
      !modal() &&
      !gamepad.editing &&
      !gamepad.notification &&
      !gamepad.windowControls
    ) {
      if (button === 9 || button === 10) {
        const pages = [
          ...document.querySelectorAll<HTMLElement>(".rail__nav [data-page]"),
        ];
        const index = pages.findIndex(
          (item) => item.dataset.page === state.page,
        );
        pages[
          (index + (button === 10 ? 1 : pages.length - 1)) % pages.length
        ]?.click();
        await changed();
        return;
      }
      if (button === 8 && state.page !== "home") {
        const action = document.querySelector<HTMLButtonElement>(
          ".page.is-active [data-pad-primary]",
        );
        if (action && !action.disabled) {
          void gamepadFeedback("confirm");
          action.click();
        }
        return;
      }
      if (button === 6) {
        openSettings("gamepad");
        gamepad.navigation = true;
        first = true;
        await changed();
        return;
      }
      if (button === 4) {
        const control = [
          ...document.querySelectorAll<HTMLElement>(".win-controls button"),
        ].find(visible);
        if (!control) return;
        windowReturn = gamepadTarget.value;
        gamepad.windowControls = true;
        point(control);
        previousKey = key();
        first = false;
        return;
      }
    }
    if (
      button === confirmButton.value &&
      target &&
      gamepad.navigation &&
      !gamepad.windowControls &&
      !modal() &&
      (navigationBar()?.contains(target) || target.matches(".settings-result"))
    ) {
      target.click();
      gamepad.navigation = false;
      first = true;
      await changed();
      return;
    }
    if (button === confirmButton.value && target) {
      const bounds = target.getBoundingClientRect(),
        viewport = gamepadViewport(target);
      if (bounds.bottom <= viewport.top || bounds.top >= viewport.bottom)
        return;
      gamepad.pressing = true;
      if (!target.closest('[data-pad-feedback="off"]'))
        void gamepadFeedback("confirm");
      if (
        (target instanceof HTMLInputElement &&
          !["checkbox", "radio", "range", "button", "submit"].includes(
            target.type,
          )) ||
        target instanceof HTMLTextAreaElement
      ) {
        gamepad.editing = target as HTMLInputElement | HTMLTextAreaElement;
      } else if (target.matches('[role="combobox"],[data-pad-region]'))
        dispatch("Enter");
      else target.click();
      await nextTick();
      refresh();
      return;
    }
    if (button < 11 || button > 14 || !target) return;
    const direction = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"][
      button - 11
    ]!;
    // 收起的下拉框只参与空间导航; 确认后才交给控件处理方向键.
    if (
      !navigating() &&
      !target.matches('[role="combobox"][aria-expanded="false"]') &&
      dispatch(direction)
    )
      return;
    if (
      target instanceof HTMLInputElement &&
      (target.type === "range" || target.type === "number") &&
      button >= 13
    ) {
      if (button === 14) target.stepUp();
      else target.stepDown();
      target.dispatchEvent(new Event("input", { bubbles: true }));
      target.dispatchEvent(new Event("change", { bubbles: true }));
      return;
    }
    const items = targets();
    if (target.matches(":disabled") && target.closest("[data-pad-actions]"))
      items.push(target);
    const rows: { y: number; items: HTMLElement[] }[] = [];
    for (const item of items) {
      const box = item.getBoundingClientRect();
      const y = box.top + box.height / 2;
      const row = rows.find(
        (row) => Math.abs(row.y - y) < Math.min(box.height / 2, 18),
      );
      if (row) row.items.push(item);
      else rows.push({ y, items: [item] });
    }
    rows.sort((a, b) => a.y - b.y);
    for (const row of rows)
      row.items.sort(
        (a, b) =>
          a.getBoundingClientRect().left - b.getBoundingClientRect().left,
      );
    const rowIndex = rows.findIndex((row) => row.items.includes(target));
    if (rowIndex < 0) return;
    const row = rows[rowIndex]!;
    if (button >= 13) {
      point(
        row.items[row.items.indexOf(target) + (button === 14 ? 1 : -1)] ??
          target,
      );
    } else {
      const next = rows[rowIndex + (button === 12 ? 1 : -1)];
      if (!next) return;
      const center =
        target.getBoundingClientRect().left +
        target.getBoundingClientRect().width / 2;
      point(
        next.items.reduce((best, item) =>
          Math.abs(
            item.getBoundingClientRect().left +
              item.getBoundingClientRect().width / 2 -
              center,
          ) <
          Math.abs(
            best.getBoundingClientRect().left +
              best.getBoundingClientRect().width / 2 -
              center,
          )
            ? item
            : best,
        ),
      );
    }
  }
  const stopWatch = watch(
    [
      () => state.page,
      () => settingsNavigation.category,
      () => playerExpanded.value,
      () => state.download?.phase,
    ],
    () => {
      if (!gamepad.active) return;
      if (page !== state.page) {
        gamepad.navigation = ["settings", "bulletin", "game-mode"].includes(
          state.page,
        );
        first = true;
      }
      void changed();
    },
    { flush: "sync" },
  );
  return {
    press,
    scroll,
    refresh,
    reset: () => {
      point(null);
      gamepad.pressing = false;
      gamepad.notification = false;
      gamepad.windowControls = false;
      windowReturn = null;
      pauseNotifications("gamepad", false);
    },
    dispose: stopWatch,
  };
}

export function startGamepad() {
  const navigation = createGamepadNavigation();
  const events = new AbortController();
  let stopped = false;
  let timer = 0;
  let previous = new Set<number>();
  let repeating = -1;
  let repeatAt = 0;
  let focused = false;
  let device: number | undefined;
  let sampledAt = performance.now();
  const stopFeedbackWatch = watch(
    () =>
      notifications[0] && `${notifications[0].id}:${notifications[0].revision}`,
    () => {
      const notification = notifications[0];
      if (
        gamepad.active &&
        document.hasFocus() &&
        notification &&
        notification.kind !== "info"
      )
        void gamepadFeedback(
          notification.kind === "error" ? "error" : "complete",
        );
    },
  );
  const stopIdleWatch = watch(
    () => gamepad.active,
    (active) => {
      if (!active && isTauri())
        void invoke("gamepad_release_idle").catch((error) => {
          gamepad.deviceError = String(error);
        });
    },
  );
  const mouse = (event: PointerEvent) => {
    if (
      event.type === "pointermove" &&
      Math.abs(event.movementX) + Math.abs(event.movementY) < 3
    )
      return;
    gamepad.active = false;
    navigation.reset();
  };
  document.addEventListener("pointermove", mouse, { signal: events.signal });
  document.addEventListener("pointerdown", mouse, { signal: events.signal });
  document.addEventListener(
    "keydown",
    (event) => {
      if (event.isTrusted) {
        gamepad.active = false;
        navigation.reset();
      }
    },
    { signal: events.signal },
  );
  window.addEventListener(
    "blur",
    () => {
      gamepad.active = false;
      navigation.reset();
      void gamepadFeedback("stop");
    },
    { signal: events.signal },
  );
  async function poll() {
    const snapshot = await invoke("gamepad_poll").catch((error) => {
      gamepad.deviceError = String(error);
      return null;
    });
    if (stopped) return;
    if (snapshot) await accept(snapshot);
    timer = window.setTimeout(
      () => {
        void poll();
      },
      snapshot?.focused ? 16 : 180,
    );
  }
  async function accept(snapshot: GamepadSnapshot) {
    gamepad.idle = snapshot.idle;
    gamepad.devices = snapshot.devices;
    gamepad.device = snapshot.device;
    gamepad.deviceError = snapshot.error ?? "";
    const held = new Set(snapshot.buttons);
    const [x, y, , scroll] = snapshot.axes;
    const dead = form.gamepad.deadZone;
    if (Math.abs(x) > dead || Math.abs(y) > dead)
      held.add(Math.abs(x) > Math.abs(y) ? (x > 0 ? 14 : 13) : y > 0 ? 12 : 11);
    if (!snapshot.focused || !focused || snapshot.device?.id !== device) {
      sampledAt = performance.now();
      previous = held;
      focused = snapshot.focused;
      device = snapshot.device?.id;
      if (!snapshot.focused || !snapshot.device) {
        gamepad.active = false;
        navigation.reset();
      }
      return;
    }
    const pressed = new Set([
      ...snapshot.pressed,
      ...[...held].filter((button) => !previous.has(button)),
    ]);
    const direction =
      [...held].find((button) => button >= 11 && button <= 14) ?? -1;
    const now = performance.now();
    const elapsed = Math.min(32, now - sampledAt);
    sampledAt = now;
    if (direction !== repeating) {
      repeating = direction;
      repeatAt = now + 320;
    } else if (direction >= 0 && now >= repeatAt) {
      pressed.add(direction);
      repeatAt = now + 95;
    }
    previous = held;
    gamepad.pressing = held.has(confirmButton.value);
    if (pressed.size) {
      gamepad.active = true;
      for (const button of pressed) await navigation.press(button);
    }
    if (Math.abs(scroll) > dead) {
      gamepad.active = true;
      navigation.scroll(
        ((Math.sign(scroll) * (Math.abs(scroll) - dead)) / (1 - dead)) *
          elapsed *
          0.8,
      );
    }
    navigation.refresh();
  }
  if (isTauri()) void poll();
  return () => {
    stopped = true;
    clearTimeout(timer);
    events.abort();
    stopFeedbackWatch();
    stopIdleWatch();
    navigation.dispose();
    navigation.reset();
    void gamepadFeedback("stop");
  };
}
