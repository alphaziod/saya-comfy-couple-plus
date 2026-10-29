"""App adapter: read engine capabilities and call its existing gender conversion.
No GPU submission, pool promotion, prompt reconstruction, or engine writes.
"""
import ast
import json
import sys
from pathlib import Path


def literal(path, key):
    for node in ast.parse(path.read_text()).body:
        if isinstance(node, ast.Assign) and any(isinstance(t, ast.Name) and t.id == key for t in node.targets):
            value = ast.literal_eval(node.value)
            if isinstance(value, (list, tuple)) and value and all(isinstance(v, str) and v for v in value):
                return list(value)
    raise ValueError(f"Capacité moteur absente : {path.name}:{key}")


def subfamilies(root):
    path = root / "moteur/classes_oc.py"
    if not path.is_file():
        return {}
    buffs = literal(path, "BUFFS")
    for node in ast.parse(path.read_text()).body:
        if isinstance(node, ast.FunctionDef) and node.name == "buff_espece":
            for condition in node.body:
                test = getattr(condition, "test", None)
                if (isinstance(test, ast.Compare) and isinstance(test.left, ast.Name)
                        and test.left.id == "classe" and len(test.ops) == 1
                        and isinstance(test.ops[0], ast.NotIn)):
                    families = ast.literal_eval(test.comparators[0])
                    if isinstance(families, (tuple, list)) and all(isinstance(f, str) for f in families):
                        return {family: buffs for family in families}
    raise ValueError("Déclaration des sous-familles moteur non reconnue")


def run_subfamily(root, family, buff, args):
    if buff not in subfamilies(root).get(family, []):
        raise ValueError("Sous-famille non fournie par le moteur")
    import runpy
    sys.path[:0] = [str(root), str(root / "moteur")]
    import classes_oc
    original = classes_oc.buff_espece
    def selected(classe, seed, compteurs):
        if classe != family:
            return original(classe, seed, compteurs)
        usage = compteurs.setdefault("buff", {name: 0 for name in classes_oc.BUFFS})
        usage[buff] += 1
        return buff
    classes_oc.buff_espece = selected
    sys.argv = [str(root / "test_solo.py"), *args]
    try:
        runpy.run_path(str(root / "test_solo.py"), run_name="__main__")
    finally:
        classes_oc.buff_espece = original


def run_poses(root, perso, graine, actions):
    """One character on every validated solo action (the engine's own `draw_poses`, the
    `--poses <ID>` mode), as a pass-1 casting catalogue in sortie/casting/. `actions` keeps
    only those action names (all of them when empty). Nothing is submitted to ComfyUI."""
    sys.path[:0] = [str(root), str(root / "moteur")]
    import noyau
    import tirage
    from comfy import casting_passe_un
    noyau.graine_tirage(int(graine))
    shots = tirage.draw_poses(perso)
    if actions:
        drawn = {shot.get("action_name") for shot in shots}
        missing = [name for name in actions if name not in drawn]
        if missing:
            raise ValueError("Actions absentes du tirage de ce perso : " + ", ".join(missing))
        shots = [shot for shot in shots if shot.get("action_name") in set(actions)]
    if not shots:
        raise ValueError(f"Aucune action jouable pour {perso}")
    for shot in shots:
        shot["lazy"] = True
        shot["casting"] = casting_passe_un(shot)
    folder = root / "sortie" / "casting"
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "catalogue.json").write_text(json.dumps(shots, ensure_ascii=False, indent=2) + "\n")
    print(f"CASTING POSES {perso} — {len(shots)} actions, rien envoyé", flush=True)


def capabilities(root):
    return {"families": literal(root / "moteur/registre_ids.py", "FAMILLES"),
            "modes": literal(root / "moteur/generateur_femme_wiring.py", "_MODES"),
            "subfamilies": subfamilies(root)}


def prepare(root, mode, shots):
    if mode not in capabilities(root)["modes"]:
        raise ValueError(f"Mode non fourni par le moteur : {mode}")
    sys.path.insert(0, str(root / "moteur"))
    from generateur_femme_wiring import apply_female_mode
    from comfy import casting_passe_un
    shots = apply_female_mode(shots, mode)
    for shot in shots:
        shot["casting"] = casting_passe_un(shot)
    return shots


if __name__ == "__main__":
    root = Path(sys.argv[1])
    if len(sys.argv) > 2 and sys.argv[2] == "--subfamily":
        run_subfamily(root, sys.argv[3], sys.argv[4], sys.argv[5:])
        sys.exit(0)
    if len(sys.argv) > 2 and sys.argv[2] == "--poses-all":
        run_poses(root, sys.argv[3], sys.argv[4], [a for a in sys.argv[5].split(",") if a] if len(sys.argv) > 5 else [])
        sys.exit(0)
    result = capabilities(root) if len(sys.argv) == 2 else prepare(root, sys.argv[2], json.load(sys.stdin))
    print(json.dumps(result, ensure_ascii=False))
