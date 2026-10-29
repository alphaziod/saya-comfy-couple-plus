{
  # UNTESTED: written without Nix available. See packaging/nix/package.nix for the hash to fill.
  description = "Saya Studio: local image, character and ComfyUI workflow manager";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      packages.${system} = rec {
        saya-studio = pkgs.callPackage ./packaging/nix/package.nix { };
        default = saya-studio;
      };

      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          pkg-config
          rustc
          cargo
          clippy
          rustfmt
          nodejs_22
          wrapGAppsHook3
          python3
        ];
        buildInputs = with pkgs; [
          webkitgtk_4_1
          gtk3
          libsoup_3
          glib-networking
          openssl
          librsvg
        ];
        shellHook = ''
          export GIO_EXTRA_MODULES="${pkgs.glib-networking}/lib/gio/modules:$GIO_EXTRA_MODULES"
          echo "Saya Studio dev shell: npm ci && npm run tauri -- dev"
        '';
      };
    };
}
