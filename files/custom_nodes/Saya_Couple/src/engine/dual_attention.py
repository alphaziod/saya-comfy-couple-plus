"""Saya Couple engine: the forced attn2 path of MODEL_1 (MAIN native + P1/P2 on cond rows, fused by
main_locked_delta) and the dynamic P1/P2 ownership (v22).

This is the code that lived in ComfyUI's core (comfy/ldm/modules/attention.py, Saya patch, 2026-09 -> 2026-10-05),
moved VERBATIM into the pack (audit Fable, M1): only the injection changed. The core used to branch on
transformer_options["saya_dual_mode"]; now the proxy installed by ``saya_dual_attention.enable_dual_attention``
(ModelPatcher object patches, see that module) calls ``saya_dual_attn2`` when transformer_options["saya_couple"]
(the payload: p1, p2, mask_1, mask_2, fusion_mode, params) is present. Everything else -- the fusion table, the
zones, the persistence, ``_SAYA_OWNERSHIP_STATE`` read by SayaOwnershipMapCapture -- is unchanged.
Error messages keep their historic "Saya forced attention:" prefix (tests and logs rely on it).
"""

from __future__ import annotations

import json
import os

import torch
import torch.nn.functional as F

from comfy.ldm.modules.attention import optimized_attention

SAYA_LOCKED_DELTA_EPS = 1e-6
# Scales the locked P1/P2 deltas: 1.0 = neutral reference (pre-gain base, bit-identical); 0.78 = recommended balance MAIN/P1/P2.
SAYA_LOCKED_DELTA_PERSON_GAIN = 0.78


def _saya_fusion_main_only(main_out, p1_out, p2_out, mask_1, mask_2):
    return main_out


def _saya_fusion_main_locked_delta(main_out, p1_out, p2_out, mask_1, mask_2):
    main = main_out.float()
    denom = (main * main).sum(dim=-1, keepdim=True).clamp_min(SAYA_LOCKED_DELTA_EPS)
    out = main
    for p_out, mask in ((p1_out, mask_1), (p2_out, mask_2)):
        delta = mask.float() * (p_out.float() - main)
        out = out + SAYA_LOCKED_DELTA_PERSON_GAIN * delta - SAYA_LOCKED_DELTA_PERSON_GAIN * ((delta * main).sum(dim=-1, keepdim=True) / denom * main)
    return out.to(main_out.dtype)


def _saya_fusion_main_locked_delta_dynamic(main_out, p1_out, p2_out, mask_1, mask_2, **params):
    # The masks were already replaced by the dynamic ownership map in saya_dual_attn2.
    return _saya_fusion_main_locked_delta(main_out, p1_out, p2_out, mask_1, mask_2)


# Dynamic ownership (zones, after Bounded Attention, arXiv 2403.16990): the static split is kept until
# start_sigma. Below it, every low-resolution block (<= max_tokens) adds its self-attention affinity and
# the cross-attention maps of each person's anchor words (p1_anchor / p2_anchor: an encoded short prompt
# and the positions of its words) to the step's evidence. When the step ends, k-means on the affinity rows
# splits the frame into zones. Each person's anchor zones: the zone where its anchors lead the other
# person's the most (and are above the frame mean), plus any zone reaching anchor_share of that lead;
# the two persons' best zones must differ. A contested zone (at least contest_share of its pixels clearly
# P1 and as many clearly P2, clearly = P1 - P2 anchor beyond one frame standard deviation) holds both
# persons, e.g. two heads merged by k-means: it is never given to one of them. Only zones where the generic person anchor (person_anchor,
# e.g. "woman") is above the frame mean can be linked: the background never is. A person zone joins a
# person only if it touches (grid neighbours) a zone already given to that person, its self-attention
# tie to that person is >= link_ratio times its tie to the other person and to its mean tie, and less than
# contest_share of its pixels are clearly the other person; two rounds.
# Without an anchor zone for each person, or for any zone left unassigned, the static split is kept.
# Per pixel, a zone owner only becomes active after confirm_steps consecutive identical steps; an active
# owner survives miss_steps steps without confirmation, and falls back to the static split at once when
# the other person is found (which then needs confirm_steps again). A step without an anchor zone for each
# person is skipped (no evidence either way). Ownership is binary and exclusive.
# zone_fallback (off by default): a person zone (person anchor above the frame mean) that is neither assigned,
# contested nor a head anchor zone falls back to the static split as a whole, to the side holding the majority
# of its pixels, instead of being cut pixel by pixel by the split line. Background, contested and head anchor
# zones, and a frame without an anchor zone for each person, keep the pixel-wise static split.
# background_main (off by default): a zone where no person is present (person anchor at or below the frame mean),
# with no person owner (it may still be linked to a touching person zone it is link_ratio times more tied to than
# to the other background zones) and an anchor zone for
# each person in the frame, is background, except its pixels where the person anchor is above the frame mean
# (they keep the static split): it gets MAIN only (both
# person masks 0), so P1/P2 prompts (acts, anatomy) are never drawn into the scenery. Background goes through
# the same persistence as person ownership (confirm_steps / miss_steps).
# main_scene (None by default, set by SayaMultiCouple when an ACTION conditioning is given): the sampler MAIN is
# MAIN + ACTION; on background pixels the MAIN attention is recomputed with this scene-only MAIN, so the pose /
# act words never reach the scenery (no act drawn on a wall, in a bubble or an inset).
SAYA_DYNAMIC_DEFAULTS = {"start_sigma": 5.0, "zones": 8, "anchor_share": 0.5, "contest_share": 0.2, "link_ratio": 1.5,
                         "confirm_steps": 3, "miss_steps": 1, "zone_fallback": False, "background_main": False, "main_scene": None, "max_tokens": 1100, "p1_anchor": None, "p2_anchor": None, "person_anchor": None}
_SAYA_OWNERSHIP_STATE = {}


SAYA_FUSION_MODES = {
    "main_only": (_saya_fusion_main_only, frozenset()),
    "main_locked_delta": (_saya_fusion_main_locked_delta, frozenset()),
    "main_locked_delta_dynamic": (_saya_fusion_main_locked_delta_dynamic, frozenset(SAYA_DYNAMIC_DEFAULTS)),
}


def _saya_heads(t, heads):
    b, n, c = t.shape
    return t.reshape(b, n, heads, c // heads).transpose(1, 2)


def _saya_head_mean_softmax(q, k, heads):
    qh, kh = _saya_heads(q.float(), heads), _saya_heads(k.float(), heads)
    return torch.softmax(qh @ kh.transpose(-1, -2) * qh.shape[-1] ** -0.5, dim=-1).mean(dim=1)


def saya_ownership_evidence(block, n_cond, q_cond, anchors):
    """One low-resolution block: self-attention affinity [B, N, N] and the anchor maps [B, 3, N] (P1, P2, person)."""
    attn2 = block.attn2
    maps = []
    for context, positions in anchors:
        k = attn2.to_k(context.to(q_cond.device, q_cond.dtype)).expand(q_cond.shape[0], -1, -1)
        maps.append(_saya_head_mean_softmax(q_cond, k, attn2.heads)[..., positions].sum(dim=-1))
    anchor = torch.stack(maps, dim=1)
    anchor = (anchor - anchor.mean(dim=-1, keepdim=True)) / anchor.std(dim=-1, keepdim=True).clamp_min(1e-12)
    attn1 = block.attn1
    source = n_cond if not block.disable_self_attn else q_cond
    affinity = _saya_head_mean_softmax(attn1.to_q(source), attn1.to_k(source), attn1.heads)
    return affinity, anchor


SAYA_ZONE_UNASSIGNED, SAYA_ZONE_ANCHOR, SAYA_ZONE_LINKED = 0, 1, 2


def saya_ownership_zones(affinity, anchor, grid, params):
    """k-means zones on the affinity rows, then zone -> owner. Returns labels [B, N], owner [B, N],
    confident [B, N] (both 0/1) and per image the zone owner (-1 / 0 = P1 / 1 = P2) and zone role."""
    zones = int(params["zones"])
    ratio = float(params["link_ratio"])
    feats = F.normalize(affinity, dim=-1)
    labels, owners, confident, zone_owners, zone_roles = [], [], [], [], []
    info = {"contested": [], "person": [], "gate": []}
    height, width = grid
    for x, a, aff in zip(feats, anchor, affinity):
        centers = [x[int(a[0].argmax())]]
        for _ in range(zones - 1):
            distance = 1 - (x @ torch.stack(centers).transpose(0, 1)).max(dim=1).values
            centers.append(x[int(distance.argmax())])
        centers = torch.stack(centers)
        for _ in range(10):
            label = (x @ centers.transpose(0, 1)).argmax(dim=1)
            counts = torch.bincount(label, minlength=zones).clamp_min(1).unsqueeze(1).to(x.dtype)
            centers = F.normalize(torch.zeros_like(centers).index_add_(0, label, x) / counts, dim=-1)
        counts = torch.bincount(label, minlength=zones).clamp_min(1).to(x.dtype)
        zone_anchor = torch.zeros(a.shape[0], zones, device=x.device).index_add_(1, label, a) / counts
        owner = torch.full((zones,), -1, dtype=torch.long, device=x.device)
        lead_pixel = a[0] - a[1]
        clear = lead_pixel.std()
        clear_share = torch.stack([torch.zeros(zones, device=x.device).index_add_(0, label, (sign * lead_pixel > clear).to(x.dtype)) / counts for sign in (1, -1)])
        contested = (clear_share >= float(params["contest_share"])).all(dim=0)
        present = (counts > 1) & ~contested
        if params["background_main"]:
            # An identity word can light the scenery (red streaks on a red wall): with a MAIN-only background, a head
            # anchor zone must also hold a person.
            present = present & (zone_anchor[2] > 0)
        for p in (0, 1):
            lead = torch.where(present & (zone_anchor[p] > 0), zone_anchor[p] - zone_anchor[1 - p], torch.full_like(zone_anchor[p], -1.0))
            best = lead.max()
            if best > 0:
                owner[(lead >= float(params["anchor_share"]) * best) & (owner < 0)] = p
        role = torch.where(owner >= 0, SAYA_ZONE_ANCHOR, SAYA_ZONE_UNASSIGNED)
        gate = bool((owner == 0).any() and (owner == 1).any())
        if gate:
            onehot = F.one_hot(label, zones).to(aff.dtype)
            tie = onehot.transpose(0, 1) @ aff @ onehot / (counts.unsqueeze(1) * counts.unsqueeze(0))
            others = ~torch.eye(zones, dtype=torch.bool, device=x.device)
            mean_tie = (tie * others).sum(dim=1) / (zones - 1)
            grid_label = label.reshape(height, width)
            touch = torch.zeros(zones, zones, dtype=torch.bool, device=x.device)
            for u, v in ((grid_label[:, :-1], grid_label[:, 1:]), (grid_label[:-1], grid_label[1:])):
                touch[u.flatten(), v.flatten()] = True
                touch[v.flatten(), u.flatten()] = True
            for _ in range(2):
                to_p = torch.stack([torch.where(owner == p, tie, torch.zeros_like(tie)).max(dim=1).values for p in (0, 1)])
                best, person = to_p.max(dim=0)
                touches = torch.stack([(touch & (owner == p)).any(dim=1) for p in (0, 1)]).gather(0, person.unsqueeze(0))[0]
                # With background_main a zone below the person mean can be linked too: a body part with a weak "woman"
                # anchor (legs, a crossing arm) must stay with its person instead of becoming MAIN-only background. It must
                # also be link_ratio times more tied to that person than to any other background zone (scenery is not).
                scenery = (owner < 0) & ~contested & (zone_anchor[2] <= 0)
                to_scenery = torch.where(scenery.unsqueeze(0) & others, tie, torch.zeros_like(tie)).max(dim=1).values
                linkable = (zone_anchor[2] > 0) | (bool(params["background_main"]) & (best >= ratio * to_scenery))
                # Never link a zone holding contest_share of pixels clearly the OTHER person (a body or a scenery lit by
                # the other's anchor words would be carried over to the wrong owner).
                other_clear = clear_share.gather(0, (1 - person).unsqueeze(0))[0]
                join = (owner < 0) & ~contested & linkable & touches & (best >= ratio * to_p.min(dim=0).values) & (best >= ratio * mean_tie) \
                    & (other_clear < float(params["contest_share"]))
                owner = torch.where(join, person, owner)
                role = torch.where(join, SAYA_ZONE_LINKED, role)
        else:
            owner.fill_(-1)
            role.fill_(SAYA_ZONE_UNASSIGNED)
        labels.append(label)
        owners.append((owner == 0)[label].float())
        confident.append((owner >= 0)[label].float())
        zone_owners.append(owner)
        zone_roles.append(role)
        info["contested"].append(contested)
        info["person"].append(zone_anchor[2] > 0)
        info["gate"].append(gate)
    info = {"contested": torch.stack(info["contested"]), "person": torch.stack(info["person"]), "gate": info["gate"]}
    return torch.stack(labels), torch.stack(owners), torch.stack(confident), torch.stack(zone_owners), torch.stack(zone_roles), info


def _saya_finish_ownership_step(state, params):
    if state["count"] == 0:
        return
    labels, owner, confident, zone_owner, zone_role, info = saya_ownership_zones(state["affinity"] / state["count"], state["anchor"] / state["count"], state["grid"], params)
    # Step candidate per pixel: 0 = P1, 1 = P2, 2 = background (MAIN only), -1 = none (static split).
    found = torch.where(confident > 0, 1 - owner.long(), torch.full_like(owner, -1, dtype=torch.long))
    if params["background_main"]:
        gate = torch.tensor(info["gate"], device=labels.device).unsqueeze(1)
        # A pixel where the person anchor is above the frame mean is never background, even inside a background zone
        # (a leg or a torso merged with the scenery by k-means): it keeps the static split instead of MAIN only.
        person_pixel = (state["anchor"] / state["count"])[:, 2] > 0
        background = (gate & ~info["person"] & (zone_owner < 0)).gather(1, labels) & ~person_pixel
        found = torch.where(background & (found < 0), torch.full_like(found, 2), found)
    found = found.reshape(owner.shape[0], *state["grid"])
    if state["active"] is None or state["active"].shape != found.shape:
        state.update(active=torch.full_like(found, -1), candidate=torch.full_like(found, -1), streak=torch.zeros_like(found), miss=torch.zeros_like(found))
    active, candidate, streak, miss = state["active"], state["candidate"], state["streak"], state["miss"]
    streak = torch.where((found >= 0) & (found == candidate), streak + 1, (found >= 0).long())
    candidate = found
    confirmed = (active < 0) & (streak >= int(params["confirm_steps"]))
    kept = (active >= 0) & (found == active)
    missed = (active >= 0) & (found < 0)
    miss = torch.where(missed, miss + 1, torch.zeros_like(miss))
    drop = (active >= 0) & (((found >= 0) & (found != active)) | (miss > int(params["miss_steps"])))
    active = torch.where(confirmed, candidate, torch.where(drop, torch.full_like(active, -1), active))
    # A step without an anchor zone for each person carries no evidence (small heads in a wide shot drop in and
    # out): it neither confirms, breaks a streak nor counts as a miss; that image's state is left as it was.
    gated = torch.tensor(info["gate"], device=found.device).view(-1, *([1] * (found.dim() - 1)))
    active, candidate, streak, miss = (torch.where(gated, new, old) for new, old in
                                       ((active, state["active"]), (candidate, state["candidate"]), (streak, state["streak"]), (miss, state["miss"])))
    state.update(active=active, candidate=candidate, streak=streak, miss=miss)
    # Map channel 0 = P1 (1) / P2 (0) / static (0.5), channel 1 = 1 where a person owns the pixel, channel 2 = 1 on background.
    state["map"] = torch.stack([torch.where(active == 0, 1.0, torch.where(active == 1, 0.0, 0.5)), ((active == 0) | (active == 1)).float(),
                                (active == 2).float()], dim=1)
    zone_log = []
    debug = os.environ.get("SAYA_OWNERSHIP_DEBUG")
    if params["zone_fallback"] or debug:
        # Per pixel block owner (0 = P1, 1 = P2, -1 = pixel-wise static split) from the static split majority.
        block = torch.full_like(labels, -1)
        side_p1 = state["static_grid"].flatten(1).expand(labels.shape[0], -1)
        for b in range(labels.shape[0]):
            zones = zone_owner.shape[1]
            counts = torch.bincount(labels[b], minlength=zones).clamp_min(1).to(side_p1.dtype)
            share_p1 = torch.zeros(zones, device=side_p1.device, dtype=side_p1.dtype).index_add_(0, labels[b], side_p1[b]) / counts
            eligible = info["gate"][b] & info["person"][b] & ~info["contested"][b] & (zone_role[b] != SAYA_ZONE_ANCHOR) & (zone_owner[b] < 0)
            choice = torch.where(eligible & (share_p1 > 0.5), 0, torch.where(eligible & (share_p1 < 0.5), 1, -1))
            block[b] = choice[labels[b]]
            for z in range(zones):
                size = int((labels[b] == z).sum())
                if size:
                    zone_log.append({"image": b, "zone": z, "size": size, "split_p1": round(float(share_p1[z]), 3), "split_p2": round(1 - float(share_p1[z]), 3),
                                     "owner": int(choice[z]) if choice[z] >= 0 else int(zone_owner[b, z]), "margin": round(abs(2 * float(share_p1[z]) - 1), 3),
                                     "contested": bool(info["contested"][b, z]), "person": bool(info["person"][b, z]),
                                     "kind": "dynamic" if zone_owner[b, z] >= 0 else ("static_pixel" if choice[z] < 0 else "block" if params["zone_fallback"] else "would_block"),
                                     "head_anchor": bool(zone_role[b, z] == SAYA_ZONE_ANCHOR)})
        if params["zone_fallback"]:
            state["block"] = block.reshape(labels.shape[0], *state["grid"])
    if debug:
        with open(debug, "a") as log:
            log.write(json.dumps({"sigma": round(state["sigma"], 4), "grid": state["grid"],
                                  "p1": round(float((active == 0).float().mean()), 4), "p2": round(float((active == 1).float().mean()), 4),
                                  "static": round(float((active < 0).float().mean()), 4), "background": round(float((active == 2).float().mean()), 4),
                                  "found_p1": round(float((found == 0).float().mean()), 4), "found_p2": round(float((found == 1).float().mean()), 4),
                                  "zones": zone_log}) + "\n")
        torch.save({"map": state["map"].detach().cpu(), "labels": labels.reshape(-1, *state["grid"]).cpu(),
                    "anchor": (state["anchor"] / state["count"]).reshape(-1, 3, *state["grid"]).cpu(),
                    "zone_owner": zone_owner.cpu(), "zone_role": zone_role.cpu(), "found": found.cpu(), "streak": streak.cpu(),
                    "block": state["block"].cpu() if state.get("block") is not None else torch.empty(0),
                    # Evidence of the step, for an offline replay of the zone / link / persistence rules (large: opt-in).
                    "affinity": (state["affinity"] / state["count"]).half().cpu() if os.environ.get("SAYA_OWNERSHIP_DEBUG_AFFINITY") else torch.empty(0)},
                   f"{debug}.sigma{state['sigma']:.3f}.pt")


def saya_dynamic_masks(block, n, q, dual, cond_chunks, batch, height, width, masks, transformer_options):
    params = {**SAYA_DYNAMIC_DEFAULTS, **dual["params"]}
    sigmas = transformer_options.get("sigmas")
    if not torch.is_tensor(sigmas):
        raise RuntimeError("Saya forced attention: dynamic ownership needs transformer_options['sigmas']")
    sigma = float(sigmas.max())
    key = id(dual["p1"])
    state = _SAYA_OWNERSHIP_STATE.get(key)
    if state is None or sigma > state["sigma"]:
        _SAYA_OWNERSHIP_STATE.clear()
        state = _SAYA_OWNERSHIP_STATE[key] = {"sigma": sigma, "map": None, "grid": None, "count": 0, "active": None, "block": None}
    elif sigma < state["sigma"]:
        _saya_finish_ownership_step(state, params)
        state.update(sigma=sigma, count=0)
    if sigma > float(params["start_sigma"]) or any(params[k] is None for k in ("p1_anchor", "p2_anchor", "person_anchor")):
        return masks
    if height * width <= int(params["max_tokens"]) and state["grid"] in (None, [height, width]):
        def cond(t):
            return torch.cat([t[i * batch:(i + 1) * batch] for i in cond_chunks])
        affinity, anchor = saya_ownership_evidence(block, cond(n), cond(q), (params["p1_anchor"], params["p2_anchor"], params["person_anchor"]))
        if state["count"] == 0:
            if state["grid"] is None:
                static = dual["mask_1"].unsqueeze(0) if dual["mask_1"].ndim == 2 else dual["mask_1"]
                state["static_grid"] = F.interpolate(static.unsqueeze(1).to(q.device, torch.float32), size=(height, width), mode="area")[:, 0]
            state.update(grid=[height, width], affinity=affinity, anchor=anchor, count=1)
        else:
            state.update(affinity=state["affinity"] + affinity, anchor=state["anchor"] + anchor, count=state["count"] + 1)
    if state["map"] is None or state["map"].shape[0] != len(cond_chunks) * batch:
        return masks
    own = F.interpolate(state["map"], size=(height, width), mode="nearest-exact").to(q.device)
    owner = (own[:, 0].flatten(1).unsqueeze(-1) > 0.5).float()
    confident = own[:, 1].flatten(1).unsqueeze(-1) > 0.5
    if state.get("block") is not None:
        block = F.interpolate(state["block"].unsqueeze(1).float(), size=(height, width), mode="nearest-exact")[:, 0].flatten(1).unsqueeze(-1).to(q.device)
        blocked = ~confident & (block >= 0)
        owner = torch.where(blocked, (block == 0).float(), owner)
        confident = confident | blocked
    background = own[:, 2].flatten(1).unsqueeze(-1) > 0.5
    dynamic = []
    for static, target in ((masks[0], owner), (masks[1], 1 - owner)):
        mask = static.clone()
        for j, i in enumerate(cond_chunks):
            rows = slice(i * batch, (i + 1) * batch)
            mask[rows] = torch.where(confident[j * batch:(j + 1) * batch], target[j * batch:(j + 1) * batch].to(mask.dtype), static[rows])
            mask[rows] = torch.where(background[j * batch:(j + 1) * batch], torch.zeros_like(mask[rows]), mask[rows])
        dynamic.append(mask)
    return dynamic


def saya_scene_main(attn2, q, main_out, dual, cond_chunks, batch, height, width, transformer_options):
    """Background pixels (cond elements) take the MAIN attention of the scene-only MAIN (no ACTION words)."""
    scene = dual["params"].get("main_scene")
    state = _SAYA_OWNERSHIP_STATE.get(id(dual["p1"]))
    if scene is None or state is None or state["map"] is None or state["map"].shape[0] != len(cond_chunks) * batch:
        return main_out
    background = F.interpolate(state["map"][:, 2:3], size=(height, width), mode="nearest-exact")[:, 0].flatten(1).unsqueeze(-1).to(q.device) > 0.5
    if not background.any():
        return main_out
    q_cond = torch.cat([q[i * batch:(i + 1) * batch] for i in cond_chunks])
    scene = scene.to(q.device, q.dtype)
    k = attn2.to_k(scene).expand(q_cond.shape[0], -1, -1).contiguous()
    v = attn2.to_v(scene).expand(q_cond.shape[0], -1, -1).contiguous()
    scene_out = attn2.to_out(optimized_attention(q_cond, k, v, attn2.heads, attn_precision=attn2.attn_precision, transformer_options=transformer_options))
    out = main_out.clone()
    for j, i in enumerate(cond_chunks):
        rows = slice(i * batch, (i + 1) * batch)
        out[rows] = torch.where(background[j * batch:(j + 1) * batch], scene_out[j * batch:(j + 1) * batch], main_out[rows])
    return out


def saya_regional_fusion(mode, main_out, p1_out, p2_out, mask_1, mask_2, params):
    if mode not in SAYA_FUSION_MODES:
        raise RuntimeError(f"Saya forced attention: unknown fusion_mode {mode!r}, known: {sorted(SAYA_FUSION_MODES)}")
    fusion, allowed = SAYA_FUSION_MODES[mode]
    unknown = set(params) - allowed
    if unknown:
        raise RuntimeError(f"Saya forced attention: fusion_mode {mode!r} does not accept parameters {sorted(unknown)}")
    out = fusion(main_out, p1_out, p2_out, mask_1, mask_2, **params)
    if out.shape != main_out.shape or out.dtype != main_out.dtype:
        raise RuntimeError(f"Saya forced attention: fusion returned {tuple(out.shape)} {out.dtype}, want {tuple(main_out.shape)} {main_out.dtype}")
    return out


def saya_dual_check_hooks(block, transformer_options):
    # M1 (2026-10-05): no core flag any more, the payload itself is the switch.
    if not isinstance(transformer_options.get("saya_dual"), dict):
        raise RuntimeError("Saya forced attention: transformer_options['saya_dual'] must be a dict")
    if "optimized_attention_override" in transformer_options:
        raise RuntimeError("Saya forced attention: optimized_attention_override is forbidden")
    transformer_patches = transformer_options.get("patches", {})
    for name in ("attn2_patch", "attn2_output_patch"):
        if transformer_patches.get(name):
            raise RuntimeError(f"Saya forced attention: {name} is forbidden")
    replaced = transformer_options.get("patches_replace", {}).get("attn2")
    if replaced:
        raise RuntimeError(f"Saya forced attention: attn2 patches_replace is forbidden, found keys {list(replaced)}")
    if block.switch_temporal_ca_to_sa:
        raise RuntimeError("Saya forced attention: switch_temporal_ca_to_sa blocks have no nominal MAIN context")


def saya_dual_attn2(block, n, context, transformer_options):
    dual = transformer_options.get("saya_dual")
    fields = {"p1", "p2", "mask_1", "mask_2", "fusion_mode", "params"}
    if not isinstance(dual, dict) or set(dual) != fields:
        raise RuntimeError(f"Saya forced attention: transformer_options['saya_dual'] must be a dict with exactly {sorted(fields)}")
    if not isinstance(dual["fusion_mode"], str) or not isinstance(dual["params"], dict):
        raise RuntimeError("Saya forced attention: fusion_mode must be a string and params a dict")
    for key in ("cond_or_uncond", "activations_shape"):
        if key not in transformer_options:
            raise RuntimeError(f"Saya forced attention: transformer_options['{key}'] is missing")
    cond_or_uncond = [int(flag) for flag in transformer_options["cond_or_uncond"]]
    if not cond_or_uncond or any(flag not in (0, 1) for flag in cond_or_uncond) or n.shape[0] % len(cond_or_uncond):
        raise RuntimeError(f"Saya forced attention: cond_or_uncond {cond_or_uncond} does not match batch {n.shape[0]}")
    batch = n.shape[0] // len(cond_or_uncond)
    height, width = transformer_options["activations_shape"][-2:]
    if height * width != n.shape[1]:
        raise RuntimeError(f"Saya forced attention: activations {height}x{width} do not match {n.shape[1]} tokens")

    attn2 = block.attn2
    q = attn2.to_q(n)
    k = attn2.to_k(context)
    v = attn2.to_v(context)
    if k.shape[0] != q.shape[0]:
        raise RuntimeError("Saya forced attention: nominal context batch does not match the query batch")
    main_out = attn2.to_out(optimized_attention(q, k, v, attn2.heads, attn_precision=attn2.attn_precision, transformer_options=transformer_options))

    cond_chunks = [i for i, flag in enumerate(cond_or_uncond) if flag == 0]
    p_outs = []
    masks = []
    for name in ("1", "2"):
        p_context = dual["p" + name]
        if not torch.is_tensor(p_context) or p_context.ndim != 3 or p_context.shape[0] != 1 or p_context.shape[-1] != context.shape[-1]:
            raise RuntimeError(f"Saya forced attention: p{name} must be a [1, tokens, {context.shape[-1]}] tensor")
        mask = dual["mask_" + name]
        if not torch.is_tensor(mask) or mask.ndim not in (2, 3):
            raise RuntimeError(f"Saya forced attention: mask_{name} must be a [H, W] or [B, H, W] tensor")
        mask = mask.unsqueeze(0) if mask.ndim == 2 else mask
        if mask.shape[0] not in (1, batch):
            raise RuntimeError(f"Saya forced attention: mask_{name} batch {mask.shape[0]} does not match {batch}")
        if not torch.isfinite(mask).all():
            raise RuntimeError(f"Saya forced attention: mask_{name} is not finite")
        p_out = torch.zeros_like(main_out)
        full_mask = torch.zeros_like(q[:, :, :1])
        if cond_chunks:
            mask = F.interpolate(mask.unsqueeze(1).to(q.device, torch.float32), size=(height, width), mode="nearest-exact")
            mask = mask[:, 0].flatten(1).unsqueeze(-1).expand(batch, -1, -1).to(q.dtype)
            p_context = p_context.to(q.device, q.dtype)
            q_cond = torch.cat([q[i * batch:(i + 1) * batch] for i in cond_chunks])
            k_p = attn2.to_k(p_context).expand(q_cond.shape[0], -1, -1).contiguous()
            v_p = attn2.to_v(p_context).expand(q_cond.shape[0], -1, -1).contiguous()
            p_cond = attn2.to_out(optimized_attention(q_cond, k_p, v_p, attn2.heads, attn_precision=attn2.attn_precision, transformer_options=transformer_options))
            for j, i in enumerate(cond_chunks):
                p_out[i * batch:(i + 1) * batch] = p_cond[j * batch:(j + 1) * batch]
                full_mask[i * batch:(i + 1) * batch] = mask
        p_outs.append(p_out)
        masks.append(full_mask)

    if dual["fusion_mode"] == "main_locked_delta_dynamic" and cond_chunks:
        masks = saya_dynamic_masks(block, n, q, dual, cond_chunks, batch, height, width, masks, transformer_options)
        main_out = saya_scene_main(attn2, q, main_out, dual, cond_chunks, batch, height, width, transformer_options)
    return saya_regional_fusion(dual["fusion_mode"], main_out, p_outs[0], p_outs[1], masks[0], masks[1], dual["params"])
