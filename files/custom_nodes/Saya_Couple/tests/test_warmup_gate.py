"""SayaWarmupGate: the original latent goes through untouched; the discarded warm-up output is never used."""

import torch

from harness import Check, load_pack


def test_gate_returns_original_latent():
    gate = load_pack().NODE_CLASS_MAPPINGS["SayaWarmupGate"]()
    c = Check("warmup_gate")
    original = {"samples": torch.randn(1, 16, 8, 8)}
    warmup = {"samples": torch.randn(1, 16, 8, 8)}
    (out,) = gate.gate(original, warmup)
    c.ok(out is original, "the very same latent object is returned")
    c.ok(torch.equal(out["samples"], original["samples"]), "samples unchanged")
    return c.report()


TESTS = (test_gate_returns_original_latent,)
