"""Read whitelisted literal data through AST. Never import or run the supplied script."""
import ast
import json
import resource
import sys

resource.setrlimit(resource.RLIMIT_CPU, (3, 3))
resource.setrlimit(resource.RLIMIT_AS, (512 * 1024 * 1024, 512 * 1024 * 1024))
source = sys.stdin.buffer.read(16 * 1024 * 1024 + 1)
if len(source) > 16 * 1024 * 1024:
    raise ValueError("Script supérieur à 16 Mio")
tree = ast.parse(source)
if sum(1 for _ in ast.walk(tree)) > 250000:
    raise ValueError("AST trop volumineux")
allowed = {"POLE_PERSO", "POLE_STAGES", "POLE_INTIMES"}
data, warnings = {}, []
for node in tree.body:
    if not isinstance(node, ast.Assign):
        continue
    names = [n.id for n in node.targets if isinstance(n, ast.Name) and n.id in allowed]
    for name in names:
        try:
            value = ast.literal_eval(node.value)
            if not isinstance(value, dict):
                raise ValueError("table attendue")
            data[name] = value
        except (ValueError, TypeError, RecursionError, MemoryError):
            warnings.append(name + " : expression dynamique ignorée")
warnings.append("Lecture des constantes seulement : fonctions, imports, substitutions et alias calculés non exécutés.")
result = json.dumps({"tables": data, "warnings": warnings}, ensure_ascii=False)
if len(result) > 16 * 1024 * 1024:
    raise ValueError("Données extraites trop volumineuses")
print(result)
