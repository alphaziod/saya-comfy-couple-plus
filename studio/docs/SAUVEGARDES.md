# Sauvegarder et restaurer les données

Dans **Données**, choisir un **nouveau dossier** dont le parent existe, puis **Créer la sauvegarde vérifiée**. Le nom proposé se trouve à côté de la base active. Conserver une copie sur un support local indépendant si nécessaire. La sauvegarde contient des données privées : ne pas la publier.

La sauvegarde v3 contient SQLite par défaut. L’option **Inclure les états de passes** ajoute explicitement les checkpoints IMAGE et le stockage versionné des états de passes. SQLite est copié par son API de sauvegarde, y compris les transactions validées encore présentes dans le WAL. La copie est contrôlée par `integrity_check`, clés étrangères, identité Saya Studio et version de schéma. Le manifeste enregistre taille et SHA-256. Les dossiers/fichiers créés ont des permissions 0700/0600 sur Unix. Aucun dossier existant n'est réutilisé ou écrasé.

## Contenu

Inclus : personnages, variantes, références et journal, index et associations, annotations, workflows importés, snapshots/jobs et provenance, propositions et décisions, préférences. L’option des états inclut les checkpoints IMAGE gérés (inventaire du snapshot SQLite) et les fichiers enregistrés dans le manifeste `pass-states`, avec leurs types, empreintes et rétentions. Les états opaques LATENT peuvent être sauvegardés sans être interprétés ni exécutés. Les fichiers orphelins ne sont pas copiés. Une sortie de pipeline déclarée terminée mais absente empêche la sauvegarde complète.

Les anciennes sauvegardes v1 (SQLite seul) et v2 (SQLite + checkpoints gérés) restent vérifiables et restaurables. Aucun format existant n’est réécrit. Les anciennes versions de Studio ne lisent pas le format v3 : conserver une sauvegarde compatible avant mise à jour pour revenir à une ancienne application.

Chaque fichier géré est contrôlé avant et après copie (hash/taille), stocké sous `checkpoints/` en mode 0600. Un fichier requis manquant, corrompu ou symbolique empêche la publication du manifeste final. Les fichiers temporaires et orphelins sont exclus. Limites : 10000 checkpoints gérés distincts et 128 Mio par PNG ; 10000 fichiers du stockage de passes et manifeste final de 4 Mio maximum. Dépassement refusé, jamais tronqué. Les copies sont faites en flux, sans conserver toutes les images décodées en mémoire.

Exclus : images originales, modèles, installation ComfyUI, fichiers de génération/latents non gérés et cache de miniatures. Les chemins des originaux restent inchangés ; déplacer la sauvegarde sur une autre machine ne déplace pas ces fichiers. La restauration de cette base ne reprend pas un sampler et ne soumet aucun job.

## Choisir le contenu

L’option des états est décochée au démarrage de l’écran. Cliquer **Estimer la taille** affiche les octets estimés et la liste relative des fichiers inclus. Changer l’option invalide l’estimation précédente. Le manifeste final enregistre le contenu réellement copié ; l’estimation n’est pas une garantie de taille exacte en présence d’écritures concurrentes.

Sans l’option, l’historique et les références de passes restent dans SQLite, mais leurs fichiers ne sont pas transportés. La sauvegarde n’est jamais présentée comme autonome : images originales, modèles et autres références externes restent nécessaires. Une restauration ne démarre aucun calcul.

## Vérifier et restaurer

1. Indiquer le dossier contenant `manifest.json` et `studio.sqlite3`.
2. Cliquer **Vérifier la sauvegarde** pour contrôler empreinte, schéma et intégrité.
3. Choisir un nouveau dossier de restauration dont le parent existe.
4. Cliquer **Restaurer dans le nouveau dossier**. La base active reste inchangée.

Un dossier sans manifeste valide est incomplet après erreur/interruption ; il n'est pas restaurable. Un manifeste inconnu, une empreinte incorrecte, une base étrangère/future ou un WAL ajouté à une sauvegarde sont refusés. Une erreur en cours de copie peut laisser un dossier partiel sans manifeste final ; le conserver pour diagnostic ou choisir un autre nom. Aucun nettoyage automatique.

Pour ouvrir une copie restaurée séparément sous Linux, créer un dossier parent, restaurer dans son sous-dossier **studio.saya.desktop**, puis lancer le binaire avec ce parent comme `XDG_DATA_HOME`. Exemple à adapter :

```sh
mkdir -p /tmp/saya-restauration
./target/debug/studio-maintenance restore /chemin/sauvegarde /tmp/saya-restauration/studio.saya.desktop
XDG_DATA_HOME=/tmp/saya-restauration ./target/debug/saya-studio
```

Ce lancement ne remplace pas la base habituelle. Le répertoire /tmp de cet exemple est temporaire : choisir un emplacement durable pour une restauration à conserver. Ne pas déplacer/remplacer manuellement une base active avec ses fichiers WAL/SHM.

## Commandes locales

```sh
cargo build -p studio-core --locked --bin studio-maintenance
./target/debug/studio-maintenance backup /chemin/studio.sqlite3 /chemin/nouvelle-sauvegarde
./target/debug/studio-maintenance backup /chemin/studio.sqlite3 /chemin/sauvegarde-avec-etats --include-pass-states
./target/debug/studio-maintenance verify /chemin/nouvelle-sauvegarde
./target/debug/studio-maintenance restore /chemin/nouvelle-sauvegarde /chemin/nouveau-dossier
```

Backup/verify lisent sans migration. Restore copie dans un nouveau dossier, revérifie les octets copiés et conserve les identifiants. Une version antérieure reconnue sera migrée seulement lors de son ouverture ultérieure par l'application, avec la sauvegarde automatique habituelle. Aucun mécanisme de restauration destructive de la base active n'est fourni.

Validation actuelle : Linux local uniquement. Permissions Windows/macOS et copie sur un système de fichiers distant non qualifiées.
