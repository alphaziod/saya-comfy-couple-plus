import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from types import ModuleType

PATH = Path(__file__).resolve().parents[1] / "crates/studio-core/src/casting_bridge.py"
spec = importlib.util.spec_from_file_location("bridge", PATH)
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class BridgeTests(unittest.TestCase):
    def test_capabilities_follow_engine_without_executing_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "moteur").mkdir()
            (root / "moteur/registre_ids.py").write_text('raise RuntimeError("do not execute")\nFAMILLES = ("existing", "new_class")\n')
            (root / "moteur/generateur_femme_wiring.py").write_text('_MODES = ("mode_a", "girls", "new_mode")\n')
            self.assertEqual(bridge.capabilities(root), {"families": ["existing", "new_class"], "modes": ["mode_a", "girls", "new_mode"], "subfamilies": {}})

    def test_subfamily_uses_existing_engine_choice_without_modifying_files(self):
        import subprocess
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); (root/"moteur").mkdir()
            source='BUFFS=("cute","ard","new_buff")\ndef buff_espece(classe,seed,compteurs):\n    if classe not in ("bny","new_family"):\n        return None\n    return "cute"\n'
            path=root/"moteur/classes_oc.py";path.write_text(source)
            (root/"test_solo.py").write_text('from classes_oc import buff_espece\nimport json,sys\nprint(json.dumps([buff_espece("bny",42,{}),buff_espece("new_family",42,{}),buff_espece("hum",42,{}),sys.argv[1:]]))')
            self.assertEqual(bridge.subfamilies(root),{"bny":["cute","ard","new_buff"],"new_family":["cute","ard","new_buff"]})
            command=["python3","-B",str(PATH),str(root),"--subfamily","bny","ard","--casting","4"]
            result=subprocess.run(command,capture_output=True,text=True,check=True)
            self.assertEqual(json.loads(result.stdout),["ard","cute",None,["--casting","4"]])
            self.assertEqual(path.read_text(),source)
            command[6]="absent"
            self.assertNotEqual(subprocess.run(command,capture_output=True).returncode,0)

    def test_invalid_declaration_is_not_guessed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bad.py"
            path.write_text('FAMILLES = fetch_remote()')
            with self.assertRaises(ValueError):
                bridge.literal(path, "FAMILLES")

    def test_modes_use_engine_functions_and_preserve_seed(self):
        gender = ModuleType("generateur_femme_wiring")
        comfy = ModuleType("comfy")
        calls = []
        def apply(shots, mode):
            calls.append(mode)
            return shots
        gender.apply_female_mode = apply
        comfy.casting_passe_un = lambda shot: {"p1": shot["p1"]}
        shots = [{"seed": 18446744073709551615, "p1": "exact, (tags:1.2)"}]
        with patch.object(bridge, "capabilities", return_value={"modes": ["new_mode"]}), patch.dict("sys.modules", {"generateur_femme_wiring": gender, "comfy": comfy}), patch.object(bridge.sys, "path", list(bridge.sys.path)):
            result = bridge.prepare(Path("/unused"), "new_mode", shots)
            self.assertEqual(calls, ["new_mode"])
            self.assertEqual(result[0]["casting"]["p1"], shots[0]["p1"])
            self.assertEqual(result[0]["seed"], 18446744073709551615)
            with self.assertRaises(ValueError):
                bridge.prepare(Path("/unused"), "unsupported", shots)


if __name__ == "__main__":
    unittest.main()
