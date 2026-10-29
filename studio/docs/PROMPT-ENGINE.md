# Assistant local Saya Prompt Engine

L’onglet **Assistant local** appelle les fonctions Python `prompts.analyze`, `prompts.optimize`, `prompts.validate` et `workflow.analyze_graph` du projet Saya Prompt Engine installé. Ce n’est pas une connexion MCP réseau ni un modèle distant. Aucune route ComfyUI mutante ni service Grok n’est utilisé par cet adaptateur.

Prérequis : Python 3 (commande `python3`) et dossier de source de confiance contenant `src/saya_prompt_engine`. Saisir ce dossier dans l’interface. Le chemin est conservé après une analyse ou proposition. Le processus isolé Python `-I` n’instancie pas Settings, ne crée pas de state_dir du MCP et n’écrit pas de bytecode. L’adaptateur limite requêtes/réponses à 16 Mo et tue un appel dépassant 20 s. Il ne constitue pas un sandbox pour un moteur local malveillant : sélectionner uniquement son installation de confiance.

Une proposition ajoute seulement les triggers documentés compatibles retenus par les règles existantes. Son statut reste `proposed_unvalidated`. Le JSON montre les suggestions et la validation structurelle ; deux champs affichent l’avant et l’après exacts. L’utilisateur confirme séparément sa décision. L’acceptation remplace seulement le texte de cet éditeur ; aucune fiche ni entrée de workflow n’est modifiée automatiquement. Si le texte a changé depuis la proposition, l’acceptation est refusée. Le refus et l’acceptation deviennent des décisions immuables.

SQLite v5 conserve requête, réponse, chemin du moteur, empreinte SHA-256 des quatre modules lus et décision horodatée. L’interface affiche les 50 dernières propositions ; aucun enregistrement plus ancien n’est purgé. Les décisions Studio sont distinctes des validations humaines CLI du MCP, qui ne sont jamais fabriquées ici. Pas de validation de qualité d’image implicite.

L’inspecteur accepte un JSON UI/API ou un fichier jusqu’à 15 Mo. Il traverse les subgraphs via l’inspecteur existant et rapporte ses limites sur widgets sauvegardés, liens/promotions et conditioning non résolu. Il ne convertit pas un graphe UI en graphe exécutable.

## Vérification et retour arrière

`SAYA_PROMPT_ENGINE_ROOT=/chemin/saya-prompt-engine cargo test -p studio-core --locked --test engine -- --include-ignored` vérifie les sources installées avec des textes synthétiques, sans transfert. Le test portable normal ignore explicitement cette intégration si elle n’est pas demandée.

La migration 0005 ajoute une table ; elle ne transforme aucune fiche, image ou génération. `Database::open` sauvegarde la base v4 avec l’API backup SQLite et vérifie son intégrité avant migration. Le test de migration couvre conservation et réouverture sans nouvelle sauvegarde. Pour un retour à un ancien binaire, fermer Studio et restaurer une copie de cette sauvegarde dans un dossier applicatif séparé ; conserver la base v5 pour ne perdre aucune nouvelle proposition. L’ancien binaire refuse une base plus récente. Ne jamais remplacer une base ouverte ni revenir en arrière en modifiant seulement user_version.
