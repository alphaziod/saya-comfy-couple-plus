#!/usr/bin/env bash
# Pacman package built from the CURRENT FILES of the repo (uncommitted changes and new
# files included), without committing anything. Same PKGBUILD as build-arch.sh; only the source changes:
# tracked + untracked non-ignored files (git ls-files -co --exclude-standard).
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
stamp="$(date +%Y%m%d%H%M)"
version="0.2.0.dev.wt${stamp}"
mkdir -p "$root/test-results"
stage="$(mktemp -d "$root/test-results/arch-package-worktree-${stamp}-XXXXXX")"
# `git ls-files -c` also lists files DELETED from the worktree (uncommitted
# deletion, e.g. GenerationTree.svelte removed from the frontend): tar would fail. We only
# archive what actually exists on disk.
git ls-files -co --exclude-standard -z \
  | while IFS= read -r -d '' f; do [ -e "$f" ] && printf '%s\0' "$f"; done \
  | tar --null -T - --transform 's,^,saya-studio/,' -cf - \
  | gzip -n > "$stage/saya-studio-$version.tar.gz"
cp packaging/arch/PKGBUILD packaging/arch/saya-studio.desktop packaging/arch/saya-studio.install "$stage/"
sed -i "s/^pkgver=.*/pkgver=$version/" "$stage/PKGBUILD"
cp docs/INSTALLATION-ARCH.md docs/SAUVEGARDES.md "$stage/"
git rev-parse HEAD > "$stage/SOURCE-COMMIT"
printf 'worktree non commité au %s\n' "$stamp" >> "$stage/SOURCE-COMMIT"
export CARGO_TARGET_DIR="$root/target"
export PKGEXT='.pkg.tar.zst'
cd "$stage"
makepkg --geninteg >> PKGBUILD
makepkg --printsrcinfo > .SRCINFO
makepkg --noconfirm
printf '\nPaquet : %s\n' "$(ls "$stage"/*.pkg.tar.zst)"
