const { app } = require("electron");
const { writeFileSync } = require("node:fs");
const { spawn } = require("node:child_process");
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const samples = [];
app.once("browser-window-created", (_, win) => {
  win.webContents.once("did-finish-load", () => {
    void (async () => {
      async function sample(stage, released) {
        let state;
        for (let attempt = 0; attempt < 100; attempt++) {
          state = await win.webContents.executeJavaScript(`({
            hidden:document.hidden,
            videos:[...document.querySelectorAll('video.hero__source')].map(v=>({src:v.getAttribute('src'),ready:v.readyState,paused:v.paused,time:v.currentTime})),
          })`);
          if (
            released
              ? state.videos.every((v) => v.src === null && v.ready === 0)
              : state.videos.some((v) => !v.paused && v.time > 0.1)
          )
            break;
          await sleep(100);
        }
        samples.push({
          stage,
          ...state,
          visible: (
            await win.webContents.executeJavaScript(
              "window.oddity.windowState()",
            )
          ).visible,
        });
      }
      for (let attempt = 0; !win.isVisible() && attempt < 100; attempt++)
        await sleep(100);
      await sample("playing", false);
      win.hide();
      await sample("hidden", true);
      win.show();
      await sample("visible", false);
      await win.webContents.executeJavaScript(
        "document.querySelector('[data-win=minimize]').click()",
      );
      await sample("minimized", true);
      const wake = spawn(process.execPath, [__filename], {
        env: process.env,
        stdio: "ignore",
        timeout: 5000,
        killSignal: "SIGKILL",
      });
      await new Promise((resolve, reject) => {
        wake.on("exit", resolve);
        wake.on("error", reject);
      });
      await sample("restored", false);
      await win.webContents.executeJavaScript("location.hash='settings'");
      await sleep(700);
      await win.webContents.executeJavaScript(
        "document.querySelector('#set-motion').click()",
      );
      await sample("reduce-motion", true);
      await win.webContents.executeJavaScript(
        "document.querySelector('#set-motion').click();location.hash='home'",
      );
      await sample("motion-restored", false);
      writeFileSync(process.env.ODDITY_TEST_REPORT, JSON.stringify(samples));
      app.quit();
    })().catch((error) => {
      console.error(error);
      app.exit(1);
    });
  });
});
require(process.env.ODDITY_TEST_MAIN);
