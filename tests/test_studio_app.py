"""Offline checks of the Saya Studio part of the installer: python3 tests/test_studio_app.py"""
import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "installer"))
from saya_installer import studio_app as s  # noqa: E402

assert s.detect_format({"ID": "linuxmint", "ID_LIKE": "ubuntu debian"}) == "deb"
assert s.detect_format({"ID": "fedora"}) == "fedora"
assert s.detect_format({"ID": "opensuse-tumbleweed", "ID_LIKE": "opensuse suse"}) == "opensuse"
assert s.detect_format({"ID": "endeavouros", "ID_LIKE": "arch"}) == "arch"
assert s.detect_format({"ID": "nixos"}) == "nix"
assert s.detect_format({"ID": "void"}) == "appimage"
assert s.asset_matches("saya-studio-1.0.0-beta.1.fedora.x86_64.rpm", "fedora")
assert not s.asset_matches("saya-studio-1.0.0-beta.1.opensuse.x86_64.rpm", "fedora")
assert s.expected_sha256("ab12  a.deb\ncd34 *b.rpm\n", "b.rpm") == "cd34"
assert s.expected_sha256("ab12  a.deb\n", "missing.deb") is None
releases = [{"draft": True, "tag_name": "studio-v9"}, {"draft": False, "tag_name": "v2.3.0"},
            {"draft": False, "prerelease": True, "tag_name": "studio-v1.0.0-beta.1"}]
assert s.latest_release(releases)["tag_name"] == "studio-v1.0.0-beta.1", "drafts and node releases skipped"
assert s.install_command("arch", "/tmp/x.pkg.tar.zst")[:3] == ["sudo", "pacman", "-U"]
print("studio_app: OK")
