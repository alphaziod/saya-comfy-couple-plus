# GUIDE DE PROMPTS SFW — Saya Couple (appartenance dynamique, Saya Couple 2.0 — 05/10/2026)

> Guide du mode d’appartenance **dynamic** (Saya Couple 2.0). Basé sur : grammaire duo r6 (30/30 duos),
> lots G/H/I (900 images, revues Codex), tests J/K, validation SFW du fond MAIN (v19), crash test 100 persos.

## 1. Les 4 champs + le négatif

```
┌──────────────────────────────────────────────────────────────────────────┐
│ MAIN  = le FOND uniquement                                               │
│   qualité, triggers LoRA, style, décor, lumière, palette                 │
│   → reçu seul par les pixels de fond (fond = MAIN)                       │
├──────────────────────────────────────────────────────────────────────────┤
│ ACTION = le support du fond : ce que les persos font DANS l'espace       │
│   comptage, cadrage, qui est à GAUCHE / DROITE, pose, gestes, visages     │
│   → reçu par les persos (MAIN + ACTION), jamais par le fond              │
├──────────────────────────────────────────────────────────────────────────┤
│ P1 = apparence du perso de DROITE  (identité, ancres)                    │
│ P2 = apparence du perso de GAUCHE  (identité, ancres)                    │
├──────────────────────────────────────────────────────────────────────────┤
│ NÉGATIF = anti-qualité + anti-encart (+ anti-nudité en SFW)              │
└──────────────────────────────────────────────────────────────────────────┘
```

**Câblage** (nœud `SayaMultiCouple`) :
- `ownership` = `dynamic` ;
- `anchor_tokens` = `phrase_anatomy` ;
- entrée `action` = un `CLIPTextEncode` séparé (le texte ACTION).

Quand `action` est branché, le nœud active tout seul `background_main` : le fond reçoit MAIN seul, les persos MAIN + ACTION.

Sans `action` branché, mets le texte ACTION à la fin de MAIN : le fond reçoit alors aussi la pose, ce qui donne plus d'encarts.

**Ordre gauche / droite :** P2 est à GAUCHE, P1 à DROITE. Le mot « LEFT » de l'ACTION désigne donc toujours P2.

---

## 2. MAIN — le fond

### 2.0 — le nœud MAIN

Depuis Saya Couple 2.0 le texte MAIN peut être construit par un seul nœud, **Saya Prompt MAIN · Préfixe + Décor + Tags** (`SayaMainPromptFR`, libellés anglais : `SayaMainPrompt`) : `prefixe` (tes tags de qualité et triggers LoRA, copiés tels quels), un décor tiré du stock de 7 409 décors classés (41 catégories, `seed` fixe = verrouillé, randomize = nouveau décor à chaque run, `libre` = ton texte, `ultra_detaille` = la version longue écrite à la main), un tag par menu déroulant (lumière, moment, météo, particules, palette, détail), et `complement`. L'ordre suit les 9 blocs ci-dessous. La Phase 1 lit le propriétaire de chaque pixel dans l'attention du modèle (`ownership = dynamic`) et le nœud *Saya Ownership Map* transmet cette carte à toutes les passes suivantes.

MAIN est le seul texte que reçoivent les pixels de fond, et les persos le reçoivent aussi. Il décrit **le lieu, les objets, la lumière et l'ambiance**, jamais une personne ni un geste. Un fond **dense** aide le masque : plus le décor a d'objets et de matières, mieux le modèle sépare le fond des corps.

### 2.1 L'ordre (9 blocs)

```
[1 qualité], [2 triggers LoRA], [3 lieu : indoors/outdoors + endroit],
[4 objets et mobilier : 5 à 10], [5 lumière : moment + source], [6 météo / ciel],
[7 particules / atmosphère], [8 palette], [9 profondeur / détail]
```

Exemple :

```
best quality, very aesthetic, absurdres, highres, <your LoRA triggers>,
indoors, (detailed cozy bedroom:1.2),
wooden floor, rumpled bed with many pillows, bookshelf full of books, potted plant, desk lamp, curtains, carpet,
evening, warm lamplight, sunset light through the window, light rays, dust particles,
warm colors, depth of field, rich detailed background
```

### 2.2 Blocs 1–2 — Qualité et triggers

- **Préfixe validé :** `best quality, very aesthetic, absurdres, highres` ✓. Le checkpoint validé (base Illustrious, classé « no tags ») n'exige pas d'autres tags de qualité. Ne pas en empiler davantage.
- **Triggers :** `<your LoRA triggers>` ✓, une seule fois (§6).
- **Note de classement** ○ : `general` / `explicit`. C'est une convention Illustrious, pas testée ici. À essayer seule sur 10 images.

### 2.3 Bloc 3 — Le lieu (tags Danbooru les plus utilisés)

Commence par `indoors` ou `outdoors`, puis **un** lieu principal. Le pondérer est permis, comme le MAIN de Saya : `(highly detailed cluttered anime gamer bedroom:1.2)`.

| Intérieur | Extérieur | Fantastique / autre |
|---|---|---|
| `bedroom`, `living room`, `kitchen` | `forest`, `bamboo forest`, `flower field` | `ruins`, `castle`, `throne room` |
| `bathroom`, `onsen`, `hot spring` | `beach`, `ocean`, `lake`, `riverbank` | `cathedral`, `shrine`, `temple` |
| `classroom`, `library`, `office` | `city`, `street`, `alley`, `rooftop` | `dungeon`, `cave`, `floating island` |
| `cafe`, `bar (place)`, `restaurant` | `park`, `garden`, `courtyard` | `space station`, `spaceship interior` |
| `hallway`, `dressing room`, `ballroom` | `mountain`, `cliff`, `desert`, `snowfield` | `cyberpunk city`, `steampunk workshop` |

### 2.4 Bloc 4 — Objets et mobilier (5 à 10, ils rendent le fond dense)

- **Sol et murs :** `wooden floor`, `tatami`, `carpet`, `stone floor`, `brick wall`, `wallpaper`, `shoji`, `sliding doors`.
- **Meubles :** `bed`, `pillow`, `blanket`, `couch`, `chair`, `table`, `desk`, `bookshelf`, `cabinet`, `fireplace`.
- **Objets :** `book`, `potted plant`, `flower vase`, `teapot`, `cup`, `candle`, `lamp`, `lantern`, `clock`, `globe`, `rug`, `cushion`, `stuffed toy`.
- **Végétation :** `plant`, `flower`, `ivy`, `tree`, `cherry blossoms`, `falling petals`, `leaves`.

### 2.5 Blocs 5–7 — Lumière, moment, météo, atmosphère

| Moment | Source de lumière | Effet de lumière | Ciel / météo | Particules |
|---|---|---|---|---|
| `day`, `morning`, `dawn` | `sunlight`, `moonlight` | `light rays`, `sunbeam` | `blue sky`, `cloud`, `cloudy sky` | `light particles`, `sparkle` |
| `sunset`, `dusk`, `twilight` | `candlelight`, `lamp`, `lantern` | `dappled sunlight`, `backlighting` | `starry sky`, `aurora`, `moon` | `dust`, `steam`, `smoke` |
| `evening`, `night` | `chandelier`, `neon lights`, `fireplace` | `sidelighting`, `dim lighting`, `lens flare` | `rain`, `snow`, `fog` | `petals`, `snowflakes`, `embers` |

`backlighting` et `dim lighting` assombrissent les visages : garde-les pour l'ambiance, pas pour une scène d'identité.

### 2.6 Blocs 8–9 — Palette et profondeur

- **Palette :** `warm colors`, `cool colors`, `pastel colors`, `muted colors`, `colorful`, ou un thème (`blue theme`, `pink theme`…).
- **Profondeur :** `depth of field` ✓, `rich detailed background` ✓, `rich layered background` ✓. Évite `blurry background` et `bokeh` forts : le fond devient flou, donc moins dense.

### 2.7 Interdits dans MAIN

- **Personnes et gestes :** tout ce qui parle d'un perso va dans l'ACTION ou dans P1/P2.
- **Fonds vides :** `simple background`, `white background`, `black background`, `grey background`, `gradient background`, « studio », « spotlight on black ». Le fond devient uni, sans repère pour le masque.
- **Objets qui créent une personne ou un encart :** `mirror`, `reflection`, `statue`, `doll`, `portrait`, `painting \(object\)`, `poster \(object\)`, `photo \(object\)`, `television`, `monitor`, écrans, canevas, croquis, et une fenêtre sombre la nuit (elle reflète les persos).
  Le MAIN d'origine de Saya (nœud 144) contient `dual monitors` : c'est un candidat à surveiller s'il y a des encarts dans cette scène.
- **Styles qui retirent la couleur :** `monochrome`, `greyscale`, `sepia`, `lineart`. Le masque se base sur les couleurs des cheveux et des yeux.
- **Thèmes horreur ou transformation** (`horror \(theme\)`, corruption…) dans une scène de contrôle d'identité.

---

## 3. ACTION — qui sont les persos, où ils sont, ce qu'ils font

L'ACTION est le « support du fond » : elle dit au modèle **combien** de persos il y a, **où** chacun est ancré dans l'espace, **comment** il se tient, ce qu'ils font **ensemble** et **où sont les visages**. C'est le champ qui a le plus d'impact sur la composition.

Les persos reçoivent MAIN + ACTION ; le fond ne la reçoit jamais.

Légende : ✓ = validé dans nos lots · ○ = tag Danbooru fréquent, pas encore testé ici.

*État des tests :* dans les lots I/J/K, le comptage et le cadrage (blocs 1–2) étaient encore dans le préfixe de MAIN. Les placer dans l'ACTION suit la règle « fond = MAIN » : le fond ne reçoit plus « 2girls », donc moins de personnes apparaissent dans le décor. À confirmer sur un petit lot (10 images) avant un lot complet.

### 3.1 L'ordre (8 blocs, une ligne chacun)

```
[1 population], [2 cadrage / caméra],
[3 ancrage gauche/droite + profondeur/hauteur + support],
[4 posture de chacun], [5 TAG D'INTERACTION:1.3], [6 tag secondaire:1.2],
[7 où sont les visages], [8 1–2 gestes courts]
```

Exemple :

```
2girls, 2 adult characters, duo, (fully clothed:1.2), modest clothing,
(upper body:1.2), close-up two-shot, from side, (single frame:1.2), (both faces visible:1.2), heads separated,
the red-streaked brunette woman on the LEFT and the pink-haired woman on the RIGHT, at the same height, sitting on a wide sofa,
both sitting, (head on another's shoulder:1.3), (holding hands:1.2),
both faces in three-quarter view, eye contact,
the red-streaked brunette woman's free hand resting on her knee
```

### 3.2 Bloc 1 — Population (qui)

Le comptage est le socle anti-fusion : sans lui, 17 % des images fusionnent les deux persos.

| Duo | ACTION (population) | Dans chaque perso (P1/P2) |
|---|---|---|
| deux filles | `2girls, 2 adult characters, duo` ✓ | `1girl` |
| fille + garçon | `1girl, 1boy, 2 adult characters, duo` ○ | `1girl` / `1boy` |
| deux garçons | `2boys, 2 adult characters, duo` ○ | `1boy` |
| fille + androgyne / femboy | `1girl, 1boy, duo` ○ | `1boy, otoko no ko` (Danbooru : `trap` / `otoko_no_ko`) |
| perso non humain / indéfini | `1girl, 1other, duo` ○ | `1other, …` |

**Règles :**
- Toujours `2 adult characters`.
- Toujours `duo`, jamais `solo`.
- Le comptage va dans l'ACTION, pas dans P1/P2. Chaque perso garde son propre `1girl` / `1boy`.

**Différences entre les deux (bonus de séparation) :** `height difference` ○, `size difference` ○, `skin color difference` ○. Elles aident le modèle à construire deux corps distincts.

### 3.3 Bloc 2 — Cadrage et caméra

| Tag | Effet | Statut |
|---|---|---|
| `(upper body:1.2)`, `close-up two-shot` | deux têtes et deux torses proches, le meilleur pour l'identité | ✓ |
| `cowboy shot` | jusqu'aux cuisses : poses debout, mains visibles | ○ |
| `full body` | corps entiers, visages plus petits (identité moins sûre) | ○ |
| `from side` | profils : la vue la plus sûre pour deux visages en contact | ✓ (J/K) |
| `from above` / `from below` | allongé vu d'en haut / personnage dominant | ○ |
| `from behind` | **dangereux** : les visages disparaissent ou un encart apparaît | à éviter |
| `dutch angle`, `wide shot` | style, décor plus large | ○ |
| `pov` | **à éviter** en duo : le spectateur devient une 3e personne | ✗ |
| `(single frame:1.2)` | une seule case, pas de BD | ✓ |
| `(both faces visible:1.2)`, `heads separated` | deux visages, deux têtes | ✓ |

### 3.4 Bloc 3 — Ancrage dans l'espace (où)

C'est ce qui place chaque corps sur sa zone du masque. P2 = **LEFT**, P1 = **RIGHT** (toujours).

- **Gauche / droite :** `the [attr P2] on the LEFT and the [attr P1] on the RIGHT` ✓. Un seul attribut par perso, **de cheveux ou de couleur** (« the auburn-haired woman »), jamais un trait d'espèce (`-eared`, `horned`, `tailed`) : l'ACTION s'applique aux deux persos ; en persos génériques : `the woman on the LEFT and the man on the RIGHT`.
- **Hauteur :** `at the same height` (le plus sûr), `slightly above`, `lower in the frame`. Évite `above` / `below` extrêmes : NS-049 a donné une P1 minuscule.
- **Profondeur :** `side by side` ✓, `slightly in front`, `slightly behind`. Évite qu'un corps cache l'autre.
- **Support** (ancre les deux corps au même objet) : `on a wide sofa`, `on the bed`, `on the floor`, `against the wall`, `at the edge of the bed`, `on one wide armchair` ✓, `on a bench`.
- **Orientation l'un par rapport à l'autre :** `face-to-face` ○, `side-by-side` ✓, `back-to-back` ✓, `facing another` ○, `facing away` (cache un visage), `cheek-to-cheek` ○, `forehead-to-forehead` ○.

### 3.5 Bloc 4 — Posture de chacun

| Debout / assis | Bas / allongé | Corps |
|---|---|---|
| `standing` ✓ | `kneeling` | `leaning forward` |
| `sitting` ✓ | `on one knee` | `leaning back` |
| `sitting on lap` | `seiza` | `arched back` |
| `straddling` | `lying`, `on back` | `head tilt` |
| `squatting` | `on side`, `on stomach` | `arms behind back` |
| `reclining` | `all fours` | `hand on own hip` |

Une posture par perso : `the red-streaked brunette woman standing, the pink-haired woman sitting on the desk`. Avec une posture qui baisse un perso (kneeling, on stomach), précise toujours où est son visage (bloc 7).

### 3.6 Bloc 5–6 — Interaction (un tag principal à 1.3, un secondaire à 1.2)

| Contact | Tag | Statut |
|---|---|---|
| bras | `(arm around shoulder:1.3)` | ✓ duo stable |
| étreinte | `(hug:1.3)` / `(hug from behind:1.3)` | ○ / ✓ |
| dos | `(back-to-back:1.3)` | ✓ indispensable pour cette pose |
| tête | `(head on another's shoulder:1.3)` | ✓ contact doux |
| mains | `(holding hands:1.3)` / `(interlocked fingers:1.2)` | ✓ / ○ |
| visage | `(hand on another's face:1.2)`, `(hand on another's head:1.2)` | ○ |
| porté | `(princess carry:1.3)`, `(piggyback:1.3)`, `(carrying:1.3)` | ○ (vérifier les deux visages) |
| genoux | `(lap pillow:1.3)`, `(sitting on lap:1.3)` | ○ |
| baiser | `(kissing:1.3)`, `(cheek kiss:1.2)` | ✓ têtes au contact tenues |
| objet partagé | `(shared blanket:1.3)`, `(shared umbrella:1.3)`, `(sharing food:1.2)` | ✓ / ○ |
| regard | `eye contact`, `looking at another` | ○ (en bloc 7) |
| côte à côte | `(standing side by side:1.2)`, `(sitting side by side:1.2)` | ✓ en secondaire |

### 3.7 Bloc 7 — Où sont les visages (la règle apprise en G/H/I)

- Dis où est **chaque** tête : `both faces in profile`, `her face above looking down`, `her head resting on the pillow, face turned to the camera`, `both faces at the top of the frame`.
- **Pose qui cache un visage = fusion des têtes** : une tête contre le ventre ou le dos de l'autre, un gros plan sur les mains, `facing away`.
- **Vue impossible = encart.** « head turned back over her shoulder looking at… » dans une pose de dos : le modèle dessine un encart pour montrer le visage. Choisis l'angle où les deux visages se voient naturellement : `from side`, profils.
- Regards possibles : `eye contact`, `looking at another`, `looking at viewer` (un seul des deux), `closed eyes`.

### 3.8 Bloc 8 — Gestes

Un ou deux gestes courts, nommés par perso : `the red-streaked brunette woman raising her staff`, `the pink-haired woman holding a mug`. Pas plus de deux : chaque geste est une contrainte de plus sur des corps proches.

### 3.9 Interdits dans l'ACTION

- **Traits d'apparence** (cheveux, yeux, cornes, tenue) : ils vont dans P1/P2. Écrits dans l'ACTION, ils s'appliquent aux deux persos.
- **Négations** (`no merging`, `no one else`) : le mot nié peut s'appliquer quand même.
- **Prose de fusion** : `no gap`, `pressed against`, `wedged`, `tangled`.
- **Gros plan** (`close-up on…`, `extreme focus`) quand les deux visages doivent être visibles.
- **Transformations du corps** dans une scène de contrôle.
- **Miroirs, reflets, clones** : catégorie « effet », comptée à part.

### 3.10 Longueur, ordre et ce qui est partagé

- **Les premiers tags pèsent le plus.** CLIP lit le texte par tranches d'environ 75 tokens (environ 40 à 50 tags courts). Garde la population, le cadrage, LEFT/RIGHT et le tag principal dans la **première tranche**. Les gestes viennent en dernier.
- **Partagé ou propre à un perso ?**
  - Ce qui concerne **les deux** va dans l'ACTION : `both blushing`, `both sweating`, `eye contact`, l'objet qu'elles tiennent ensemble.
  - Ce qui concerne **un seul** perso va dans **son** P1/P2 (voir §4) : son expression, l'état de ses vêtements.
  - Dans l'ACTION, un trait propre à un perso s'appliquerait aux deux.
- **Une seule pose par image.** Deux tags d'interaction principaux à 1.3 (par exemple `hug` + `back-to-back`) se contredisent, et le modèle mélange les deux corps.

---

## 4. P1 / P2 — l'apparence (identité + ancres)

Le masque dynamique trouve chaque perso en lisant ses **ancres**. Ce sont des tags tirés automatiquement de son prompt d'apparence. Un tag ne devient une ancre que s'il est **propre à ce perso** : ses mots ne doivent pas apparaître dans le prompt de l'autre.

**Écrire P1/P2 en tags séparés par des virgules**, chaque trait dans son propre tag :

| Catégorie (ordre de priorité) | Exemples |
|---|---|
| espèce | `long white floppy ears`, `long swept-back horns`, `fox tail`, `angel wings`, `halo`, `antennae`, `fins` |
| cheveux | `deep dark pink hair`, `vivid red streaks`, `short black pixie cut`, `platinum bob`, `twin drills`, `ahoge`, `sidelocks` |
| yeux | `white eyes`, `muted crimson eyes`, `heterochromia`, `slit pupils`, `tsurime` |
| peau | `pale skin`, `dark-skinned`, `freckles`, `whisker markings`, `mole under eye`, `tattoos`, `scales` |
| tenue | `oversized cream sweater`, `obsidian circlet` |
| type de perso | `1girl`, `fox girl`, `elf`, `android`, `1boy`, `tomboy`… (ancre seulement s'il diffère de l'autre) |
| silhouette / parties | `petite`, `curvy`, `muscular`, `thick thighs`, `wide hips`, `red lips`, `sharp teeth`, `black nails` |

**Règles des ancres :**
1. **Couleurs propres à chacun.** Si P2 a « purple hair », « purple eyes » ne sert plus d'ancre pour P1 : le mot « purple » allumerait les deux.
2. **Garde `1girl` dans chaque perso**, pas `2girls`. Il aide le modèle et ne gêne pas.
3. **Pas de pose ni de geste dans P1/P2.** « spread legs », « crossed arms », « hands on hips » appartiennent à l'ACTION, et l'extracteur les ignore.
4. **Un tag de localisation est coupé à la préposition :** « circlet between the horns » → `circlet` (tenue), « hair like water » → `hair`.
5. **Synonymes** comptés comme un seul mot : butt = ass, stomach = belly, fingernails = nails…
6. **Deux persos sans aucun trait propre** (même couleur de cheveux, rien d'autre) → aucune ancre → l'image reste en **split statique** 50/50, volontairement.
7. **Garde une identité validée telle quelle.** Quand tu réutilises un personnage, ne change que la partie visée (tenue, état du moment) : chaque autre tag est une ancre sur laquelle le moteur s'appuie.

### 4.1 Ordre dans un perso, et ce qui change avec la scène

**Ordre conseillé** (les premiers tags pèsent le plus) :

```
[type : 1girl / 1boy, adult], [espèce : oreilles, cornes, queue, ailes], [cheveux : couleur + coiffure],
[yeux], [peau + marques], [silhouette + poitrine], [tenue], [état du moment]
```

**Ce qui change avec la scène mais reste propre à un perso** va à la **fin** de son P1/P2. Ce ne sont pas des ancres d'identité, mais il faut que ça reste dans sa zone :
- **expression :** `smile`, `blush`, `closed eyes`, `open mouth`, `tears`, `embarrassed` ;
- **état des vêtements :** `barefoot`, `rolled-up sleeves`, `loose hair`, `hood up`, `wet clothes` ;
- **marques du moment :** `sweat`, `wet hair`, `messy hair`, `lipstick mark`.

**Poids :** 1.1 à 1.4 sur 2 ou 3 traits clés (comme `(long white floppy ears:1.3)`), pas plus. Au-delà, le perso déborde sur l'autre.

**Longueur :** 15 à 35 tags. Au-delà, les derniers tags (la tenue, l'état du moment) pèsent peu.

---

## 5. Négatif SFW

```
worst quality, low quality, normal quality, score_1, score_2, score_3, artist name, bad anatomy, bad hands,
extra fingers, missing fingers, fused fingers, extra toes, missing toes, extra limbs, malformed limbs, blurry,
lowres, jpeg artifacts, uncanny, revealing clothing, cleavage, exposed skin,
inset, cutout, x-ray, cross-section, speech bubble, thought bubble, spoken heart, poster \(object\),
painting \(object\), picture frame
```

- La 2e ligne de tags (anti-encart, « neg2 ») réduit les encarts, sans les supprimer à 100 %. La vraie cause d'un encart est souvent une vue impossible demandée dans l'ACTION.
- **Ne jamais ajouter** `fused bodies, duplicate person` : ces tags masquent les vrais défauts du masque.

**Ajouts possibles** ○ (tags Danbooru, à tester sur 10 images si un défaut revient) :
- encarts : `chibi inset`, `projected inset`, `split screen`, `multiple views` ;
- bords : `border`, `letterboxed` ;
- texte : `watermark`, `signature`, `english text` ;
- couleur : `monochrome`, `greyscale` ;
- fond vide : `simple background`.

N'en ajoute qu'un groupe à la fois, pour savoir lequel agit.

---

## 6. Écrire les triggers et les poids

**Triggers LoRA, une seule fois, dans le préfixe de MAIN :**

| Trigger | LoRA | Force |
|---|---|---|
| `<style trigger>` | a style LoRA | 0.85 |
| `addmicrodetails` | AddMicroDetails (Illustrious) | 0.25 |
| `scnr` | Scenery Enhancer (Illustrious) | 0.30 |

Jamais dans P1/P2 ni dans l'ACTION : un trigger dans P1 ne s'appliquerait qu'à la zone de P1.

**Syntaxe des poids :**
- **Poids :** `(tag:1.3)`. Les parenthèses sont obligatoires : `muted crimson eyes:1.3` sans parenthèses est lu comme du texte, pas comme un poids.
- **Plages conseillées :** 1.1–1.4 en général, 1.3 pour le tag de pose principal, 1.2 pour le secondaire.
- **Parenthèses d'un tag danbooru :** il faut les échapper, `poster \(object\)`. Sinon elles sont lues comme un poids.
- **Séparateur :** une virgule par tag. Les retours à la ligne comptent comme des virgules pour l'extracteur d'ancres.

---

## 7. Checklist SFW

- [ ] MAIN = fond seulement : décor dense, aucune personne, ni statue, ni miroir, ni écran.
- [ ] Triggers LoRA dans le préfixe de MAIN, une seule fois.
- [ ] ACTION : `2girls, 2 adult characters, duo, (fully clothed:1.2)`, cadrage, `(both faces visible:1.2)`, `heads separated`.
- [ ] ACTION : `the [attr P2] on the LEFT and the [attr P1] on the RIGHT`, un attribut chacune.
- [ ] ACTION : un tag de pose `(…:1.3)`, et l'endroit de chaque visage.
- [ ] Aucune vue impossible, aucune négation, aucune prose de fusion.
- [ ] P1/P2 : apparence seule, tags séparés, au moins un trait propre à chacune (espèce, cheveux, yeux).
- [ ] Négatif SFW + neg2, sans `fused bodies`.
