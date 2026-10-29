# V11 — Fusion de workflows : pré-architecture du pipeline multi-passes (Tâche 4)

Branche : `glm/v11-fusion` (worktree `saya-studio-v11/fusion`). Base commune : `ff616a2`.
Auteur : agent Tâche 4. Destinataire de l'intégration : Astra (orchestration/exécution).

**Statut : PRÊT À INTÉGRER (couche contrats uniquement).** Types, manifeste versionné,
validations pures et tests sont livrés et verts. Aucune orchestration, aucune soumission
ComfyUI, aucun appel réseau, aucune fusion de graphes JSON : ces éléments restent à
construire par Astra au-dessus de ces contrats (voir « Ce qu'Astra construit »).

## Périmètre respecté

- Nouveau module isolé `crates/studio-core/src/workflow_pipeline.rs` + déclaration `pub mod`
  dans `lib.rs`. Modifications strictement additives : aucun module existant modifié.
- Le module est pur : pas d'accès base de données, pas de filesystem, pas de réseau.
- Les workflows restent des documents API distincts référencés par identifiant de révision
  (`workflow_revisions.id`) ; le pipeline ne concatène jamais leurs graphes.

## Modèle de types

| Type | Rôle |
| --- | --- |
| `WorkflowPipeline` | Manifeste complet versionné (`format`/`version` + id, nom, description, tags, workflows, étapes) |
| `WorkflowRef` | Workflow source déclaré : `id` de révision, nom facultatif, `sourceSha256` facultatif épinglant la révision exacte |
| `Step` | Une passe : `id` stable, `workflowId`, `kind`, ports typés, liaisons, `seed`, `parameters`, `status`, `state`, `resume` |
| `StepId` | Identifiant d'étape stable choisi par l'auteur (`[A-Za-z0-9][A-Za-z0-9_-]*`, ≤ 120), jamais généré à la volée |
| `PassKind` | `image` ou `latent` (extensible ; une nouvelle variante est une évolution de format refusée par ce lecteur v1) |
| `PassStatus` | `pending` (défaut), `prepared`, `submitted`, `completed`, `failed`, `skipped` — écrits uniquement par l'orchestration future |
| `PortSpec` | Port typé : `name` + `portType` (IMAGE, LATENT, MODEL, CLIP, VAE, CONDITIONING… tels que déclarés par le moteur) |
| `PortType` | Chaîne validée (alphanumérique/`_` ou joker `*`) ; `accepts()` = égalité exacte ou joker, sémantique identique à `workflow::validate_graph` |
| `InputBinding` | Sortie d'étape amont → entrée d'étape cible : `port` ciblé + `nodeId` + `input` du graphe API cible + `source {step, port}` |
| `IntermediateState` | État intermédiaire par passe : `path` relatif + `portType`, cohérent avec le `kind` de la passe |
| `ResumeConditions` | Conditions de reprise explicites : `requiresState`, `requiresUpstreamCompleted`, `fromStatuses`, `note` — aucun comportement d'exécution |
| `Seed` | Type existant `contracts::Seed` réutilisé : u64 en texte JSON décimal canonique exact, jamais en f64 |

API : `WorkflowPipeline::parse(&str)` (format + version + validation complète),
`validate()`, `to_json()`, `execution_order()` (Kahn déterministe, ordre déduit du DAG,
indépendant de l'ordre de la liste `steps`).

## Format manifeste v1

Champs `camelCase`, `deny_unknown_fields` partout (un champ inconnu refuse la lecture).
L'exemple ci-dessous est extrait et relu par le parseur réel à chaque `cargo test`
(test `handoff_manifest_example_stays_parseable`) : il ne peut pas dériver du format.

```json
{
  "format": "saya-workflow-pipeline",
  "version": 1,
  "id": "6f1e2d3c-4b5a-4678-9abc-def012345678",
  "name": "Trois passes Saya",
  "description": "Base, raffinement latent, finalisation image",
  "tags": [
    "couple",
    "three-pass"
  ],
  "workflows": [
    {
      "id": "wf-base",
      "name": "Base SDXL",
      "sourceSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    },
    {
      "id": "wf-refine",
      "name": "Raffinement latent"
    },
    {
      "id": "wf-polish",
      "name": "Finalisation image"
    }
  ],
  "steps": [
    {
      "id": "base",
      "workflowId": "wf-base",
      "kind": "image",
      "name": "Passe de base",
      "inputs": [],
      "outputs": [
        {"name": "latent", "portType": "LATENT"},
        {"name": "image", "portType": "IMAGE"}
      ],
      "bindings": [],
      "seed": "18446744073709551615",
      "parameters": {
        "P1": "saya, solo",
        "NEG": "blurry"
      },
      "status": "pending",
      "state": {"path": "passes/base/boundary.png", "portType": "IMAGE"}
    },
    {
      "id": "refine",
      "workflowId": "wf-refine",
      "kind": "latent",
      "inputs": [
        {"name": "latent", "portType": "LATENT"}
      ],
      "outputs": [
        {"name": "latent", "portType": "LATENT"}
      ],
      "bindings": [
        {
          "port": "latent",
          "nodeId": "12",
          "input": "samples",
          "source": {"step": "base", "port": "latent"}
        }
      ],
      "seed": "42",
      "status": "failed",
      "state": {"path": "passes/refine/samples.safetensors", "portType": "LATENT"},
      "resume": {
        "requiresState": true,
        "requiresUpstreamCompleted": true,
        "fromStatuses": ["failed", "submitted"],
        "note": "Relancer après vérification du latent"
      }
    },
    {
      "id": "polish",
      "workflowId": "wf-polish",
      "kind": "image",
      "name": "Finalisation",
      "inputs": [
        {"name": "latent", "portType": "LATENT"}
      ],
      "outputs": [
        {"name": "image", "portType": "IMAGE"}
      ],
      "bindings": [
        {
          "port": "latent",
          "nodeId": "88",
          "input": "latent_image",
          "source": {"step": "refine", "port": "latent"}
        }
      ],
      "status": "pending"
    }
  ]
}
```

`nodeId`/`input` sont les coordonnées exactes dans le graphe API du workflow de l'étape
cible : c'est Astra qui appliquera effectivement la donnée amont à cette entrée lors de la
préparation réelle, sans jamais modifier le document source.

## Règles de validation (`validate()`, toutes refusées par un diagnostic clair en français)

1. `format == "saya-workflow-pipeline"` et `version == 1`, sinon refus explicite.
2. Bornes : 1..=64 étapes, ≤ 64 workflows déclarés, ≤ 64 ports par liste, ≤ 64 paramètres
   par étape, noms ≤ 300, identifiants ≤ 120, description ≤ 4000, ≤ 32 tags, note ≤ 2000.
3. Unicité des identifiants d'étapes et des identifiants de workflows déclarés.
4. Chaque `workflowId` d'étape est présent dans la liste `workflows` (la présence réelle en
   base reste un contrôle d'intégration).
5. `sourceSha256`, s'il est présent : hexadécimal minuscule de 64 caractères.
6. Chaque port d'entrée déclaré est alimenté par exactement une liaison ; chaque liaison
   vise un port d'entrée déclaré (ni port non alimenté, ni double liaison, ni port non déclaré).
7. Toute référence d'entrée pointe vers un port de sortie existant d'une étape existante du
   pipeline ; l'ordre de la liste `steps` est libre, le DAG décide.
8. Types de ports compatibles : LATENT→LATENT, IMAGE→IMAGE, égalité exacte ou joker `*`
   (sémantique du moteur) ; tout autre croisement est refusé nommément (ex. « IMAGE attend,
   reçu LATENT »).
9. Détection de cycle par Kahn (déterministe, collections ordonnées) avec diagnostic listant
   les étapes non ordonnables ; l'auto-référence d'une étape est un cycle.
10. `state.path` : relatif, sans composant `..` ni `:` ni caractère de contrôle (chemin
    absolu ou sortant du dossier refusé) ; `state.portType` doit correspondre au `kind`
    (passe `image` → IMAGE, passe `latent` → LATENT).
11. Seeds : validées par `contracts::Seed` à la désérialisation (u64 décimal canonique ;
    `007`, `-1`, dépassement u64 refusés).

## Tests exécutés (réels, 2026-10-16)

Environnement : crate verrouillé.

- `cargo clippy -p studio-core --all-targets --locked -- -D warnings` : **0 avertissement**.
- `cargo fmt --all --check` : **conforme**.
- `cargo test -p studio-core --locked` : **90 tests PASS, 0 échec, 2 ignorés** (les 2
  opt-in préexistants). Dont **11 nouveaux** dans `tests/workflow_pipeline.rs` :
  1. `linear_three_pass_pipeline_validates_and_orders_steps_from_the_dag`
  2. `branched_pipeline_orders_diamond_deterministically`
  3. `manifest_round_trips_exactly_and_preserves_the_u64_seed_text`
  4. `pipeline_built_in_rust_matches_the_json_format`
  5. `incompatible_port_types_are_rejected_explicitly`
  6. `unknown_step_port_and_workflow_references_are_rejected`
  7. `cycles_are_rejected_with_clear_diagnostics`
  8. `duplicate_step_ids_are_rejected`
  9. `unbound_duplicated_and_undeclared_input_ports_are_rejected`
  10. `wrong_format_version_and_invalid_seeds_are_rejected`
  11. `resume_state_and_intermediate_paths_round_trip_and_fail_safely`
  12. `handoff_manifest_example_stays_parseable` (relit l'exemple JSON de ce document avec
      le parseur réel)
- `node --test scripts/*.test.mjs` : **7 tests PASS, 0 échec** (contrats inchangés).

## Ce qu'Astra construit au-dessus

1. **Persistance** : stocker les manifestes (table dédiée ou extension de
   `generation_jobs`) et faire évoluer `status`/`state` par passe, dans l'esprit des tables
   `pass_attempts`/`pass_artifacts` existantes.
2. **Résolution des workflows** : vérifier que chaque `WorkflowRef.id` existe en base et,
   si `sourceSha256` est épinglé, que le document courant concorde (refuser sinon).
3. **Résolution des liaisons** : à la préparation de la passe N, injecter la donnée amont
   dans l'entrée `bindings[].input` du node `bindings[].nodeId` — sans modifier les
   originaux, comme le font déjà `prepare_job` et le préparateur Couple.
4. **Orchestration** : dérouler `execution_order()`, soumettre chaque passe, écrire les
   statuts, gérer l'incertitude de soumission avec les mécanismes jobs existants.
5. **Reprise réelle** : consommer `ResumeConditions` avec les vérifications réelles
   (présence/intégrité de l'état, compatibilité amont) ; pour la frontière IMAGE,
   réutiliser `checkpoints.rs` (`saya-image-checkpoint` v1, capture/vérification/revue) ;
   pour LATENT, s'appuyer sur la qualification CPU SaveLatent/LoadLatent de
   `docs/REPRISE.md`. Aucun PNG ne doit être présenté comme latent restaurable.
6. **UI/IPC** : commandes Tauri exposant parse/validate/execution_order et l'état des passes.

## Limites explicites (à ne pas survendre)

- Aucune exécution, aucune preuve GPU : ce livrable est structurel, pas fonctionnel.
- Aucune inspection des documents de workflow au niveau pipeline : la cohérence
  port ↔ entrée réelle du graphe est validée lors de la préparation par le moteur
  (`validate_graph`), pas ici.
- Aucun format v2, aucune migration : un champ inconnu ou une version supérieure est
  refusé, pas deviné.
- `PassKind` limité à `image`/`latent` : l'extension est une évolution de format assumée.
