"""Fake `solo_cli.py` for action-draft tests (Rust unit test and native smoke): implements
`sandbox create`, `action add` and `action validate` on the engine folder it lives in,
with the real CLI's JSON contract ({"ok": ...}, non-zero exit on refusal). No GPU, no ComfyUI."""
import json
import shutil
import sys
from pathlib import Path

a = [x for x in sys.argv[1:] if x != "--json"]
here = Path(__file__).resolve().parent
bank = here / "data" / "actions"


def out(payload, code=0):
    print(json.dumps(payload, ensure_ascii=False))
    sys.exit(code)


def opt(name):
    return a[a.index(name) + 1]


if a[:2] == ["sandbox", "create"]:
    dest = Path(opt("--dest"))
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(here, dest / "generez", ignore=shutil.ignore_patterns("sortie"))
    out({"ok": True, "root": str(dest)})
if a[:2] == ["action", "add"]:
    family, ident = opt("--famille"), opt("--id")
    for path in bank.glob("*.actions.json"):
        if ident in json.loads(path.read_text())["actions"]:
            out({"ok": False, "code": "id_duplique", "message": "ID déjà présent: " + ident}, 1)
    if "--dry-run" in a:
        out({"ok": True, "dry_run": True, "id": ident})
    path = bank / f"{family}.actions.json"
    data = json.loads(path.read_text())
    data["actions"][ident] = json.loads(Path(opt("--file")).read_text())
    path.write_text(json.dumps(data, ensure_ascii=False))
    order = json.loads((bank / "_ordre.json").read_text()) + [ident]
    (bank / "_ordre.json").write_text(json.dumps(order))
    out({"ok": True, "id": ident, "backup": "_backups/fake-" + ident})
if a[:2] == ["action", "validate"]:
    out({"ok": True, "id": a[2]})
out({"ok": False, "message": "commande inconnue"}, 2)
