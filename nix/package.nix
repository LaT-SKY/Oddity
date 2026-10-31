{
  lib,
  rustPlatform,
  fetchPnpmDeps,
  pnpmConfigHook,
  pnpm,
  nodejs,
  pkg-config,
  cmake,
  udev,
  alsa-lib,
  libpulseaudio,
  wrapGAppsHook3,
  makeWrapper,
  electron_44,
  gtk3,
  libsecret,
  xdg-utils,
  umu-launcher,
  openssl,
  dbus,
  xz,
  zlib,
  src,
}:
let
  version =
    (builtins.fromTOML (builtins.readFile (src + "/launcher/Cargo.toml"))).workspace.package.version;
in
rustPlatform.buildRustPackage (finalAttrs: {
  pname = "oddity";
  inherit version src;
  cargoRoot = "launcher";
  buildAndTestSubdir = "launcher";
  cargoLock = {
    lockFile = src + "/launcher/Cargo.lock";
    outputHashes = {
      "hdiffpatch-rs-2.0.0" = "sha256-o5OHJPQTeW0traOIY2Xx37OZQC/gFOf0xTksp3Zhdig=";
    };
  };
  cargoBuildFlags = [
    "-p"
    "nte-service"
    "-p"
    "nte-cli"
    "-p"
    "nte-recovery"
  ];
  pnpmRoot = "launcher/ui";
  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs) pname version;
    src = ./../launcher/ui;
    inherit pnpm;
    fetcherVersion = 5;
    hash = "sha256-INFSHvBueZapTtg6Uo4lVjVSQUoYeoI5INvJl9ioSro=";
  };
  nativeBuildInputs = [
    pnpmConfigHook
    pnpm
    nodejs
    pkg-config
    cmake
    wrapGAppsHook3
    makeWrapper
  ];
  buildInputs = [
    udev
    alsa-lib
    libpulseaudio
    gtk3
    libsecret
    openssl
    dbus
    xz
    zlib
  ];
  preBuild = ''
    (cd launcher/ui && pnpm run desktop:build)
  '';
  installPhase = ''
    runHook preInstall
    target="$(find . -type f -path '*/release/oddity-backend' -print -quit)"
    test -n "$target"
    target="$(dirname "$target")"
    install -dm755 "$out/lib/oddity/resources/bin" "$out/bin"
    for binary in oddity-backend nte-cli oddity-recovery; do
      install -m755 "$target/$binary" "$out/lib/oddity/resources/bin/$binary"
    done
    cp -r launcher/ui/dist launcher/ui/desktop-dist "$out/lib/oddity/"
    cp launcher/ui/package.json "$out/lib/oddity/"
    install -Dm755 launcher/packaging/oddity "$out/lib/oddity/oddity"
    makeWrapper ${electron_44}/bin/electron "$out/lib/oddity/oddity-shell" \
      --add-flags "$out/lib/oddity" \
      --set ODDITY_RESOURCES "$out/lib/oddity/resources" \
      --prefix PATH : ${
        lib.makeBinPath [
          xdg-utils
          umu-launcher
        ]
      } \
      --prefix LD_LIBRARY_PATH : ${
        lib.makeLibraryPath [
          udev
          alsa-lib
          libpulseaudio
        ]
      }
    # 系统 Electron 使用应用目录定位资源, 辅助程序保持同目录契约.
    makeWrapper "$out/lib/oddity/oddity" "$out/bin/oddity"
    makeWrapper "$out/lib/oddity/oddity" "$out/lib/oddity/resources/bin/oddity"
    for binary in nte-cli oddity-recovery; do
      ln -s "$out/lib/oddity/resources/bin/$binary" "$out/bin/$binary"
    done
    for size in 16 22 24 32 48 64 128 256 512; do
      install -Dm644 "launcher/crates/nte-service/icons/''${size}x''${size}.png" \
        "$out/share/icons/hicolor/''${size}x''${size}/apps/oddity.png"
    done
    cp -r launcher/crates/nte-service/icons "$out/lib/oddity/resources/icons"
    install -Dm644 launcher/packaging/oddity.desktop "$out/share/applications/oddity.desktop"
    install -Dm644 launcher/packaging/oddity-recovery.desktop "$out/share/applications/oddity-recovery.desktop"
    install -Dm644 LICENSE "$out/share/licenses/oddity/LICENSE"
    install -Dm644 THIRD_PARTY.md "$out/share/licenses/oddity/THIRD_PARTY.md"
    install -Dm644 README.md "$out/share/doc/oddity/README.md"
    runHook postInstall
  '';
  preFixup = ''
    gappsWrapperArgs+=(--prefix PATH : ${
      lib.makeBinPath [
        xdg-utils
        umu-launcher
      ]
    })
  '';
  # Rust 测试由独立质量检查执行, 构建沙箱不提供桌面与发行版信息.
  doCheck = false;
  meta = {
    description = "Linux launcher for Neverness To Everness";
    homepage = "https://github.com/LaT-SKY/Oddity";
    license = lib.licenses.gpl3Plus;
    platforms = [ "x86_64-linux" ];
    mainProgram = "oddity";
  };
})
