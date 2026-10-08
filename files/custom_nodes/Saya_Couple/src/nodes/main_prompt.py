"""Saya Main Prompt: ONE node for the whole MAIN (background) prompt.

MAIN = [prefix] + [background] + [tags chosen in the drop-down menus] + [extra], joined with ",\\n".
* ``prefix`` and ``extra`` are free texts copied VERBATIM (quality tags, LoRA triggers, anything hand-written: the
  node never rewrites, reorders or strips their content, only the joining comma/newline is handled);
* the background comes from the categorized stock exactly like ``SayaBackgroundPicker`` (same functions, same
  history file): category ``all`` / a category + seed (fixed = locked, randomize = a new one each run),
  ``free`` = ``custom_background`` as written, ``history`` = back / forward / stay through the last 5 generations,
  ``ultra_detailed`` = the hand-written ultra version of the background;
* each drop-down group (data/main_prompt_tags.json) adds one tag, ``none`` adds nothing.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from . import background_picker as picker

TAGS_PATH = Path(__file__).resolve().parents[2] / "data" / "main_prompt_tags.json"
NONE = "none"
DEFAULT_PREFIX = "best quality, very aesthetic, absurdres, highres,"


def tag_groups() -> list[dict[str, Any]]:
    """[{name, label, options}] from the JSON; a bad file is an error, not a silent empty menu."""
    groups = json.loads(TAGS_PATH.read_text(encoding="utf-8"))["groups"]
    for group in groups:
        if not group.get("name") or not isinstance(group.get("options"), list) or not group["options"]:
            raise picker.SayaBackgroundError(f"{TAGS_PATH.name}: group {group!r} needs a name and options")
        if NONE in group["options"]:
            raise picker.SayaBackgroundError(f"{TAGS_PATH.name}: {group['name']}: {NONE!r} is reserved")
    return groups


def join_prompt(parts: list[str]) -> str:
    """Join non-empty parts with ",\\n"; a part that already ends with a comma gets only the newline. The parts
    themselves are untouched (only trailing whitespace is dropped for the join)."""
    text = ""
    for part in parts:
        part = (part or "").rstrip()
        if not part:
            continue
        if text:
            text += "\n" if text.endswith(",") else ",\n"
        text += part
    return text


class SayaMainPrompt:
    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        required: dict[str, Any] = {
            "prefix": ("STRING", {"default": DEFAULT_PREFIX, "multiline": True,
                                  "tooltip": "Copied as written (quality tags, LoRA triggers...). The node never touches it."}),
            "background": ([picker.ALL, picker.FREE, *picker.CATEGORIES], {"default": picker.ALL,
                           "tooltip": "all = whole stock, a category, or free = custom_background as written."}),
            "seed": ("INT", {"default": 0, "min": 0, "max": 0xffffffffffffffff, "control_after_generate": True,
                             "tooltip": "fixed = locked background, randomize = a new one each run."}),
            "history": ([picker.NEW, picker.BACK, picker.FORWARD, picker.STAY], {"default": picker.NEW,
                        "tooltip": "new = draw from category + seed. back / forward = one step older / newer in the last 5 "
                                   "generations (shared with Saya Background Picker). stay = keep the one shown."}),
            "ultra_detailed": ("BOOLEAN", {"default": False, "tooltip": "The hand-written ultra version of the background."}),
        }
        for group in tag_groups():
            required[group["name"]] = ([NONE, *group["options"]], {"default": NONE, "tooltip": f"{group['label']}: one tag added to MAIN, or none."})
        return {
            "required": required,
            "optional": {
                "custom_background": ("STRING", {"default": "", "multiline": True, "tooltip": "background = free: this text is the background, as written."}),
                "extra": ("STRING", {"default": "", "multiline": True, "tooltip": "Appended as written after the tags (never touched)."}),
                "unsafe_backgrounds": ("BOOLEAN", {"default": False, "tooltip": "OFF = unsafe backgrounds (fairground rides, figures, captivity, gore) are never drawn. ON = they can be drawn too."}),
            },
        }

    # prefix / scene appended last (saved workflows resolve outputs by slot index): encode them apart and wire
    # prefix to SayaMultiCouple 'quality', scene to 'main', so quality is read first and the scenery last.
    RETURN_TYPES = ("STRING", "STRING", "STRING", "STRING", "STRING", "STRING")
    RETURN_NAMES = ("main_prompt", "background", "short_name", "info", "prefix", "scene")
    FUNCTION = "compose"
    CATEGORY = "saya/couple"
    DESCRIPTION = ("MAIN prompt in one node: prefix (verbatim) + background from the stock (locked / random / free, "
                   "history, ultra) + tags from the menus + extra (verbatim).")

    @classmethod
    def IS_CHANGED(cls, prefix, background, seed, history=picker.NEW, ultra_detailed=False, custom_background="", extra="", unsafe_backgrounds=False, **tags):
        if history in (picker.BACK, picker.FORWARD):
            return float("nan")  # moves the history cursor: must run every time
        state = json.dumps(picker.read_state()) if history == picker.STAY else ""
        return f"{prefix}|{background}|{seed}|{history}|{ultra_detailed}|{custom_background}|{extra}|{unsafe_backgrounds}|{sorted(tags.items())}|{state}"

    def compose(self, prefix: str, background: str, seed: int, history: str = picker.NEW, ultra_detailed: bool = False,
                custom_background: str = "", extra: str = "", unsafe_backgrounds: bool = False, **tags: str) -> tuple[str, str, str, str, str, str]:
        bg_text, short, info = picker.SayaBackgroundPicker().choose(background, seed, history, ultra_detailed, custom_background, unsafe_backgrounds)
        chosen = [tags[group["name"]] for group in tag_groups() if tags.get(group["name"], NONE) != NONE]
        main = join_prompt([prefix, bg_text, ", ".join(chosen), extra])
        scene = join_prompt([bg_text, ", ".join(chosen), extra])
        return main, bg_text, short, f"{info} | tags: {', '.join(chosen) or NONE}", join_prompt([prefix]), scene


# ---------------------------------------------------------------------------
# Version française : mêmes réglages, mêmes sorties ; seuls les libellés de l'interface sont en français
# (noms d'entrées, catégories de décor, historique, tags des menus). Le texte envoyé au modèle reste l'anglais.
# ---------------------------------------------------------------------------

CATEGORIES_FR = {
    "all": "tout", "free": "libre",
    "horror": "horreur", "cozy": "douillet", "dream": "rêve", "feerique": "féerique", "strange": "étrange", "nature": "nature",
    "house": "maison", "city": "ville", "night": "nuit", "water": "eau", "beach": "plage", "winter": "hiver", "desert": "désert",
    "ruins": "ruines", "temple": "temple", "castle": "château", "space": "espace", "tech": "technologie", "industrial": "industriel",
    "luxury": "luxe", "forest": "forêt", "mountain": "montagne", "cave": "grotte", "garden": "jardin", "sky": "ciel", "rain": "pluie",
    "autumn": "automne", "tropical": "tropical", "underground": "souterrain", "gothic": "gothique", "cyberpunk": "cyberpunk",
    "steampunk": "steampunk", "apocalypse": "apocalypse", "festival": "festival", "market": "marché", "cafe": "café",
    "library": "bibliothèque", "bedroom": "chambre", "bathroom": "salle de bain", "train": "train", "horror++": "horreur++",
}
HISTORY_FR = {picker.NEW: "nouveau", picker.BACK: "retour", picker.FORWARD: "avant", picker.STAY: "garder"}
NONE_FR = "aucun"


def _fr_choices(mapping: dict[str, str], keys: list[str]) -> list[str]:
    """French labels in the given order; a key without a label (a category added to the stock later) is shown as is."""
    return [mapping.get(key, key) for key in keys]


def _from_fr(mapping: dict[str, str], label: str) -> str:
    """French label -> internal value (a value shown as is maps to itself)."""
    for key, value in mapping.items():
        if value == label:
            return key
    return label


class SayaMainPromptFR:
    @classmethod
    def INPUT_TYPES(cls) -> dict[str, dict[str, Any]]:
        required: dict[str, Any] = {
            "prefixe": ("STRING", {"default": DEFAULT_PREFIX, "multiline": True,
                                   "tooltip": "Copié tel quel (tags de qualité, triggers LoRA...). Le nœud n'y touche jamais."}),
            "decor": (_fr_choices(CATEGORIES_FR, [picker.ALL, picker.FREE, *picker.CATEGORIES]), {"default": CATEGORIES_FR["all"],
                      "tooltip": "tout = tout le stock, une catégorie, ou libre = decor_libre tel quel."}),
            "seed": ("INT", {"default": 0, "min": 0, "max": 0xffffffffffffffff, "control_after_generate": True,
                             "tooltip": "fixed = décor verrouillé, randomize = un nouveau décor à chaque run."}),
            "historique": (_fr_choices(HISTORY_FR, [picker.NEW, picker.BACK, picker.FORWARD, picker.STAY]), {"default": HISTORY_FR[picker.NEW],
                           "tooltip": "nouveau = tirage catégorie + seed. retour / avant = un cran plus ancien / plus récent dans les 5 "
                                      "dernières générations (historique partagé). garder = conserver le décor affiché."}),
            "ultra_detaille": ("BOOLEAN", {"default": False, "tooltip": "Version poussée à fond du décor, écrite à la main."}),
        }
        for group in tag_groups():
            required[group["name"] + "_fr"] = ([NONE_FR, *_fr_choices(group.get("fr", {}), group["options"])],
                                               {"default": NONE_FR, "tooltip": f"{group.get('label_fr', group['label'])} : un tag ajouté à MAIN, ou aucun."})
        return {
            "required": required,
            "optional": {
                "decor_libre": ("STRING", {"default": "", "multiline": True, "tooltip": "decor = libre : ce texte est le décor, tel quel."}),
                "complement": ("STRING", {"default": "", "multiline": True, "tooltip": "Ajouté tel quel après les tags (jamais modifié)."}),
                "decors_unsafe": ("BOOLEAN", {"default": False, "tooltip": "OFF = les décors unsafe (fête foraine, figures, captivité, gore) ne sont jamais tirés. ON = ils peuvent l'être."}),
            },
        }

    RETURN_TYPES = SayaMainPrompt.RETURN_TYPES
    RETURN_NAMES = ("prompt_main", "decor", "nom_court", "info", "prefixe", "scene")
    FUNCTION = "composer"
    CATEGORY = SayaMainPrompt.CATEGORY
    DESCRIPTION = ("Prompt MAIN en un nœud (version française) : préfixe (tel quel) + décor du stock (verrouillé / aléatoire / "
                   "libre, historique, ultra) + tags des menus + complément (tel quel). Même moteur que SayaMainPrompt.")

    @staticmethod
    def _english(decor: str, historique: str, tags_fr: dict[str, str]) -> tuple[str, str, dict[str, str]]:
        groups = {g["name"]: g for g in tag_groups()}
        tags = {}
        for key, label in tags_fr.items():
            name = key[:-3] if key.endswith("_fr") else key
            if name in groups:
                tags[name] = NONE if label == NONE_FR else _from_fr(groups[name].get("fr", {}), label)
        return _from_fr(CATEGORIES_FR, decor), _from_fr(HISTORY_FR, historique), tags

    @classmethod
    def IS_CHANGED(cls, prefixe, decor, seed, historique=HISTORY_FR[picker.NEW], ultra_detaille=False, decor_libre="", complement="", decors_unsafe=False, **tags_fr):
        background, history, tags = cls._english(decor, historique, tags_fr)
        return SayaMainPrompt.IS_CHANGED(prefixe, background, seed, history, ultra_detaille, decor_libre, complement, decors_unsafe, **tags)

    def composer(self, prefixe: str, decor: str, seed: int, historique: str = HISTORY_FR[picker.NEW], ultra_detaille: bool = False,
                 decor_libre: str = "", complement: str = "", decors_unsafe: bool = False, **tags_fr: str) -> tuple[str, str, str, str, str, str]:
        background, history, tags = self._english(decor, historique, tags_fr)
        return SayaMainPrompt().compose(prefixe, background, seed, history, ultra_detaille, decor_libre, complement, decors_unsafe, **tags)
