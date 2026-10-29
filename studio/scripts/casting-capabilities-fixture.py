"""Capabilities for isolated native smoke engines, never the live engine."""
import sys
from pathlib import Path
root = Path(sys.argv[1]).resolve()
assert root.name == "engine-stub" and "test-results" in root.parts, root
modules = root / "moteur"
modules.mkdir(exist_ok=True)
(modules / "registre_ids.py").write_text('FAMILLES = ("hum", "bny", "kmn", "slm", "lcs", "cat", "dem", "elf", "cute", "edgy", "hor", "ard", "div", "sop", "lra")\n')
(modules / "generateur_femme_wiring.py").write_text('_MODES = ("futa", "girls", "both")\n')
