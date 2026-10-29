#!/usr/bin/env python3
"""Découpe ILUSTCOUPLECLEAN.json en 6 workflows de passe + 1 workflow maître (config).

Chaque passe garde TOUT ce qui lui appartient : son subgraph de phase complet, son Stop/Review,
ses previews/saves, ses comparateurs, ses toggles (détailleurs en passe 5) et les hubs de config
qu'elle lit (modèles, VAE, routings, couple/solo, upscale). Les autres passes sont retirées.
Le maître contient toute la config partagée et aucune phase ; `synchro_maitre.py` recopie ses
réglages dans les 6 passes (mêmes ids de nodes).

Usage : decouper_passes.py SOURCE.json DOSSIER_SORTIE
"""
import copy, json, sys
from pathlib import Path

PHASES = {1: [100033, 100034], 2: [100050], 3: [100055], 4: [100057], 5: [2674], 6: [100061]}
NOMS = {1: "Generation", 2: "Hires_Fix", 3: "HiDream", 4: "Post_HiDream", 5: "Detailers", 6: "Final_Upscale"}


def charger(src):
    wf = json.loads(Path(src).read_text())
    nodes = {n["id"]: n for n in wf["nodes"]}
    sgs = {s["id"]: s for s in wf.get("definitions", {}).get("subgraphs", [])}
    links = {l[0]: l for l in wf["links"]}
    sets = {n["widgets_values"][0]: n["id"] for n in wf["nodes"] if n["type"] == "SetNode"}
    return wf, nodes, sgs, links, sets


def gets_internes(sgs, sid, vus):
    noms = set()
    for n in sgs[sid].get("nodes", []):
        if n["type"] == "GetNode":
            noms.add(n["widgets_values"][0])
        elif n["type"] in sgs and n["type"] not in vus:
            vus.add(n["type"])
            noms |= gets_internes(sgs, n["type"], vus)
    return noms


def amont(i, nodes, sgs, links, sets):
    n = nodes[i]
    r = [links[x["link"]][1] for x in n.get("inputs", []) if x.get("link") in links]
    noms = set()
    if n["type"] == "GetNode":
        noms.add(n["widgets_values"][0])
    if n["type"] in sgs:
        noms |= gets_internes(sgs, n["type"], {n["type"]})
    r += [sets[k] for k in noms if k in sets]
    return r


def aval(i, nodes, links):
    return [links[l][3] for o in nodes[i].get("outputs", []) for l in (o.get("links") or []) if l in links]


def fermeture(depart, voisins):
    vus, pile = set(depart), list(depart)
    while pile:
        for v in voisins(pile.pop()):
            if v not in vus:
                vus.add(v)
                pile.append(v)
    return vus


def sous_graphes_utilises(sgs, types):
    vus, pile = set(), [t for t in types if t in sgs]
    while pile:
        t = pile.pop()
        if t in vus:
            continue
        vus.add(t)
        pile += [n["type"] for n in sgs[t].get("nodes", []) if n["type"] in sgs]
    return vus


def dans(group, node):
    gx, gy, gw, gh = group["bounding"]
    x, y = node["pos"][0], node["pos"][1]
    return gx <= x <= gx + gw and gy <= y <= gy + gh


def de_passe(n, p):
    return n.get("properties", {}).get("saya_phase") == p


def extraire(wf, sgs, garder, actifs, titre, phase=None, requis=()):
    out = copy.deepcopy(wf)
    out["id"] = None
    out["nodes"] = [copy.deepcopy(n) for n in wf["nodes"] if n["id"] in garder]
    for n in out["nodes"]:
        props = n.setdefault("properties", {})
        if n["id"] in actifs or (phase and de_passe(n, phase)) or (phase is None and titre == "maitre" and "saya_original_mode" in props):
            n["mode"] = props.get("saya_original_mode", 0)
        elif n["id"] in requis and n.get("mode") == 2:
            n["mode"] = 0  # config lue par une phase : muette, la phase planterait faute d'entrée.
        # Une sortie vers un node retiré disparaît, l'entrée aussi.
    ids_links = {l[0] for l in wf["links"] if l[1] in garder and l[3] in garder}
    out["links"] = [l for l in wf["links"] if l[0] in ids_links]
    for n in out["nodes"]:
        for x in n.get("inputs", []):
            if x.get("link") is not None and x["link"] not in ids_links:
                x["link"] = None
        for o in n.get("outputs", []):
            if o.get("links"):
                o["links"] = [l for l in o["links"] if l in ids_links]
    utilises = sous_graphes_utilises(sgs, {n["type"] for n in out["nodes"]})
    out["definitions"]["subgraphs"] = [s for s in wf["definitions"]["subgraphs"] if s["id"] in utilises]
    out["groups"] = [g for g in wf.get("groups", []) if any(dans(g, n) for n in out["nodes"])]
    out.setdefault("extra", {})["saya_split"] = titre
    out["id"] = None
    return out


def main(src, dst):
    wf, nodes, sgs, links, sets = charger(src)
    tous_phase = {i for ids in PHASES.values() for i in ids}
    voisins_amont = lambda i: amont(i, nodes, sgs, links, sets)
    config_total = set()
    passes = {}
    for p, ids in PHASES.items():
        # Sorties de la passe : tout ce qu'elle alimente (stop, review, saves, previews, comparateurs).
        sorties = fermeture(ids, lambda i: [v for v in aval(i, nodes, links) if v not in tous_phase])
        # Toggles de la passe (ex. bypassers des détailleurs) repérés par saya_phase.
        sorties |= {i for i, n in nodes.items() if n.get("properties", {}).get("saya_phase") == p
                    and i not in tous_phase}
        garder = fermeture(sorties, lambda i: [u for u in voisins_amont(i) if u not in tous_phase or u in ids])
        garder -= tous_phase - set(ids)
        config_total |= garder - sorties
        requis = fermeture(ids, lambda i: [u for u in voisins_amont(i) if u not in tous_phase])
        passes[p] = (garder, sorties, requis)
    # Le maître : toute la config partagée (+ tous les SetNode et leur amont), aucune phase ni sortie.
    config_total |= fermeture([i for i, n in nodes.items() if n["type"] == "SetNode"], voisins_amont)
    config_total -= tous_phase
    config_total = {i for i in config_total if not (nodes[i].get("title") or "").startswith("Compare")}
    dst = Path(dst)
    dst.mkdir(parents=True, exist_ok=True)
    rapport = {}
    requis_tous = set().union(*(r for _, _, r in passes.values()))
    maitre = extraire(wf, sgs, config_total, set(), "maitre", requis=requis_tous)
    (dst / "ILUSTCOUPLE_00_MAITRE_CONFIG.json").write_text(json.dumps(maitre, ensure_ascii=False, indent=1))
    rapport["maitre"] = len(maitre["nodes"])
    for p, (garder, sorties, requis) in passes.items():
        # La passe 1 est la phase active du workflow source : ses modes restent ceux de Saya.
        out = extraire(wf, sgs, garder, sorties if p > 1 else set(), f"passe {p}", p if p > 1 else None, requis)
        nom = f"ILUSTCOUPLE_{p:02d}_PASSE_{p}_{NOMS[p]}.json"
        (dst / nom).write_text(json.dumps(out, ensure_ascii=False, indent=1))
        rapport[nom] = len(out["nodes"])
    rapport["source"] = len(wf["nodes"])
    print(json.dumps(rapport, indent=1, ensure_ascii=False))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
