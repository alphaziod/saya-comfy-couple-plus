# Packaging

Release: **Saya Studio 1.0.0-beta.1 (Linux beta)**, tag `studio-v1.0.0-beta.1`, built by
`.github/workflows/studio-release.yml` (draft prerelease on GitHub). Formats: deb, Fedora rpm,
openSUSE rpm, Arch, AUR, Nix, AppImage, Flatpak, Snap. Everything below the Arch section was
written without being run locally; see "Not tested" at the end.

## Arch Linux

End-user installation notes stay in `INSTALLATION-ARCH.md` and `SAUVEGARDES.md` (French,
shipped in `/usr/share/doc/saya-studio/`). This page is for developers.

## Build

```bash
bash scripts/build-arch-worktree.sh
```

- Packs the **current files** of the worktree (tracked + untracked, not ignored; uncommitted
  changes included, nothing committed) into `test-results/arch-package-worktree-<stamp>-XXXX/`.
- Version `0.2.0.dev.wt<stamp>`; `SOURCE-COMMIT` records `HEAD` plus a "worktree" note.
- Runs `makepkg` with `packaging/arch/PKGBUILD`: offline `npm ci`, `tauri build --no-bundle`,
  the `studio-maintenance` binary, then `check()` (Rust tests, JS tests, subgraph test,
  desktop file validation).
- `RUSTFLAGS --remap-path-prefix` keeps build-machine paths out of the binaries.

`scripts/build-arch.sh` does the same from a committed Git snapshot.

## Verify (no install)

```bash
python3 scripts/verify-arch-package.py test-results/arch-package-worktree-*/saya-studio-*.pkg.tar.zst
```

Checks the exact file list against `package()`, ownership and modes, no `/home/` string, no
smoke-test instrumentation (`smoke_capture`, `finish_smoke`), the desktop file, the 512×512 icon
and declared dependencies versus linked libraries.

## Installed layout

| Path | Content |
|---|---|
| `/usr/bin/saya-studio`, `/usr/bin/saya-studio-maintenance` | App and maintenance CLI |
| `/usr/share/applications/saya-studio.desktop` | Launcher |
| `/usr/share/icons/hicolor/<size>/apps/saya-studio.png` | Icon, 16 to 512 px (`packaging/arch/icons/`, generated from `src-tauri/icons/icon.png`; regenerate them when the artwork changes) |
| `/usr/share/saya-studio/presets/` (+ `passes/`, `outils/`) | Base preset and six-pass workflow, copied into ComfyUI at startup |
| `/usr/share/saya-studio/*.py`, `sidecars/` | Civitai bridge, native messaging host, optional embedding sidecar |
| `/etc/{opt/chrome,chromium}/native-messaging-hosts/com.saya.studio.json` | System native messaging manifests |

`saya-studio.install` only prints messages: pacman never touches home directories, so the
user-level native messaging manifests and `~/.local/share/studio.saya.desktop` stay after removal.

## Install and smoke the installed app

```bash
sudo pacman -U test-results/arch-package-worktree-<stamp>-XXXX/saya-studio-<version>-1-x86_64.pkg.tar.zst
```

Then start `/usr/bin/saya-studio` with an isolated `XDG_DATA_HOME`/`XDG_CONFIG_HOME` to check
it opens, migrates a fresh database and shows the main screens without touching real data.
Roll back with `sudo pacman -U` of the previous package from the pacman cache, or `sudo pacman -R saya-studio`.

`pkgver` in `packaging/arch/PKGBUILD` is `1.0.0beta1` (pacman has no hyphen in versions; `1.0.0beta1`
sorts before `1.0.0`). `build-arch*.sh` overwrite it with their dev version, so they are unaffected.
`verify-arch-package.py` still expects `license = unknown`, so the PKGBUILD `license` is unchanged.

## Version and dependency names

| | Debian 12+ / Ubuntu 22.04+ | Fedora | openSUSE | Arch |
|---|---|---|---|---|
| WebKitGTK | `libwebkit2gtk-4.1-0` | `webkit2gtk4.1` | `libwebkit2gtk-4_1-0` | `webkit2gtk-4.1` |
| GTK 3 | `libgtk-3-0` | `gtk3` | `libgtk-3-0` | `gtk3` |
| libsoup 3 | `libsoup-3.0-0` | `libsoup3` | `libsoup-3_0-0` | `libsoup3` |
| Others | `python3 curl xdg-utils xdg-desktop-portal` | same | same | `python curl xdg-utils xdg-desktop-portal` |

All come from `bundle.linux.{deb,rpm}.depends` in `src-tauri/tauri.conf.json`; the openSUSE names
are the only thing `packaging/rpm/tauri.opensuse.conf.json` overrides. Optional: `ffmpeg`
(LoRA video previews), a portal backend (`xdg-desktop-portal-gtk`/`-gnome`/`-kde`).

`src-tauri/tauri.conf.json` and `Cargo.toml`/`package.json` still say `0.1.0`. CI passes
`--config '{"version":"1.0.0-beta.1"}'` to Tauri so the bundles carry the right version; bump the
files themselves before tagging if you want the binary's own version to match.

## deb / Fedora rpm / openSUSE rpm / AppImage (Tauri bundler)

Build (needs the Tauri Linux build libraries, see AGENTS.md section 2, plus `rpm` and `patchelf`):

```bash
npm ci
cargo build --release -p studio-core --bin studio-maintenance --locked   # bundled as /usr/bin/saya-studio-maintenance
npx tauri build --bundles deb,rpm,appimage
npx tauri build --bundles rpm --config packaging/rpm/tauri.opensuse.conf.json   # openSUSE
```

Output: `target/release/bundle/{deb,rpm,appimage}/`. Install: `sudo apt install ./saya-studio_*.deb`,
`sudo dnf install ./saya-studio-*.fedora.x86_64.rpm`, `sudo zypper install ./saya-studio-*.opensuse.x86_64.rpm`,
or `chmod +x *.AppImage && ./*.AppImage`.

`bundle.linux.{deb,rpm}.files` reproduces the Arch layout: maintenance CLI, `civitai_bridge.py`,
`saya_native_host.py`, the sidecar, `presets/`, `extension/`, and the native messaging manifest
(`packaging/linux/com.saya.studio.json`) under `/usr/share/saya-studio`, `/etc/opt/chrome` and `/etc/chromium`.
Limits: the build fails if `target/release/studio-maintenance` was not built first; Tauri generates
its own `.desktop` file and hicolor icons from `icons/icon.png` (single size) instead of the
8-size set in `packaging/arch/icons/`; there is no `.install` message about leftover user files.
**AppImage**: the app looks for `/usr/share/saya-studio/...` (hardcoded in Rust), which does not exist
inside an AppImage, so the base presets, Civitai bridge and native host are not found unless
`SAYA_STUDIO_PRESETS`, `SAYA_PASSES_DIR`, `SAYA_CIVITAI_BRIDGE`, `SAYA_NATIVE_HOST` are set. Fixing it
needs an `AppRun` wrapper or a Rust fallback relative to the executable (not done: no code changes allowed for this task).

## AUR (`saya-studio-bin`)

`packaging/aur/PKGBUILD` repackages the release `.deb`
(`.../releases/download/studio-v1.0.0-beta.1/saya-studio_1.0.0-beta.1_amd64.deb`). Checksums are
`SKIP` until the release exists: then run `updpkgsums && makepkg --printsrcinfo > .SRCINFO`, test with
`makepkg -si`, and push both files to the AUR repository. It conflicts with `saya-studio` (the source package).

## Nix

`flake.nix` (in `studio/`) exposes `packages.x86_64-linux.default` and a `devShell`;
`packaging/nix/package.nix` is the derivation (`buildRustPackage` + `fetchNpmDeps`, `wrapGAppsHook3`,
WebKitGTK 4.1, GTK 3, libsoup 3, glib-networking; Python, curl and xdg-utils on `PATH`; the
`SAYA_*` environment variables point the app at the store paths).

```bash
cd studio && nix build .#default     # first build fails: copy "got: sha256-..." into npmDeps.hash
nix develop                          # dev shell
```

Cargo dependencies use `Cargo.lock` directly. `flake.lock` is not committed: run `nix flake lock`.

## Flatpak

`packaging/flatpak/studio.saya.desktop.yml` (GNOME 48 runtime, `rust-stable` and `node22` extensions).
Permissions: network (ComfyUI on 127.0.0.1:8188), Wayland + X11 fallback, DRI, and full home access
(images, `~/AI/ComfyUI`, config dir). Data files go to `/app/share/saya-studio` and
`saya-studio-launcher.sh` exports the `SAYA_*` variables.

```bash
flatpak-builder --user --install --force-clean build-dir packaging/flatpak/studio.saya.desktop.yml
flatpak run studio.saya.desktop
```

Limits: the build uses the network (not accepted by Flathub: generate `cargo-sources.json` and
`node-sources.json` first); ComfyUI must be started on the host; `xdg-open` and the native messaging
manifests written under `~/.config` for Brave/Chrome point to a script inside the sandbox path
(the browser extension bridge is unlikely to work from Flatpak).
AppStream metadata: `packaging/linux/studio.saya.desktop.metainfo.xml` (OARS 1.1 `sex-nudity` and
`sex-themes` set to `moderate`: **the owner must confirm**).

## Snap

`packaging/snap/snapcraft.yaml` (core24, GNOME extension, **strict** confinement with plugs
`home network network-bind desktop desktop-legacy wayland x11 opengl removable-media`). Strict was
chosen because the app only needs the home directory and the network; switch to `classic` if real
use needs more (it requires store approval).

```bash
mkdir -p snap && cp packaging/snap/snapcraft.yaml snap/ && snapcraft
sudo snap install --dangerous ./saya-studio_*.snap
```

Limits: the Chrome/Brave native messaging bridge cannot reach a confined script; the snap `home`
plug excludes hidden directories of other apps.

## CI (`.github/workflows/studio-release.yml`)

Trigger: tag `studio-v*` or manual run. Jobs: bundles (tests, deb, Fedora rpm, AppImage), openSUSE rpm,
Arch (makepkg in an `archlinux` container as non-root, then `verify-arch-package.py`), Nix, Flatpak, Snap,
then a **draft prerelease** with `SHA256SUMS`. Nix, Flatpak and Snap are `continue-on-error` (untested);
the release is produced when bundles, openSUSE rpm and Arch succeed. Only `GITHUB_TOKEN` is used.

## Not tested locally

No bundle was built and no workflow was run for this section. Checked statically only: JSON syntax,
Tauri config keys against the Tauri 2.12 `config.schema.json`, YAML syntax, AppStream (`appstreamcli
validate --no-net`), `.desktop` (`desktop-file-validate`), AUR `.SRCINFO` (`makepkg --printsrcinfo`).
Untested: the Tauri bundles (directory entries in `files`, `1.0.0-beta.1` as an rpm version, two
`--config` flags), the Arch CI job (offline `npm ci`/cargo from a prefetched cache), Nix (no `nix` installed),
Flatpak, Snap, the AUR package, and all of the action pins in the workflow.
