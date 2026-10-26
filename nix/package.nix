{
  lib,
  stdenv,
  rustPlatform,
  fetchPnpmDeps,
  pnpmConfigHook,
  pnpm,
  nodejs,
  pkg-config,
  wrapGAppsHook3,
  webkitgtk_4_1,
  gtk3,
  glib-networking,
  libsecret,
  libayatana-appindicator,
  librsvg,
  gdk-pixbuf,
  libsoup_3,
  xdg-utils,
  umu-launcher,
  gst_all_1,
  openssl,
  dbus,
  xz,
  zlib,
  src,
}:

let
  version = (builtins.fromTOML (builtins.readFile (src + "/launcher/Cargo.toml"))).workspace.package.version;
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
    "nte-app"
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
    hash = "sha256-L8rBAaUGx6VNqoQf+jMrgSalSty6gW0FNnIDdocONBQ=";
  };

  nativeBuildInputs = [
    pnpmConfigHook
    pnpm
    nodejs
    pkg-config
    wrapGAppsHook3
  ];

  buildInputs = [
    webkitgtk_4_1
    gtk3
    glib-networking
    libsecret
    libayatana-appindicator
    librsvg
    gdk-pixbuf
    libsoup_3
    xdg-utils
    umu-launcher
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-libav
    openssl
    dbus
    xz
    zlib
  ];

  preBuild = ''
    (cd launcher/ui && pnpm run build)
  '';

  postPatch = lib.optionalString stdenv.hostPlatform.isLinux ''
    find "$cargoDepsCopy" -type f -path '*/libappindicator-sys-*/src/lib.rs' -exec \
      substituteInPlace {} --replace-fail "libayatana-appindicator3.so.1" "${libayatana-appindicator}/lib/libayatana-appindicator3.so.1" \;
  '';

  preFixup = ''
    gappsWrapperArgs+=(
      --prefix PATH : ${lib.makeBinPath [ xdg-utils umu-launcher ]}
      --set-default WEBKIT_GST_ALLOWED_URI_PROTOCOLS asset
      --set-default __NV_DISABLE_EXPLICIT_SYNC 1
      --prefix GST_PLUGIN_SYSTEM_PATH_1_0 : ${lib.makeSearchPath "lib/gstreamer-1.0" [
        gst_all_1.gst-plugins-base
        gst_all_1.gst-plugins-good
        gst_all_1.gst-libav
      ]}
    )
  '';

  installPhase = ''
    runHook preInstall

    target="$(find . -type f -path '*/target/*/release/oddity' -print -quit)"
    if [ -z "$target" ]; then
      target="$(find . -type f -path '*/target/release/oddity' -print -quit)"
    fi
    test -n "$target"
    target="$(dirname "$target")"

    install -Dm755 "$target/oddity" "$out/bin/oddity"
    install -Dm755 "$target/nte-cli" "$out/bin/nte-cli"
    install -Dm755 "$target/oddity-recovery" "$out/bin/oddity-recovery"
    install -Dm644 launcher/packaging/oddity-recovery.desktop "$out/share/applications/oddity-recovery.desktop"
    install -Dm644 launcher/packaging/oddity.desktop "$out/share/applications/oddity.desktop"

    for size in 16 22 24 32 48 64 128 256 512; do
      install -Dm644 "launcher/crates/nte-app/icons/''${size}x''${size}.png" \
        "$out/share/icons/hicolor/''${size}x''${size}/apps/oddity.png"
    done

    install -Dm644 LICENSE "$out/share/licenses/oddity/LICENSE"
    install -Dm644 THIRD_PARTY.md "$out/share/licenses/oddity/THIRD_PARTY.md"
    install -Dm644 README.md "$out/share/doc/oddity/README.md"

    runHook postInstall
  '';

  doCheck = false;

  meta = {
    description = "Native Linux launcher for Neverness To Everness";
    homepage = "https://github.com/LaT-SKY/Oddity";
    license = lib.licenses.gpl3Plus;
    platforms = lib.platforms.linux;
    mainProgram = "oddity";
  };
})
