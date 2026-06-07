# ComfyCouple IPAdapter AutoMask Patch

A patched version of the ComfyUI Comfy Couple node for dual-character and dual-persona workflows.

This patch improves Comfy Couple so it can work more cleanly with IPAdapter, expose automatic region masks, and handle regional prompts with different encoded token lengths without crashing.

## What it does

This patch adds:

- IPAdapter compatibility for Comfy Couple attention patches
- automatic `mask_positive_1` and `mask_positive_2` outputs
- safer conditioning padding when regional prompts have different encoded lengths
- a practical dual-persona workflow structure using Comfy Couple regions plus IPAdapter references

## Token length note

This patch does **not** remove the CLIP token limit.

Instead, it pads shorter conditioning tensors so that `positive_1`, `positive_2`, and negative conditioning can be combined safely when their encoded token lengths are different.

This helps prevent tensor size mismatch crashes when one regional prompt is longer than the other.

## Main use case

This is useful for workflows like:

```text
Main prompt = scene, framing, mood, lighting
Positive 1 = left character identity
Positive 2 = right character identity
Comfy Couple = spatial separation
IPAdapter 1 = visual reference for character 1
IPAdapter 2 = visual reference for character 2
Auto masks = region control for each IPAdapter
```

This behaves like a lightweight regional prompting system, but using Comfy Couple regions and IPAdapter references together.

## Example workflow logic

```text
Checkpoint model
-> Comfy Couple
-> IPAdapter for persona 1 using mask_positive_1
-> IPAdapter for persona 2 using mask_positive_2
-> Sampler
```

## Modified files

```text
attention_couple.py
comfy_couple.py
```

## Installation

### 1. Install Git

#### Windows

Install Git from:

```text
https://git-scm.com/download/win
```

#### macOS

Install Git from:

```text
https://git-scm.com/download/mac
```

Or install it with Homebrew:

```bash
brew install git
```

#### Arch Linux

```bash
sudo pacman -S git
```

#### Fedora

```bash
sudo dnf install git
```

#### Debian / Ubuntu

```bash
sudo apt install git
```

#### Nix / NixOS

Temporary shell:

```bash
nix shell nixpkgs#git
```

Or add Git to your NixOS configuration.

### 2. Go to your ComfyUI custom nodes folder

#### Windows PowerShell

```powershell
cd C:\ComfyUI\custom_nodes
```

#### macOS / Linux

```bash
cd ~/ComfyUI/custom_nodes
```

If your ComfyUI folder is somewhere else, use your own path.

### 3. Backup your existing Comfy Couple folder

#### Windows PowerShell

```powershell
Rename-Item comfyui-comfycouple comfyui-comfycouple-backup
```

#### macOS / Linux

```bash
mv comfyui-comfycouple comfyui-comfycouple-backup
```

### 4. Clone this patched version

```bash
git clone https://github.com/YOUR_USERNAME/comfycouple-ipadapter-automask.git comfyui-comfycouple
```

Replace `YOUR_USERNAME` with the GitHub username that owns this repository.

### 5. Restart ComfyUI

Fully stop ComfyUI, then start it again.

The Comfy Couple node should now expose:

```text
model
positive
negative
mask_positive_1
mask_positive_2
```

## Requirements

This repository is not a full ComfyUI workflow.

It only provides a patched version of the Comfy Couple custom node files.

You still need:

- ComfyUI
- Comfy Couple dependencies
- IPAdapter or IPAdapter Plus nodes
- your own checkpoint
- your own CLIP Vision model
- your own IPAdapter model

## Prompt structure recommendation

For best results, keep prompt roles separated:

```text
Main prompt = scene, framing, action, mood, lighting
Positive 1 = character 1 identity
Positive 2 = character 2 identity
Negative = light cleanup only
```

Avoid putting full character identities inside the main prompt when using dual IPAdapters, because each region may try to recreate the full scene.

A clean setup looks like this:

```text
Main prompt:
romantic couple scene, close framing, medieval street, sunset lighting

Positive 1:
left character identity, outfit, expression, body type, hair, eyes

Positive 2:
right character identity, outfit, expression, body type, hair, eyes

Negative:
duplicate, clone, extra characters, merged bodies, bad hands, bad anatomy
```

## Notes

This patch is mainly intended for:

- dual-character generation
- couple scenes
- persona references
- masked IPAdapter control
- Comfy Couple based regional prompting

It is experimental and mainly focused on practical ComfyUI workflows.

## Development note

This patch was developed through hands-on ComfyUI testing and debugging, with AI assistance used to help analyze errors, write code changes, and document the workflow.

## Credits

Original Comfy Couple node by its original author.

This repository contains a modified version focused on IPAdapter compatibility, automatic mask outputs, and safer dual-prompt handling.
