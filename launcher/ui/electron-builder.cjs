const { resolve, dirname } = require("node:path");
const version = require("./package.json").version;

const extra = [
  `${resolve("../packaging/nte-cli")}=/usr/bin/nte-cli`,
  `${resolve("../packaging/oddity-recovery")}=/usr/bin/oddity-recovery`,
  `${resolve("../packaging/oddity-recovery.desktop")}=/usr/share/applications/oddity-recovery.desktop`,
  `${resolve("../../LICENSE")}=/usr/share/licenses/oddity/LICENSE`,
  `${resolve("../../THIRD_PARTY.md")}=/usr/share/licenses/oddity/THIRD_PARTY.md`,
];
module.exports = async () => {
  const { rpmMetadata } = await import("../../scripts/release-version.mjs");
  const rpm = rpmMetadata(version);
  return {
    appId: "io.github.oddity.Launcher",
    productName: "Oddity",
    executableName: "oddity",
    electronVersion: "44.10.0",
    electronDist: dirname(require("electron")),
    asar: true,
    npmRebuild: false,
    directories: {
      output: "../../dist/electron",
      buildResources: "../crates/nte-service/icons",
    },
    files: ["dist/**", "desktop-dist/**", "package.json", "!node_modules/**/*"],
    extraResources: [
      {
        from: "../target/release",
        to: "bin",
        filter: ["oddity-backend", "nte-cli", "oddity-recovery"],
      },
      { from: "../crates/nte-service/icons", to: "icons", filter: ["*.png"] },
    ],
    extraFiles: [
      { from: "../../LICENSE", to: "LICENSE.oddity" },
      { from: "../../THIRD_PARTY.md", to: "THIRD_PARTY.md" },
      { from: "../../README.md", to: "README.md" },
      { from: "../crates/nte-service/icons", to: "icons", filter: ["*.png"] },
      { from: "../packaging/oddity.desktop", to: "oddity.desktop" },
      {
        from: "../packaging/oddity-recovery.desktop",
        to: "oddity-recovery.desktop",
      },
    ],
    afterPack: "./electron/after-pack.cjs",
    linux: {
      target: ["deb", "rpm", "AppImage"],
      category: "Game",
      icon: "../crates/nte-service/icons",
      syncDesktopName: true,
      desktop: {
        entry: {
          Name: "Oddity",
          TryExec: "oddity",
          StartupWMClass: "oddity",
          Categories: "Game;Utility;",
        },
        desktopActions: {
          LaunchCn: {
            Name: "异环",
            Exec: "oddity --launch --region cn --launch-mode background",
          },
          LaunchGlobal: {
            Name: "NTE",
            Exec: "oddity --launch --region global --launch-mode background",
          },
          Recovery: { Name: "应急恢复", Exec: "oddity --recovery" },
        },
      },
    },
    deb: {
      appArmorProfile: resolve("electron/oddity.apparmor"),
      artifactName: `oddity_${version}_amd64.deb`,
      depends: [
        "libgtk-3-0 | libgtk-3-0t64",
        "libcups2 | libcups2t64",
        "libnss3",
        "libxss1",
        "libxtst6",
        "libatspi2.0-0 | libatspi2.0-0t64",
        "libdrm2",
        "libgbm1",
        "libsecret-1-0",
        "libudev1",
        "libpulse0",
        "libasound2 | libasound2t64",
        "xdg-utils",
      ],
      recommends: ["umu-launcher"],
      fpm: extra,
    },
    rpm: {
      appArmorProfile: resolve("electron/oddity.apparmor"),
      artifactName: `oddity-${rpm.version}-${rpm.release}.x86_64.rpm`,
      depends: [
        "gtk3",
        "nss",
        "libXScrnSaver",
        "libXtst",
        "at-spi2-core",
        "libdrm",
        "mesa-libgbm",
        "libsecret-1.so.0()(64bit)",
        "libudev.so.1()(64bit)",
        "libpulse.so.0()(64bit)",
        "alsa-lib",
        "xdg-utils",
      ],
      fpm: ["--version", rpm.version, "--iteration", rpm.release, ...extra],
    },
    appImage: { artifactName: `oddity-${version}-x86_64.AppImage` },
  };
};
