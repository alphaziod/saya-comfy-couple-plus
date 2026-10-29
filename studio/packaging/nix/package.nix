# Saya Studio, Nix derivation. UNTESTED: written without a Nix installation available.
# Build (from studio/):  nix build .#default
# The npm dependency hash below must change with package-lock.json: set it to lib.fakeHash, build,
# and paste the "got: sha256-..." the failure prints. Cargo dependencies use Cargo.lock directly
# (no cargoHash needed).
{
  lib,
  rustPlatform,
  fetchNpmDeps,
  npmHooks,
  nodejs_22,
  pkg-config,
  wrapGAppsHook3,
  webkitgtk_4_1,
  gtk3,
  libsoup_3,
  glib-networking,
  openssl,
  librsvg,
  python3,
  curl,
  xdg-utils,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "saya-studio";
  version = "1.0.0-beta.1";

  src = lib.cleanSource ../..; # studio/

  cargoLock.lockFile = ../../Cargo.lock;

  npmDeps = fetchNpmDeps {
    inherit (finalAttrs) src;
    # From the first CI build; update it whenever package-lock.json changes.
    hash = "sha256-Dgd7ckDUDto9aLCfoXbwrxyywmQ4Gy+DwrdV391AbjE=";
  };

  nativeBuildInputs = [
    pkg-config
    wrapGAppsHook3
    nodejs_22
    npmHooks.npmConfigHook
  ];

  buildInputs = [
    webkitgtk_4_1
    gtk3
    libsoup_3
    glib-networking # TLS for WebKitGTK / GIO
    openssl
    librsvg
  ];

  # The Tauri crate embeds ../dist (frontendDist): build the frontend first.
  # `vite build` only: `npm run build` also runs svelte-check, which the CI already does.
  preBuild = ''
    npx vite build
  '';

  # `custom-protocol` makes the binary serve the embedded frontend instead of devUrl.
  buildFeatures = [ "custom-protocol" ];
  cargoBuildFlags = [
    "--package" "saya-studio"
    "--package" "studio-core"
  ];

  # Tests run in CI (cargo test -p studio-core, node --test); some need a display or Python.
  doCheck = false;

  postInstall = ''
    mv $out/bin/studio-maintenance $out/bin/saya-studio-maintenance

    share=$out/share/saya-studio
    install -Dm755 -t $share scripts/civitai_bridge.py scripts/saya_native_host.py
    install -Dm755 sidecars/f09-semantic-embed.py $share/sidecars/f09-semantic-embed.py
    cp -r presets $share/presets
    cp -r extension $share/extension
    chmod -R u+w $share
    patchShebangs $share

    install -Dm644 packaging/linux/saya-studio.desktop $out/share/applications/saya-studio.desktop
    for s in 16 24 32 48 64 128 256 512; do
      install -Dm644 packaging/arch/icons/saya-studio-$s.png \
        $out/share/icons/hicolor/''${s}x$s/apps/saya-studio.png
    done
    install -Dm644 packaging/linux/studio.saya.desktop.metainfo.xml \
      $out/share/metainfo/studio.saya.desktop.metainfo.xml
  '';

  # The app hardcodes /usr/share/saya-studio; point it at the store path through the
  # environment variables it honours, and give it its runtime tools.
  preFixup = ''
    gappsWrapperArgs+=(
      --set-default SAYA_STUDIO_PRESETS $out/share/saya-studio/presets
      --set-default SAYA_PASSES_DIR $out/share/saya-studio/presets/passes
      --set-default SAYA_CIVITAI_BRIDGE $out/share/saya-studio/civitai_bridge.py
      --set-default SAYA_NATIVE_HOST $out/share/saya-studio/saya_native_host.py
      --set-default SAYA_SEMANTIC_SCRIPT $out/share/saya-studio/sidecars/f09-semantic-embed.py
      --prefix PATH : ${lib.makeBinPath [ python3 curl xdg-utils ]}
    )
  '';

  meta = {
    description = "Local image, character and ComfyUI workflow manager";
    homepage = "https://github.com/alphaziod/saya-comfy-couple-plus";
    license = lib.licenses.gpl3Plus;
    mainProgram = "saya-studio";
    platforms = [ "x86_64-linux" ];
  };
})
