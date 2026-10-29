"""Saya Studio (desktop app) installer: picks the package for this Linux distribution from the latest
`studio-v*` GitHub release, checks it against the release's SHA256SUMS, shows the exact command and
installs it only after the user says yes. Standard library only."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.request

REPO = os.environ.get("SAYA_REPO", "alphaziod/saya-studio-x-couple")
TAG_PREFIX = "studio-v"
FORMATS = ("deb", "fedora", "opensuse", "arch", "appimage", "nix")


def os_release(path: str = "/etc/os-release") -> dict[str, str]:
    data: dict[str, str] = {}
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                if "=" in line:
                    key, value = line.rstrip("\n").split("=", 1)
                    data[key] = value.strip().strip('"')
    except OSError:
        pass
    return data


def detect_format(release: dict[str, str]) -> str:
    """Package format for this distribution: ID first, then ID_LIKE; AppImage when unknown."""
    ids = [release.get("ID", "")] + release.get("ID_LIKE", "").split()
    for name in ids:
        if name == "nixos":
            return "nix"
        if name in ("arch", "endeavouros", "manjaro", "cachyos", "garuda"):
            return "arch"
        if name in ("fedora", "rhel", "centos", "rocky", "almalinux", "nobara"):
            return "fedora"
        if name.startswith("opensuse") or name in ("suse", "sles"):
            return "opensuse"
        if name in ("debian", "ubuntu", "linuxmint", "pop", "elementary", "zorin"):
            return "deb"
    return "appimage"


def asset_matches(name: str, fmt: str) -> bool:
    return {
        "deb": name.endswith("_amd64.deb"),
        "fedora": name.endswith(".fedora.x86_64.rpm"),
        "opensuse": name.endswith(".opensuse.x86_64.rpm"),
        "arch": name.endswith(".pkg.tar.zst"),
        "appimage": name.endswith(".AppImage"),
    }.get(fmt, False)


def install_command(fmt: str, path: str) -> list[str]:
    """The exact command run for a downloaded package (system packages go through sudo)."""
    return {
        "deb": ["sudo", "apt", "install", "-y", path],
        "fedora": ["sudo", "dnf", "install", "-y", path],
        "opensuse": ["sudo", "zypper", "--non-interactive", "install", "--allow-unsigned-rpm", path],
        "arch": ["sudo", "pacman", "-U", "--noconfirm", path],
    }[fmt]


def _get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "saya-installer", "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def latest_release(releases: list[dict] | None = None) -> dict:
    """Newest published (not draft) Saya Studio release; pre-releases count, it is a beta."""
    if releases is None:
        releases = json.loads(_get(f"https://api.github.com/repos/{REPO}/releases?per_page=30"))
    for rel in releases:
        if not rel.get("draft") and rel.get("tag_name", "").startswith(TAG_PREFIX):
            return rel
    raise RuntimeError(f"no published Saya Studio release ({TAG_PREFIX}*) on github.com/{REPO}")


def expected_sha256(sums_text: str, name: str) -> str | None:
    for line in sums_text.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1].lstrip("*") == name:
            return parts[0].lower()
    return None


def install(say, ask, assume: bool | None, dry_run: bool, fmt: str | None = None) -> int:
    if not sys.platform.startswith("linux"):
        say("Saya Studio is a Linux app for now: nothing to install on this system.")
        return 1
    fmt = fmt or detect_format(os_release())
    say("=" * 64 + "\nSaya Studio (desktop app)\n" + "=" * 64)
    say(f"package format : {fmt}")
    if fmt == "nix":
        cmd = f"nix profile install github:{REPO}?dir=studio"
        say(f"On NixOS / Nix, install it with:\n  {cmd}\n(or try it first: nix run github:{REPO}?dir=studio)")
        return 0
    try:
        rel = latest_release()
    except Exception as e:  # network or GitHub API error: say it, change nothing
        say(f"Could not read the releases of github.com/{REPO}: {e}")
        return 1
    assets = rel.get("assets", [])
    pkg = next((a for a in assets if asset_matches(a["name"], fmt)), None)
    sums = next((a for a in assets if a["name"] == "SHA256SUMS"), None)
    if pkg is None:
        say(f"Release {rel['tag_name']} has no {fmt} package. Packages: {', '.join(a['name'] for a in assets)}")
        return 1
    say(f"release        : {rel['tag_name']}{' (pre-release)' if rel.get('prerelease') else ''}")
    say(f"package        : {pkg['name']} ({pkg.get('size', 0) // (1024 * 1024)} MiB)")
    if fmt == "appimage":
        target = os.path.expanduser("~/.local/bin/saya-studio.AppImage")
        say(f"will be saved  : {target} (no root rights needed)")
    else:
        say(f"will run       : {' '.join(install_command(fmt, '<downloaded package>'))}")
    if dry_run:
        say("\n--dry-run: nothing downloaded, nothing installed.")
        return 0
    if not ask("Download and install Saya Studio now?", True, assume):
        say("Nothing installed.")
        return 0
    tmp = tempfile.mkdtemp(prefix="saya-studio-")
    try:
        data = _get(pkg["browser_download_url"])
        if sums is not None:
            want = expected_sha256(_get(sums["browser_download_url"]).decode(), pkg["name"])
            got = hashlib.sha256(data).hexdigest()
            if want is None or want != got:
                say(f"SHA256 mismatch for {pkg['name']} (expected {want}, got {got}): nothing installed.")
                return 1
            say("SHA256         : OK")
        else:
            say("SHA256         : the release has no SHA256SUMS, file not verified")
        # The name comes from the GitHub API: keep a plain file name inside the temporary folder.
        name = os.path.basename(pkg["name"])
        if not name or name != pkg["name"] or not asset_matches(name, fmt):
            say(f"Unexpected package name {pkg['name']!r}: nothing installed.")
            return 1
        path = os.path.join(tmp, name)
        with open(path, "wb") as f:
            f.write(data)
        if fmt == "appimage":
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copyfile(path, target)
            os.chmod(target, 0o755)
            say(f"Installed: {target}\nRun it from there, or add ~/.local/bin to your PATH.")
            return 0
        cmd = install_command(fmt, path)
        say("$ " + " ".join(cmd))
        code = subprocess.call(cmd)
        say("Saya Studio installed." if code == 0 else f"The package manager stopped with code {code}.")
        return code
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
