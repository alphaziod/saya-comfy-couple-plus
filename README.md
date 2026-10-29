# Saya

> I wanted two characters and a background to stop fighting each other. This got slightly out of hand.

Two things live in this repository, both local, both built around [ComfyUI](https://github.com/comfyanonymous/ComfyUI):

| | What it is | Version |
|---|---|---|
| **[Saya Couple](docs/SAYA_COUPLE.md)** | A ComfyUI custom node pack: separate prompts for the scene (MAIN), the shared action (ACTION) and each character (P1, P2) that actually work together, or a Solo mode with one character. | **2.3.0** |
| **[Saya Studio](studio/README.md)** | A Linux desktop app that drives ComfyUI and these nodes: casting, a six-pass workflow, gallery, character sheets, LoRA manager, graph editor. | **1.0.0-beta.1** (beta) |

| Couple mode | Solo mode |
|:---:|:---:|
| ![Couple: two characters, one scene](docs/images/demo_example.png) | ![Solo: one character, same scene](docs/images/solo_example.png) |

It is first and foremost **a backup of my own setup**, made public in case it helps someone with the same
problem. Take what you need. **Back up your ComfyUI folder before installing anything, and use at your own risk.**

---

## Saya Couple (node pack)

Tested on ComfyUI `41db8f4f` (v0.34.0+77), Linux, AMD RDNA4 16 GB. Since 2.0 it does **not modify any ComfyUI
file**: the couple engine is attached to the model through ComfyUI's official patching API, for the sampling only.

```bash
git clone https://github.com/alphaziod/saya-comfy-couple-plus saya-couple
cd saya-couple
./saya install --comfyui /path/to/ComfyUI     # Windows: saya.bat install --comfyui C:\path\to\ComfyUI
```

Needs [MultiMaskCouple](THIRD_PARTY_NOTICES.md) (and [RES4LYF](THIRD_PARTY_NOTICES.md) for the demo samplers).
Restart ComfyUI, open `workflows/Saya_Couple_Demo.json`, pick your SDXL / Illustrious model, run.
`./saya verify` tells you what is wrong, `./saya restore` puts everything back.

- Full reference (modes, workflows, how it works, installer, AMD VRAM patch, compatibility, limitations):
  [docs/SAYA_COUPLE.md](docs/SAYA_COUPLE.md)
- Writing prompts: [English](docs/PROMPTS_GUIDE_EN.md) · [Français](docs/PROMPTS_GUIDE_FR.md)
- What changed: [CHANGELOG.md](CHANGELOG.md)

## Saya Studio (desktop app, Linux beta)

A local app (Tauri 2, Rust, Svelte, SQLite): ComfyUI renders, Saya Studio organizes. Nothing runs in the cloud.

Packages for **Debian / Ubuntu (.deb)**, **Fedora and openSUSE (.rpm)**, **Arch (.pkg.tar.zst, AUR
`saya-studio-bin`)**, **Nix**, **AppImage**, **Flatpak** and **Snap** are attached to the
[`studio-v1.0.0-beta.1` release](https://github.com/alphaziod/saya-comfy-couple-plus/releases). It needs a running
ComfyUI with the Saya Couple nodes 2.3.0+.

- Install, requirements, your data, known limitations: [studio/README.md](studio/README.md)
- Building packages: [studio/docs/PACKAGING.md](studio/docs/PACKAGING.md)
- For AI agents and contributors: [studio/AGENTS.md](studio/AGENTS.md)

---

## AI-assisted development

**This project was heavily AI-assisted.** AI models helped read and analyse ComfyUI's code, implement, test,
diagnose, audit, package and write the documentation. The AI tools used: **Claude**, **Z Code**, **Codex** and
**Kimi**. What stayed human: the goals, the expected behaviour, the visual evaluation of every test campaign, and
the decisions to keep or reject approaches. Changes were tested and compared with previous states before being
kept, not generated and published as-is.

## Personal project note

I'm not a professional developer. This started because ComfyUI didn't do exactly what I wanted. This repository
is my setup: nodes and an app I actually use, tools most people will never need, and the scripts that tested
them. Install everything and ignore what you don't need, or take one piece. Issues and PRs are welcome; it is
maintained when I have time.

## Licenses

Main license **GPL-3.0-or-later** (`LICENSE`). Some components keep their own license (AGPL-3.0 parts from
ComfyUI-ppm, third-party patches, dependencies you install yourself; RES4LYF restricts commercial services):
see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and the table in
[docs/SAYA_COUPLE.md](docs/SAYA_COUPLE.md#reuse-contributions-and-licenses).
