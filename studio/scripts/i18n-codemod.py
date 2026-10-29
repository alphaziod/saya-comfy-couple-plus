#!/usr/bin/env python3
"""i18n codemod (Saya Studio, 2026-10-26): wraps the visible strings of the Svelte components
in {$t("…")} / attr={$t("…")} and produces locales/fr.json (identity) + locales/en.json
(key=key skeleton, translated by wave 1). Natural keys = the original French strings.
Careful: only pure text nodes (>Texte<) and literal placeholder/title/aria-label
attributes are touched; anything containing { } < is ignored."""
import json
import re
import glob
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent

def est_chaine(s: str) -> bool:
    s = s.strip()
    return len(s) >= 2 and re.search(r"[A-Za-zÀ-ÿ]", s) and "{" not in s and "}" not in s

def echapper_js(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")

cles: dict[str, str] = {}
modifies = 0
fichiers = sorted(glob.glob(str(RACINE / "src/components/*.svelte"))) + [str(RACINE / "src/App.svelte")]
for fichier in fichiers:
    src = Path(fichier).read_text()
    # Case-insensitive, and closing tags may carry spaces (`</script >`): no block is mistaken for markup.
    morceaux = re.split(r"(<script\b[^>]*>.*?</script\s*>|<style\b[^>]*>.*?</style\s*>)", src, flags=re.S | re.IGNORECASE)
    change = False

    def sub_texte(m):
        texte = m.group(1)
        if not est_chaine(texte):
            return m.group(0)
        cle = texte.strip()
        cles[cle] = cle
        nonlocal_change()
        avant = texte[: len(texte) - len(texte.lstrip())]
        apres = texte[len(texte.rstrip()) :]
        return ">" + avant + "{$t(\"" + echapper_js(cle) + "\")}" + apres + "<"

    def sub_attr(m):
        attr, valeur = m.group(1), m.group(2)
        if not est_chaine(valeur):
            return m.group(0)
        cle = valeur.strip()
        cles[cle] = cle
        nonlocal_change()
        return attr + '={$t("' + echapper_js(cle) + '")}'

    _etat = {"change": False}
    def nonlocal_change():
        _etat["change"] = True

    for i, part in enumerate(morceaux):
        if part.startswith("<script") or part.startswith("<style"):
            continue
        part = re.sub(r">([^<>{}]+)<", sub_texte, part)
        part = re.sub(r"\b(placeholder|title|aria-label)=\"([^\"]+)\"", sub_attr, part)
        morceaux[i] = part
    if _etat["change"]:
        nouveau = "".join(morceaux)
        if "lib/i18n" not in nouveau:
            nouveau = re.sub(
                r'(<script lang="ts">\n)',
                r"\1  import { t } from '../lib/i18n';\n",
                nouveau,
                count=1,
            )
        Path(fichier).write_text(nouveau)
        modifies += 1

locales = RACINE / "src/locales"
locales.mkdir(exist_ok=True)
(locales / "fr.json").write_text(json.dumps(cles, ensure_ascii=False, indent=1, sort_keys=True), encoding="utf-8")
(locales / "en.json").write_text(json.dumps(cles, ensure_ascii=False, indent=1, sort_keys=True), encoding="utf-8")
print(f"composants modifiés : {modifies} | clés : {len(cles)}")
