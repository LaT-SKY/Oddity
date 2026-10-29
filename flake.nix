{
  description = "Native Linux launcher for Neverness To Everness";

  nixConfig = {
    extra-substituters = [ "https://oddity.cachix.org" ];
    extra-trusted-public-keys = [
      "oddity.cachix.org-1:N4amj+fz8vXAJuUK5yLMAZOzX7giHeZ7wgEVfcjkDwI="
    ];
  };

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f (import nixpkgs { inherit system; }));
      oddityFor = pkgs: pkgs.callPackage ./nix/package.nix { src = self.outPath; };
    in
    {
      packages = forAllSystems (pkgs: {
        default = oddityFor pkgs;
        oddity = oddityFor pkgs;
      });

      overlays.default = final: _prev: {
        oddity = final.callPackage ./nix/package.nix { src = self.outPath; };
      };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            pkg-config
            cmake
            udev
            alsa-lib
            libpulseaudio
            openssl
            nodejs
            pnpm
            webkitgtk_4_1
            gtk3
            libayatana-appindicator
            libsecret
          ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt-rfc-style);
    };
}
