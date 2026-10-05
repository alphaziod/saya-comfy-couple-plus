"""Deprecated node types (audit Fable 2026-10-05, P-C validated by Saya).

These types are not used by the reference workflow any more. They stay loadable: an old workflow that contains
one keeps working, the node just sits in the ``saya/deprecated`` category and logs ONE warning per process when
it runs. Nothing is removed in this version; removal will come later, once no workflow of the family uses them.

Each deprecated entry is a SUBCLASS of the original class, so the original (which may be a parent of live nodes,
e.g. SayaImagePhaseCheckpointLoad -> the fixed Phase N loaders, or an alias of a live class, e.g.
SayaNear4KTargetCalculator = SayaUpscaleTargetCalculator) is never touched.
"""

from __future__ import annotations

import functools
import logging

LOGGER = logging.getLogger(__name__)
CATEGORY = "saya/deprecated"
_warned: set[str] = set()


def deprecate(cls, name: str, replacement: str):
    """A registrable subclass of ``cls``: same inputs/outputs/function, category saya/deprecated, one warning at
    first execution."""
    function = getattr(cls, cls.FUNCTION)

    @functools.wraps(function)
    def run(self, *args, **kwargs):
        if name not in _warned:
            _warned.add(name)
            LOGGER.warning("[Saya Couple] node %s is deprecated (%s); it still works but will be removed in a later version.", name, replacement)
        return function(self, *args, **kwargs)

    note = f"DEPRECATED ({replacement}). " + str(getattr(cls, "DESCRIPTION", "") or "")
    return type(name, (cls,), {cls.FUNCTION: run, "CATEGORY": CATEGORY, "DESCRIPTION": note.strip(), "__module__": __name__,
                               "saya_deprecated_from": cls})
