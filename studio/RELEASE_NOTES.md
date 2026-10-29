# Saya Studio 1.0.0-beta.1 — first Linux beta

Saya Studio is a local desktop app that drives **ComfyUI** for anime-style character images. ComfyUI renders,
Saya Studio organizes: casting, a six-pass workflow, gallery, character sheets, LoRA manager, graph editor.
Nothing runs in the cloud.

This is a **beta**. It is the companion app of the Saya Couple node pack and, like the pack, first a backup of my
own setup. Back up your data and your ComfyUI folder before trying it.

## Requirements

- Linux x86_64 with WebKitGTK 4.1 (Debian 12+, Ubuntu 22.04+, Fedora 39+, openSUSE Tumbleweed / Leap 15.6,
  Arch / EndeavourOS, NixOS).
- A running ComfyUI (default `~/AI/ComfyUI`, on `127.0.0.1:8188`) with the **Saya Couple nodes 2.3.0** or newer
  from this repository (`./saya install --comfyui /path/to/ComfyUI`), plus the models your workflow uses.
- Python 3. Optional: `ffmpeg` for LoRA video previews.

## Install

| Distribution | File | Command |
|---|---|---|
| Debian, Ubuntu | `saya-studio_1.0.0-beta.1_amd64.deb` | `sudo apt install ./saya-studio_1.0.0-beta.1_amd64.deb` |
| Fedora | `saya-studio-1.0.0-beta.1.fedora.x86_64.rpm` | `sudo dnf install ./saya-studio-1.0.0-beta.1.fedora.x86_64.rpm` |
| openSUSE | `saya-studio-1.0.0-beta.1.opensuse.x86_64.rpm` | `sudo zypper install ./saya-studio-1.0.0-beta.1.opensuse.x86_64.rpm` |
| Arch, EndeavourOS | `saya-studio-1.0.0-beta.1-x86_64.pkg.tar.zst` | `sudo pacman -U ./saya-studio-1.0.0-beta.1-x86_64.pkg.tar.zst` |
| Any distribution | `saya-studio_1.0.0-beta.1_amd64.AppImage` | `chmod +x` it, then run it |
| Flatpak | `saya-studio-1.0.0-beta.1-x86_64.flatpak` | `flatpak install --user ./saya-studio-1.0.0-beta.1-x86_64.flatpak` |
| Snap | `saya-studio_1.0.0-beta.1_amd64.snap` | `sudo snap install --dangerous ./saya-studio_1.0.0-beta.1_amd64.snap` |
| Nix | flake in `studio/` | `nix run github:alphaziod/saya-comfy-couple-plus?dir=studio` |

Check your download: `sha256sum -c SHA256SUMS --ignore-missing`.

## What's in 1.0

- **Generation** — open a ComfyUI workflow (UI format), check its faithful API conversion (refused, never
  guessed, when something cannot be resolved), edit inputs, submit, follow progress.
- **Saya preset** — six passes (composition, hires fix, HiDream, post, detailers, final upscale) with memory
  released between passes, one GPU job at a time.
- **Casting** — N characters from a draw engine, pass 1 only, keep the good ones and finish them through the six
  passes; rotations (one character on every action, one action on N characters); action drafts tested in an
  isolated copy of the engine before they are added for real.
- **Gallery** — incremental indexing of your folders (SHA-256, ComfyUI PNG metadata), filters by checkpoint, LoRA
  and character, favourites, review, duplicates; deliveries filed by day and family in `~/Images/saya studio`.
- **Characters** — sheets (identity prompt + anatomy), variants with backgrounds, reference images, import.
- **Models** — LoRA manager with send-to-generation, stack and right-click actions.
- **Workshop** — native ComfyUI graph editor (subgraphs, undo), workflows, pipelines, A/B lab, scenes.
- **Your data stays yours** — SQLite database in `~/.local/share/studio.saya.desktop/`, lasting settings and
  presets in `~/Documents/saya studio config/`, backups with `saya-studio-maintenance backup|verify|restore`.
- 14 interface languages (French by default).

## Known limitations

- **Beta.** Used day to day on Arch (Wayland) with an AMD GPU. The other packages are built and checked by CI but
  have not all been run by hand yet: please report what breaks.
- **Casting needs a draw engine**, which is not distributed; without one, Casting says so and stays unavailable.
- **Flatpak and Snap** run sandboxed: ComfyUI on `127.0.0.1` and your home folder are allowed, but the browser
  extension bridge may not work there.
- The bundled preset ships with **neutral example prompts and empty LoRA stacks**: fill in your own. Its model
  settings are mine; keep the ones that work on your machine.
- The app can be used to make adult images of adult characters; its store metadata says so.

## Also in this repository

**Saya Couple 2.3.0** (the node pack, `./saya install`): one ownership anchor per aspect of the genital anatomy
block, Saya Family Save node, `saya_role` PNG chunk on phase images, softer background picker. See
[CHANGELOG.md](https://github.com/alphaziod/saya-comfy-couple-plus/blob/main/CHANGELOG.md).

## Documentation

[README](https://github.com/alphaziod/saya-comfy-couple-plus/blob/main/studio/README.md) ·
[Packaging](https://github.com/alphaziod/saya-comfy-couple-plus/blob/main/studio/docs/PACKAGING.md) ·
[For AI agents and contributors](https://github.com/alphaziod/saya-comfy-couple-plus/blob/main/studio/AGENTS.md)

License: GPL-3.0-or-later.
