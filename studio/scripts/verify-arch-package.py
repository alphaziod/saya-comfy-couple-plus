#!/usr/bin/env python3
"""Inspect a built saya-studio .pkg.tar.zst without installing it (stdlib only).

Usage: verify-arch-package.py PACKAGE.pkg.tar.zst [EXTRACT_DIR] [--pkgbuild PKGBUILD]

Checks: exact file set (maintained lists below, cross-checked against the literal
destinations in PKGBUILD's package()), ownership and permissions, no leaked
developer paths, no smoke-test instrumentation, desktop file, icon, declared
dependencies versus the binaries' linked libraries. EXTRACT_DIR must not exist;
when omitted a temporary directory is used and removed. Nothing is installed.
"""
import argparse
import hashlib
import io
import json
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
from fnmatch import fnmatchcase
from pathlib import Path

# Exact files: path -> mode.
EXACT = {
    'usr/bin/saya-studio': 0o755,
    'usr/bin/saya-studio-maintenance': 0o755,
    'usr/share/applications/saya-studio.desktop': 0o644,
    'usr/share/icons/hicolor/16x16/apps/saya-studio.png': 0o644,
    'usr/share/icons/hicolor/24x24/apps/saya-studio.png': 0o644,
    'usr/share/icons/hicolor/32x32/apps/saya-studio.png': 0o644,
    'usr/share/icons/hicolor/48x48/apps/saya-studio.png': 0o644,
    'usr/share/icons/hicolor/64x64/apps/saya-studio.png': 0o644,
    'usr/share/icons/hicolor/128x128/apps/saya-studio.png': 0o644,
    'usr/share/icons/hicolor/256x256/apps/saya-studio.png': 0o644,
    'usr/share/icons/hicolor/512x512/apps/saya-studio.png': 0o644,
    'usr/share/doc/saya-studio/INSTALLATION-ARCH.md': 0o644,
    'usr/share/doc/saya-studio/SAUVEGARDES.md': 0o644,
    'usr/share/saya-studio/civitai_bridge.py': 0o755,
    'usr/share/saya-studio/saya_native_host.py': 0o755,
    'usr/share/saya-studio/sidecars/f09-semantic-embed.py': 0o755,
    'usr/share/saya-studio/com.saya.studio.json': 0o644,
    'etc/opt/chrome/native-messaging-hosts/com.saya.studio.json': 0o644,
    'etc/chromium/native-messaging-hosts/com.saya.studio.json': 0o644,
}
# Glob groups (at least one match each): pattern -> mode.
GROUPS = {
    'usr/share/saya-studio/presets/*.json': 0o644,
    'usr/share/saya-studio/presets/passes/*.json': 0o644,
    'usr/share/saya-studio/presets/outils/*.py': 0o755,
    'usr/share/saya-studio/extension/*': 0o644,
}
METADATA = {'.PKGINFO', '.BUILDINFO', '.MTREE', '.INSTALL'}
REQUIRED_DEPENDS = {'python', 'curl', 'xdg-utils', 'xdg-desktop-portal', 'glib2', 'webkit2gtk-4.1'}
FORBIDDEN_STRINGS = [b'integrate-astra', b'/home/']
SMOKE_MARKERS = [b'finish_smoke', b'SAYA_STUDIO_SMOKE', b'smoke-report.json']
failures = []


def check(cond, message):
    if not cond:
        failures.append(message)
    return cond


def pkgbuild_destinations(pkgbuild):
    """Literal (glob-free) $pkgdir destinations that PKGBUILD's package() installs."""
    text = Path(pkgbuild).read_text()
    body = text.split('package()', 1)[1] if 'package()' in text else ''
    found = set()
    for m in re.finditer(r'"\$pkgdir/([^"$*]+)"', body):
        found.add(m.group(1))
    # `install -Dm.. src "$pkgdir/..../"` style directory targets and mkdirs are not files.
    return {p for p in found if not p.endswith('/') and Path(p).suffix}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('package')
    ap.add_argument('output', nargs='?')
    ap.add_argument('--pkgbuild', default=str(Path(__file__).resolve().parent.parent / 'packaging/arch/PKGBUILD'))
    args = ap.parse_args()
    package = Path(args.package).resolve()
    keep = args.output is not None
    output = Path(args.output).resolve() if keep else Path(tempfile.mkdtemp(prefix='saya-verify-'))
    if keep:
        output.mkdir()
    try:
        report = verify(package, output, Path(args.pkgbuild))
    finally:
        if not keep:
            shutil.rmtree(output, ignore_errors=True)
    if keep:
        (output.parent / (output.name + '-report.json')).write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))
    if failures:
        print('\nFAILED:', *failures, sep='\n  - ', file=sys.stderr)
        sys.exit(1)


def verify(package, output, pkgbuild):
    raw = subprocess.check_output(['zstd', '-dc', str(package)])
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:') as archive:
        members = archive.getmembers()
        files = {m.name.removeprefix('./'): m for m in members if not m.isdir()}
        for m in members:
            check(not m.name.startswith('/') and '..' not in Path(m.name).parts, f'unsafe path {m.name}')
            check(m.isfile() or m.isdir(), f'not a regular file/dir (symlink or special): {m.name}')
            check(m.uid == 0 and m.gid == 0, f'not owned by root: {m.name}')
            check(not (m.mode & 0o002), f'world-writable: {m.name}')
            check(not (m.mode & 0o6000), f'setuid/setgid: {m.name}')
            if m.isfile() and not m.name.removeprefix('./').startswith('.'):
                check(m.mode & 0o444 == 0o444, f'not world-readable: {m.name}')
        payload = {n for n in files if n not in METADATA}
        matched = set(EXACT)
        for name, mode in EXACT.items():
            if check(name in files, f'missing {name}'):
                check(files[name].mode == mode, f'{name}: mode {files[name].mode:o}, expected {mode:o}')
        for pattern, mode in GROUPS.items():
            hits = {n for n in payload if fnmatchcase(n, pattern) and '/' not in n[len(pattern.rsplit('/', 1)[0]) + 1:]}
            check(hits, f'no file matches {pattern}')
            matched |= hits
            for n in hits:
                check(files[n].mode == mode, f'{n}: mode {files[n].mode:o}, expected {mode:o}')
        check(not payload - matched, f'unexpected files: {sorted(payload - matched)}')
        for name in ('.PKGINFO', '.BUILDINFO', '.MTREE', '.INSTALL'):
            check(name in files, f'missing {name}')
        if pkgbuild.is_file():
            declared = pkgbuild_destinations(pkgbuild)
            check(declared <= set(files), f'PKGBUILD installs files absent from package: {sorted(declared - set(files))}')
        info = archive.extractfile(files['.PKGINFO']).read().decode()
        check('pkgname = saya-studio\n' in info and 'arch = x86_64\n' in info, 'PKGINFO name/arch')
        check('license = GPL-3.0-or-later\n' in info, 'PKGINFO license')
        if '.INSTALL' in files:
            hook = archive.extractfile(files['.INSTALL']).read().decode()
            check('post_remove' in hook and 'NativeMessagingHosts' in hook, '.INSTALL lacks the post_remove cleanup notice')
        archive.extractall(output, filter='data')

    # Leaked developer paths and smoke instrumentation, in every installed file.
    for name in sorted(payload):
        data = (output / name).read_bytes()
        for needle in FORBIDDEN_STRINGS:
            check(needle not in data, f'{name} contains {needle.decode()!r}')
        if name.startswith('usr/bin/') or name.endswith('.json') or name.endswith('.js') or name.endswith('.py'):
            for marker in SMOKE_MARKERS:
                check(marker not in data, f'{name} contains smoke marker {marker.decode()!r}')
        if name.endswith('.py') and data.startswith(b'#!'):
            check(data.splitlines()[0] in (b'#!/usr/bin/env python3', b'#!/usr/bin/python3'),
                  f'{name}: unexpected shebang {data.splitlines()[0][:60]!r}')

    desktop = output / 'usr/share/applications/saya-studio.desktop'
    subprocess.run(['desktop-file-validate', str(desktop)], check=True)
    launcher = desktop.read_text()
    check('Exec=saya-studio\n' in launcher and 'Icon=saya-studio\n' in launcher, 'desktop Exec/Icon')
    for size in (16, 24, 32, 48, 64, 128, 256, 512):
        icon = (output / f'usr/share/icons/hicolor/{size}x{size}/apps/saya-studio.png').read_bytes()
        check(icon[:8] == b'\x89PNG\r\n\x1a\n' and struct.unpack('!II', icon[16:24]) == (size, size), f'icon {size} is not a {size}x{size} PNG')

    for rel in ('usr/share/saya-studio/com.saya.studio.json', 'etc/opt/chrome/native-messaging-hosts/com.saya.studio.json',
                'etc/chromium/native-messaging-hosts/com.saya.studio.json'):
        manifest = json.loads((output / rel).read_text())
        check(manifest.get('path') == '/usr/share/saya-studio/saya_native_host.py', f'{rel}: wrong host path')
        check(all(o.startswith('chrome-extension://') for o in manifest.get('allowed_origins', [])), f'{rel}: bad origins')

    depends = set(re.findall(r'^depend = (.+)$', info, re.M))
    check(REQUIRED_DEPENDS <= depends, f'missing depends: {sorted(REQUIRED_DEPENDS - depends)}')
    owners = {}
    for executable in ['saya-studio', 'saya-studio-maintenance']:
        path = output / 'usr/bin' / executable
        dynamic = subprocess.check_output(['readelf', '-d', str(path)], text=True)
        linked = subprocess.check_output(['ldd', str(path)], text=True)
        check('not found' not in linked, f'{executable}: unresolved libraries\n{linked}')
        for name in re.findall(r'NEEDED.*\[(.*?)\]', dynamic):
            match = re.search(r'^\s*' + re.escape(name) + r' => (\S+)', linked, re.M)
            library = match.group(1) if match else '/usr/lib/' + name
            result = subprocess.run(['pacman', '-Qoq', library], text=True, capture_output=True)
            owner = result.stdout.strip()
            if check(result.returncode == 0 and owner, f'{executable}: no pacman owner for {library}'):
                check(owner in depends, f'{executable}: {name} owned by {owner}, not in depends')
                owners[name] = owner
    return {
        'ok': not failures, 'package': str(package),
        'sha256': hashlib.sha256(package.read_bytes()).hexdigest(),
        'files': sorted(payload), 'depends': sorted(depends), 'directLibraryOwners': owners,
        'installHook': '.INSTALL' in files, 'installed': False,
    }


if __name__ == '__main__':
    main()
