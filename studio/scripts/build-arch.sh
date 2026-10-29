#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
# Package only a recorded source state. Uncommitted application edits are preserved.
commit="$(git rev-parse --verify "${1:-HEAD}^{commit}")"
mkdir -p "$root/test-results"
stage="$(mktemp -d "$root/test-results/arch-package-${commit:0:8}-XXXXXX")"
# Validation builds are versioned from the commit: never mistaken for a release.
version="0.2.0.dev.g${commit:0:8}"
git archive --format=tar --prefix=saya-studio/ "$commit" | gzip -n > "$stage/saya-studio-$version.tar.gz"
cp packaging/arch/PKGBUILD packaging/arch/saya-studio.desktop packaging/arch/saya-studio.install "$stage/"
sed -i "s/^pkgver=.*/pkgver=$version/" "$stage/PKGBUILD"
cp src-tauri/icons/icon.png "$stage/icon.png"
cp docs/INSTALLATION-ARCH.md docs/SAUVEGARDES.md "$stage/"
printf '%s\n' "$commit" > "$stage/SOURCE-COMMIT"
# Shared compilation cache only; artifacts are rebuilt from the selected snapshot.
export CARGO_TARGET_DIR="$root/target"
export PKGEXT='.pkg.tar.zst'
cd "$stage"
makepkg --geninteg >> PKGBUILD
makepkg --printsrcinfo > .SRCINFO
makepkg --noconfirm
printf '\nPackage directory: %s\nSource commit: %s\n' "$stage" "$commit"
