import { invoke, isTauri, listenGameState } from "./api.js";

const $ = (selector, root = document) => root.querySelector(selector);
const $$ = (selector, root = document) => Array.from(root.querySelectorAll(selector));

const state = {
  bootstrap: null,
  check: null,
  page: "home",
  checking: false,
  game: "idle",
  session: null,
  heroMedia: "video",
  toastTimer: 0,
};

function text(id, value) {
  const el = document.getElementById(id);
  if (el) el.textContent = value == null ? "" : String(value);
}

function isReady(data) {
  return Boolean(data && data.ok && data.protonName && data.umuAvailable && data.gameExists);
}

const THEMES = new Set(["light", "dark"]);

// The native launch returns as soon as umu-run is spawned. Keep "正在启动"
// visible for a beat so the feedback is readable before the window hides.
const LAUNCH_FEEDBACK_MS = 900;
const LAUNCH_COPY = {
  idle: { label: "启动游戏", icon: "i-play" },
  launching: { label: "正在启动", icon: "i-play" },
  running: { label: "关闭游戏", icon: "i-power" },
  closing: { label: "正在关闭", icon: "i-power" },
};

const wait = (ms) => new Promise((resolve) => window.setTimeout(resolve, ms));

function applyTheme(theme) {
  const value = THEMES.has(theme) ? theme : "light";
  document.documentElement.dataset.theme = value;
  try {
    localStorage.setItem("nte.theme", value);
  } catch {
    // Theme is still applied for this session when storage is unavailable.
  }
  const select = $("#set-theme");
  if (select && select.value !== value) select.value = value;
  return value;
}

const HERO_MEDIA = new Set(["video", "photo"]);
const REDUCE_MOTION_KEY = "nte.reduceMotion";
const INTRO_DURATION = 380;
let heroVideoUrl = "";
let heroVideoRequest = null;
let heroActive = 0;
let heroLooping = false;
let introStarted = false;
let introDone = false;
let pendingHeroMedia = null;
const introWaiters = [];

function motionReduced() {
  return (
    document.documentElement.classList.contains("reduce-motion") ||
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function applyReduceMotion(value) {
  const enabled = Boolean(value);
  document.documentElement.classList.toggle("reduce-motion", enabled);
  try {
    localStorage.setItem(REDUCE_MOTION_KEY, enabled ? "1" : "0");
  } catch {
    // The preference still applies for this session.
  }
  const toggle = $("#set-motion");
  if (toggle && toggle.checked !== enabled) toggle.checked = enabled;
  return enabled;
}

function whenIntroDone(callback) {
  if (introDone) callback();
  else introWaiters.push(callback);
}

function finishIntro() {
  if (introDone) return;
  introDone = true;
  document.body.classList.add("is-intro-done");
  const pending = pendingHeroMedia;
  pendingHeroMedia = null;
  if (pending) applyHeroMedia(pending);
  introWaiters.splice(0).forEach((callback) => callback());
}

function startIntro() {
  if (introStarted) return;
  introStarted = true;
  if (window.__nteIntroFallback) {
    window.clearTimeout(window.__nteIntroFallback);
    window.__nteIntroFallback = 0;
  }
  // Flush the hidden first-paint state so the fade always starts from it
  // instead of jumping straight to full opacity.
  const content = $(".hero__content");
  if (content) void getComputedStyle(content).opacity;
  document.documentElement.classList.remove("intro");
  window.setTimeout(finishIntro, motionReduced() ? 0 : INTRO_DURATION);
}

function heroVideos() {
  return [$("#hero-video-a"), $("#hero-video-b")].filter(Boolean);
}

async function loadHeroVideo(videos) {
  if (!videos.length) return false;
  if (heroVideoUrl) {
    videos.forEach((video) => {
      if (video.src !== heroVideoUrl) video.src = heroVideoUrl;
    });
    return true;
  }
  if (heroVideoRequest) return heroVideoRequest;

  const candidates = [
    { url: "assets/hero-loop.mp4", type: 'video/mp4; codecs="avc1.42E01E"' },
    { url: "assets/hero-loop.webm", type: 'video/webm; codecs="vp9"' },
  ];
  const chosen = candidates.find((candidate) => videos[0].canPlayType(candidate.type));
  if (!chosen) return false;

  heroVideoRequest = (async () => {
    const response = await fetch(chosen.url, { cache: "force-cache" });
    if (!response.ok) return false;
    const blob = await response.blob();
    heroVideoUrl = URL.createObjectURL(blob);
    videos.forEach((video) => {
      video.src = heroVideoUrl;
      video.preload = "auto";
      video.load();
    });
    return true;
  })().catch(() => false);

  const result = await heroVideoRequest;
  heroVideoRequest = null;
  return result;
}

function canPlayHeroVideo() {
  if (state.heroMedia !== "video") return false;
  return !motionReduced();
}

function bindHeroLoop(videos) {
  videos.forEach((video) => {
    if (video.dataset.loopBound) return;
    video.dataset.loopBound = "1";
    video.addEventListener("ended", () => {
      if (!heroLooping || videos[heroActive] !== video) return;
      const next = videos[1 - heroActive];
      // Hard cut on the final official frame. The next layer is already
      // paused at frame 0 and its poster is the same frame, so no black gap
      // and no end frames are hidden by a crossfade.
      video.classList.remove("is-visible", "is-front");
      next.classList.add("is-visible", "is-front");
      try {
        next.currentTime = 0;
      } catch {
        // Metadata may not be ready yet.
      }
      const attempt = next.play();
      if (attempt && typeof attempt.catch === "function") attempt.catch(() => {});
      heroActive = 1 - heroActive;
      try {
        video.currentTime = 0;
      } catch {
        // Ignore reset failure.
      }
    });
  });
}

function startHeroVideo(videos) {
  if (!videos.length) return;
  bindHeroLoop(videos);
  heroActive = 0;
  heroLooping = true;
  videos.forEach((video, index) => {
    video.pause();
    video.classList.toggle("is-visible", index === heroActive);
    video.classList.toggle("is-front", index === heroActive);
    try {
      video.currentTime = 0;
    } catch {
      // Ignore until metadata is available.
    }
  });
  const attempt = videos[heroActive].play();
  if (attempt && typeof attempt.catch === "function") attempt.catch(() => {});
}

function stopHeroVideo(videos) {
  heroLooping = false;
  videos.forEach((video) => {
    video.pause();
    video.classList.remove("is-visible", "is-front");
    try {
      video.currentTime = 0;
    } catch {
      // Ignore reset failure.
    }
  });
}

function applyHeroMedia(media) {
  const value = HERO_MEDIA.has(media) ? media : "video";
  state.heroMedia = value;
  const select = $("#set-hero-media");
  if (select && select.value !== value) select.value = value;

  // Never start fetching/decoding the hero video while the entrance is still
  // animating: on WebKitGTK that extra compositor work lands on the exact
  // frames the intro is using.
  if (!introDone) {
    pendingHeroMedia = value;
    return value;
  }

  const videos = heroVideos();
  if (!videos.length) return value;

  if (!canPlayHeroVideo()) {
    stopHeroVideo(videos);
    return value;
  }

  loadHeroVideo(videos).then((loaded) => {
    if (!loaded) {
      stopHeroVideo(videos);
      return;
    }
    if (!canPlayHeroVideo()) {
      stopHeroVideo(videos);
      return;
    }
    if (!heroLooping) {
      startHeroVideo(videos);
    } else {
      const attempt = videos[heroActive].play();
      if (attempt && typeof attempt.catch === "function") attempt.catch(() => {});
    }
  });
  return value;
}

function setStatus(kind, label = "需要配置") {
  const chip = $("#setup-chip");
  if (!chip) return;
  const needsSetup = kind !== "ok";
  chip.hidden = !needsSetup;
  chip.dataset.state = kind;
  const copy = $("span", chip);
  if (copy) copy.textContent = label;
  chip.title = needsSetup ? "启动环境未就绪，打开设置" : "";
}

function toast(message, kind = "ok", duration = 3400) {
  const el = $("#toast");
  if (!el) return;
  window.clearTimeout(state.toastTimer);
  el.textContent = message;
  el.classList.toggle("is-error", kind === "error");
  el.classList.add("is-visible");
  state.toastTimer = window.setTimeout(() => {
    el.classList.remove("is-visible");
  }, duration);
}

function renderChecks(checks) {
  const host = $("#settings-checks");
  if (!host) return;
  host.replaceChildren();
  (checks || []).forEach((item) => {
    const row = document.createElement("div");
    row.className = "check-row";
    const dot = document.createElement("i");
    dot.className = "check-dot " + (item.ok ? "is-ok" : "is-bad");
    const copy = document.createElement("div");
    copy.className = "check-copy";
    const strong = document.createElement("strong");
    strong.textContent = item.label;
    const span = document.createElement("span");
    span.textContent = item.detail;
    copy.append(strong, span);
    row.append(dot, copy);
    host.append(row);
  });
}

function renderBootstrap(data) {
  state.bootstrap = data;
  const settings = data.settings || {};

  const ready = isReady(data);
  setStatus(ready ? "ok" : "warn", "需要配置");
  applyTheme(settings.theme || "light");

  const setValue = (id, value) => {
    const el = document.getElementById(id);
    if (el) el.value = value == null ? "" : value;
  };
  setValue("set-install", settings.installRoot || data.installRoot || "");
  setValue("set-bridge", settings.bridgeRoot || data.bridgeRoot || "");
  setValue("set-prefix", settings.prefix || data.prefix || "");
  setValue("set-proton", settings.protonPath || data.protonPath || "");
  setValue("set-mode", settings.launchMode || "background");

  applyReduceMotion(Boolean(settings.reduceMotion));
  applyHeroMedia(settings.heroMedia || "video");

  renderChecks(data.checks);
  text("app-version", data.appVersion ? "v" + data.appVersion : "v0.1.0");
  if (!state.check) {
    setUpdateState(
      "idle",
      "尚未检查",
      data.launcherVersion ? "本地版本 " + data.launcherVersion : "等待检查",
      "i-arrows-clockwise",
    );
  }

  if (data.error) {
    toast(data.error, "error", 6000);
  }
}

function setBusy(elements, busy) {
  elements.filter(Boolean).forEach((button) => {
    button.classList.toggle("is-busy", busy);
    button.disabled = busy;
  });
}

function setUpdateState(kind, title, copy, icon) {
  const card = $("#update-card");
  if (card) card.className = "update-card is-" + kind;
  text("update-state-title", title);
  text("update-state-text", copy);
  const use = $("#update-icon-use");
  if (use) use.setAttribute("href", "#" + icon);
}

function renderCheck(result) {
  state.check = result;
  const pending = (result.missing || 0) + (result.changed || 0);
  const files = result.files || 0;
  const version = result.localVersion || "未知";

  const rows = [];
  (result.missingPreview || []).forEach((path) => rows.push({ tag: "缺失", path }));
  (result.changedPreview || []).forEach((path) => rows.push({ tag: "变更", path }));

  const knownOnly =
    pending === 1 &&
    rows.length === 1 &&
    rows[0].tag === "变更" &&
    rows[0].path.includes("QtQuick/Controls/Styles/Flat/plugins.qmltypes");
  const effectivePending = knownOnly ? 0 : pending;

  text("update-count", effectivePending);

  const homeStatus = $("#home-status");
  if (homeStatus) {
    homeStatus.hidden = effectivePending === 0;
    homeStatus.dataset.state = effectivePending === 0 ? "ok" : "warn";
    if (effectivePending > 0) text("home-status-text", effectivePending + " 个文件待更新");
  }

  const filesBox = $("#update-files");
  const list = $("#update-file-list");

  if (knownOnly) {
    setUpdateState(
      "ok",
      "已是最新",
      version + " · " + files + " 个文件（1 个已知例外）",
      "i-check-circle",
    );
    if (filesBox) filesBox.hidden = true;
    if (list) list.replaceChildren();
    return;
  }

  if (pending === 0) {
    setUpdateState(
      "ok",
      "已是最新",
      version + " · " + files + " 个文件与官方一致",
      "i-check-circle",
    );
    if (filesBox) filesBox.hidden = true;
    if (list) list.replaceChildren();
    return;
  }

  setUpdateState(
    "warn",
    pending + " 个文件待更新",
    version + " · " + files + " 个文件",
    "i-warning-circle",
  );

  if (!list || !filesBox) return;
  filesBox.hidden = false;
  list.replaceChildren();

  rows.forEach((row) => {
    const el = document.createElement("div");
    el.className = "update-file";
    const tag = document.createElement("span");
    tag.className = "update-file__tag";
    tag.textContent = row.tag;
    const path = document.createElement("span");
    path.className = "update-file__path";
    path.textContent = row.path;
    path.title = row.path;
    el.append(tag, path);
    list.append(el);
  });

  if (pending > rows.length) {
    const more = document.createElement("div");
    more.className = "update-file__more";
    more.textContent = "另有 " + (pending - rows.length) + " 个文件未展开";
    list.append(more);
  }
}

async function checkUpdate(full = false, silent = false) {
  if (state.checking) return;
  state.checking = true;
  const buttons = [$("#check-btn"), $("#update-quick"), $("#update-full")];
  setBusy(buttons, true);
  const homeStatus = $("#home-status");
  if (homeStatus) homeStatus.hidden = true;
  const filesBox = $("#update-files");
  if (filesBox) filesBox.hidden = true;
  setUpdateState("checking", "正在检查", "读取官方文件列表…", "i-arrows-clockwise");

  try {
    const result = await invoke("check_update", { full });
    renderCheck(result);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setUpdateState("error", "检查失败", "无法连接官方 CDN 或安装目录无效", "i-warning-circle");
    if (homeStatus) {
      homeStatus.hidden = false;
      homeStatus.dataset.state = "error";
      text("home-status-text", "检查失败，打开更新页");
    }
    if (!silent) toast(message, "error", 6500);
  } finally {
    state.checking = false;
    setBusy(buttons, false);
  }
}

function setLaunchState(next) {
  const copy = LAUNCH_COPY[next];
  if (!copy) return;
  state.game = next;
  const button = $("#launch-btn");
  if (!button) return;
  const label = $(".btn__text", button);
  if (label) label.textContent = copy.label;
  const use = $(".btn__icon use", button);
  if (use) use.setAttribute("href", "#" + copy.icon);
  const busy = next === "launching" || next === "closing";
  button.classList.toggle("is-busy", busy);
  button.classList.toggle("is-running", next === "running");
  button.disabled = busy;
  button.setAttribute("aria-busy", busy ? "true" : "false");
}

/**
 * Apply a backend session snapshot. In-flight launch/close transitions own the
 * button until their command resolves; events only drive idle<->running.
 */
function applyGameStatus(status, { force = false, announce = false } = {}) {
  if (!status || typeof status.running !== "boolean") return;
  const previous = state.game;
  if (!force && (previous === "launching" || previous === "closing")) return;
  if (status.running) {
    state.session = status;
    setLaunchState(status.closing ? "closing" : "running");
    return;
  }
  state.session = null;
  setLaunchState("idle");
  if (announce && previous === "running") toast("游戏已退出。");
}

async function syncGameStatus() {
  if (!isTauri()) return;
  try {
    applyGameStatus(await invoke("game_status"));
  } catch {
    // The launch/close commands remain usable if the status probe fails.
  }
}

async function launchGame() {
  if (state.game !== "idle") return;
  if (!isTauri()) toast("浏览器预览：模拟启动流程。", "ok", 2600);
  setLaunchState("launching");
  const startedAt = performance.now();
  try {
    const result = await invoke("launch_official");
    state.session = result;
    const remaining = LAUNCH_FEEDBACK_MS - (performance.now() - startedAt);
    if (remaining > 0) await wait(remaining);

    // The process may have died during the feedback window. Confirm the
    // session before hiding the window on a launch that no longer exists.
    const status = await invoke("game_status").catch(() => null);
    if (status && !status.running) {
      setLaunchState("idle");
      toast("游戏进程已退出，启动未完成。", "error", 7000);
      return;
    }

    setLaunchState("running");
    if (isTauri()) {
      try {
        await invoke("win_hide");
      } catch {
        // Keeping the window visible is a safe fallback.
      }
    }
  } catch (error) {
    let status = null;
    try {
      status = await invoke("game_status");
    } catch {
      // Fall through to the conservative local state below.
    }
    if (status && status.running) applyGameStatus(status, { force: true });
    else setLaunchState("idle");
    toast(error instanceof Error ? error.message : String(error), "error", 7500);
  }
}

async function closeGame() {
  if (state.game !== "running") return;
  setLaunchState("closing");
  try {
    const status = await invoke("close_game");
    applyGameStatus(status, { force: true });
    toast("游戏已关闭。");
  } catch (error) {
    let status = null;
    try {
      status = await invoke("game_status");
    } catch {
      // Fall through to the conservative local state below.
    }
    if (status) applyGameStatus(status, { force: true });
    else setLaunchState("running");
    toast(error instanceof Error ? error.message : String(error), "error", 7500);
  }
}

function handleLaunchAction() {
  if (state.game === "running") closeGame();
  else launchGame();
}

async function saveSettings(event) {
  event.preventDefault();
  const value = (id) => {
    const el = document.getElementById(id);
    return el ? el.value.trim() : "";
  };
  const settings = {
    installRoot: value("set-install") || null,
    bridgeRoot: value("set-bridge") || null,
    prefix: value("set-prefix") || null,
    protonPath: value("set-proton") || null,
    launchMode: value("set-mode") || "background",
    language: "zh-CN",
    reduceMotion: Boolean($("#set-motion") && $("#set-motion").checked),
    theme: applyTheme(value("set-theme") || "light"),
    heroMedia: applyHeroMedia(value("set-hero-media") || "video"),
  };
  const button = $("#settings-save");
  setBusy([button], true);
  try {
    const data = await invoke("save_settings", { settings });
    renderBootstrap(data);
    applyReduceMotion(settings.reduceMotion);
    toast("设置已保存。");
  } catch (error) {
    toast(error instanceof Error ? error.message : String(error), "error", 6000);
  } finally {
    setBusy([button], false);
  }
}

async function redetect() {
  const button = $("#settings-redetect");
  setBusy([button], true);
  try {
    const data = await invoke("bootstrap");
    renderBootstrap(data);
    toast("已重新检测环境。");
  } catch (error) {
    toast(error instanceof Error ? error.message : String(error), "error", 6000);
  } finally {
    setBusy([button], false);
  }
}

async function openUrl(url) {
  if (!isTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    toast("预览模式：已在新标签页打开。");
    return;
  }
  try {
    await invoke("open_url", { url });
    toast("已在浏览器打开。");
  } catch (error) {
    toast(error instanceof Error ? error.message : String(error), "error", 6000);
  }
}

function hashPage() {
  const value = window.location.hash.replace(/^#/, "");
  return ["home", "update", "bulletin", "settings"].includes(value) ? value : "home";
}

function showPage(name, syncHash = true) {
  if (!["home", "update", "bulletin", "settings"].includes(name)) return;
  state.page = name;
  $$(".rail-item").forEach((item) => {
    const active = item.dataset.page === name;
    item.classList.toggle("is-active", active);
    if (active) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  });
  $$(".page").forEach((page) => {
    page.classList.toggle("is-active", page.id === "page-" + name);
  });
  if (syncHash && window.location.hash !== "#" + name) {
    try {
      history.replaceState(null, "", "#" + name);
    } catch {
      // The Tauri custom protocol can reject history writes on some WebKitGTK builds.
    }
  }
}

function bindInteractions() {
  const blockDrag = (event) => event.preventDefault();
  document.addEventListener("dragstart", blockDrag);
  document.addEventListener("dragover", blockDrag);
  document.addEventListener("drop", blockDrag);
  document.addEventListener("selectstart", (event) => {
    if (event.target instanceof Element && event.target.closest("input, textarea, [contenteditable='true']")) return;
    event.preventDefault();
  });
  document.addEventListener("contextmenu", (event) => {
    if (event.target instanceof Element && event.target.closest("input, textarea, select, [contenteditable='true']")) return;
    event.preventDefault();
  });
  // The game itself has no accessibility layer; do not advertise Tab traversal.
  document.addEventListener("keydown", (event) => {
    if (event.key === "Tab") event.preventDefault();
  });
  document.addEventListener("pointerdown", () => {
    if (!canPlayHeroVideo()) return;
    const videos = heroVideos();
    const video = videos[heroActive];
    if (video && video.src) {
      const attempt = video.play();
      if (attempt && typeof attempt.catch === "function") attempt.catch(() => {});
    }
  }, { once: true });

  $$(".rail-item").forEach((item) => {
    item.addEventListener("click", () => showPage(item.dataset.page));
  });

  $("#launch-btn")?.addEventListener("click", handleLaunchAction);
  $("#check-btn")?.addEventListener("click", () => {
    showPage("update");
    checkUpdate(false);
  });
  $("#home-status")?.addEventListener("click", () => showPage("update"));
  $("#setup-chip")?.addEventListener("click", () => showPage("settings"));
  $("#update-quick")?.addEventListener("click", () => checkUpdate(false));
  $("#update-full")?.addEventListener("click", () => checkUpdate(true));
  $("#settings-form")?.addEventListener("submit", saveSettings);
  $("#settings-redetect")?.addEventListener("click", redetect);
  $("#set-theme")?.addEventListener("change", (event) => applyTheme(event.target.value));
  $("#set-hero-media")?.addEventListener("change", (event) => applyHeroMedia(event.target.value));
  $("#set-motion")?.addEventListener("change", (event) => {
    applyReduceMotion(event.target.checked);
    applyHeroMedia($("#set-hero-media")?.value || "video");
  });

  $("#open-official")?.addEventListener("click", () => openUrl("https://yh.wanmei.com/launcher/launcher_ob2.html?expand=0"));
  $("#open-contract")?.addEventListener("click", () => openUrl("https://www.wanmei.com/safestatic/contract.html"));
  $("#open-privacy")?.addEventListener("click", () => openUrl("https://www.wanmei.com/safestatic/privacy.html"));
  $("#open-support")?.addEventListener("click", () => openUrl("https://games.wanmei.com/m/sdk_kf3/index.html"));

  $$("[data-win]").forEach((button) => {
    button.addEventListener("click", async () => {
      if (!isTauri()) return;
      const action = button.dataset.win;
      const command = action === "minimize" ? "win_minimize" : action === "maximize" ? "win_toggle_maximize" : "win_close";
      try {
        await invoke(command);
      } catch (error) {
        toast(error instanceof Error ? error.message : String(error), "error");
      }
    });
  });

  const titlebar = $("#titlebar");
  if (titlebar) {
    titlebar.addEventListener("mousedown", (event) => {
      if (event.button !== 0) return;
      if (event.target.closest("button, input, select, a")) return;
      if (isTauri()) invoke("win_drag").catch(() => {});
    });
  }

  document.addEventListener("keydown", (event) => {
    if (!event.ctrlKey || event.altKey || event.metaKey) return;
    const index = ["1", "2", "3", "4"].indexOf(event.key);
    if (index >= 0) {
      event.preventDefault();
      showPage(["home", "update", "bulletin", "settings"][index]);
    }
  });

  document.addEventListener("keydown", (event) => {
    if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== "s") return;
    if (state.page !== "settings") return;
    event.preventDefault();
    $("#settings-form")?.requestSubmit();
  });

  window.addEventListener("hashchange", () => showPage(hashPage(), false));

  // Reopening from the tray should always reflect the real session even if an
  // event was missed while the webview was hidden.
  window.addEventListener("focus", () => {
    if (state.game === "launching" || state.game === "closing") return;
    syncGameStatus();
  });
}

async function loadSprite() {
  try {
    const response = await fetch("assets/icons.svg");
    if (!response.ok) return;
    const markup = await response.text();
    const host = document.createElement("div");
    host.setAttribute("aria-hidden", "true");
    host.style.display = "none";
    host.innerHTML = markup;
    document.body.prepend(host);
  } catch {
    // Icons are decorative; the layout remains usable without the sprite.
  }
}

async function watchWindowShape() {
  if (!isTauri() || !window.__TAURI__?.window) return;
  try {
    const win = window.__TAURI__.window.getCurrentWindow();
    const sync = async () => {
      document.body.classList.toggle("is-maximized", await win.isMaximized());
    };
    await sync();
    await win.onResized(sync);
  } catch {
    // The rounded window shape is cosmetic; never block startup for it.
  }
}

async function init() {
  let storedReduceMotion = document.documentElement.classList.contains("reduce-motion");
  try {
    applyTheme(localStorage.getItem("nte.theme") || "light");
    storedReduceMotion =
      storedReduceMotion || localStorage.getItem(REDUCE_MOTION_KEY) === "1";
  } catch {
    applyTheme("light");
  }
  applyReduceMotion(storedReduceMotion);
  bindInteractions();
  showPage(hashPage(), false);
  setLaunchState("idle");
  listenGameState((status) => applyGameStatus(status, { announce: true }));
  await loadSprite();
  watchWindowShape();
  await syncGameStatus();

  try {
    const data = await invoke("bootstrap");
    renderBootstrap(data);
  } catch (error) {
    setStatus("warn", "需要配置");
    toast(error instanceof Error ? error.message : String(error), "error", 6500);
  }

  startIntro();

  if (state.bootstrap && state.bootstrap.ok) {
    whenIntroDone(() => {
      window.setTimeout(() => checkUpdate(false, true), 400);
    });
  }
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init);
} else {
  init();
}
