"""CPU-only AMD patch checks: python tests/amd_desktop_reserve.py --comfyui CLEAN_ROOT."""
import argparse
import ast
import subprocess
import sys
import tempfile
from pathlib import Path
from unittest.mock import patch as mock_patch

PKG = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PKG / 'installer'))
from saya_installer import cli, compat, patchlib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--comfyui', required=True)
    args = parser.parse_args()
    source = (Path(args.comfyui) / 'comfy/model_management.py').read_text()
    full = compat.load_patch(str(PKG / cli.AMD_PATCH))
    delta = compat.load_patch(str(PKG / 'patches/amd_desktop_reserve.patch'))
    fixed = patchlib.apply_to_text(source, full)
    assert fixed is not None, 'expected a clean compatible ComfyUI source'
    old = patchlib.apply_to_text(fixed, delta, reverse=True)
    assert old is not None and old != fixed
    assert patchlib.apply_to_text(fixed, full, reverse=True) == source
    assert (PKG / cli.AMD_PATCH).read_bytes() == (PKG / 'files/custom_nodes/Saya_Couple/tools/core_patches/amd_vram_safety.patch').read_bytes()

    with tempfile.TemporaryDirectory() as temp:
        target = Path(temp) / full.path
        target.parent.mkdir(parents=True)
        for label, initial in [('fresh', source), ('upgrade', old)]:
            target.write_text(initial)
            assert cli.patch_state(temp, cli.AMD_PATCH) == 'clean', label
            assert cli.apply_patch(temp, cli.AMD_PATCH) == 'applied', label
            assert target.read_text() == fixed, label
            assert cli.apply_patch(temp, cli.AMD_PATCH) == 'applied', 'idempotence'
        target.write_text(old.replace('AMD_DESKTOP_VRAM_HEADROOM = 1024 * 1024 * 1024', 'AMD_DESKTOP_VRAM_HEADROOM = 2 * 1024 * 1024 * 1024'))
        before = target.read_bytes()
        assert cli.patch_state(temp, cli.AMD_PATCH) == 'conflict'
        try:
            cli.apply_patch(temp, cli.AMD_PATCH)
        except RuntimeError:
            pass
        else:
            raise AssertionError('modified safety patch must be refused')
        assert target.read_bytes() == before
        for patch, initial, expected in [(full, source, fixed), (delta, old, fixed)]:
            target.write_text(initial)
            path = PKG / ('patches/amd_vram_safety.patch' if patch is full else 'patches/amd_desktop_reserve.patch')
            subprocess.run(['git', 'apply', '--check', str(path)], cwd=temp, check=True)
            subprocess.run(['git', 'apply', str(path)], cwd=temp, check=True)
            assert target.read_text() == expected
            subprocess.run(['git', 'apply', '--reverse', str(path)], cwd=temp, check=True)
            assert target.read_text() == initial

        # An install record from the old patch must retain its original clean backup.
        target.write_text(source)
        (Path(temp) / 'main.py').write_text('')
        attention = Path(temp) / 'comfy/ldm/modules/attention.py'
        attention.parent.mkdir(parents=True)
        attention.write_text('')
        (Path(temp) / 'custom_nodes').mkdir()
        _, state = cli.new_backup(temp, {})
        cli.backup_file(temp, state, full.path, 'amd_optional')
        target.write_text(old)
        state['core_files'][full.path]['sha256_after'] = cli.sha256_file(target)
        cli.save_state(temp, state)
        backup = Path(temp) / state['backup'] / 'files' / full.path
        with mock_patch.object(cli.gpu, 'detect', return_value={'vendor': 'amd', 'backend': 'rocm', 'vram_gib': 16}):
            cli.run_amd(temp, state, sys.executable, 'yes', True)
        assert target.read_text() == fixed and backup.read_text() == source
        assert cli.load_state(temp)['core_files'][full.path]['sha256_after'] == cli.sha256_file(target)
        args = argparse.Namespace(comfyui=temp, python=sys.executable, revert=True, force=False)
        assert cli.cmd_amd(args) == 0
        assert target.read_text() == source

    functions = [n for n in ast.parse(fixed).body if isinstance(n, ast.FunctionDef)
                 and n.name in {'extra_reserved_memory', 'minimum_inference_memory'}]
    gib = 1024 ** 3
    for capped, reserve, expected in [(False, .75, .75), (True, 0, 0), (True, .75, 0), (True, 1, 0), (True, 2, 1)]:
        namespace = {'AMD_VRAM_CAP': 15 * gib if capped else None,
                     'AMD_DESKTOP_VRAM_HEADROOM': gib, 'EXTRA_RESERVED_VRAM': reserve * gib}
        exec(compile(ast.Module(body=functions, type_ignores=[]), '<patched core>', 'exec'), namespace)
        assert namespace['extra_reserved_memory']() == expected * gib
        assert namespace['minimum_inference_memory']() == (0.8 + expected) * gib
    print('PASS: fresh install, upgrade, idempotence, conflict refusal, git apply/revert, five reserve cases')


if __name__ == '__main__':
    main()
