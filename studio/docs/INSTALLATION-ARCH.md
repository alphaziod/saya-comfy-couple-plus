# Installation native EndeavourOS / Arch Linux

Le paquet pacman est le format principal. Il installe :

- `/usr/bin/saya-studio` et `/usr/bin/saya-studio-maintenance` (0755) ;
- le lanceur XDG, l'icône hicolor et les guides dans `/usr/share/doc/saya-studio` ;
- `/usr/share/saya-studio/` : pont Civitai (`civitai_bridge.py`), hôte Native Messaging (`saya_native_host.py`), sidecar `sidecars/f09-semantic-embed.py`, presets (`presets/`, `presets/passes/`, `presets/outils/`) et l'extension navigateur (`extension/`, à charger en mode développeur) ;
- les manifestes Native Messaging `com.saya.studio.json` dans `/etc/opt/chrome/native-messaging-hosts/` et `/etc/chromium/native-messaging-hosts/` (et une copie dans `/usr/share/saya-studio/`).

Un seul script pacman : `saya-studio.install`, qui n'affiche qu'un message après désinstallation. Aucun service, réglage Niri, téléchargement de modèle ni lancement ComfyUI. Le fichier `saya-studio.desktop` correspond à l’app_id Wayland réel `saya-studio` et est indépendant de fish (PATH du lanceur DMS/Niri).

Au démarrage, l'application écrit aussi des manifestes Native Messaging au niveau utilisateur pour Chrome, Chromium et Brave (voir Désinstallation) et lance le pont Civitai local sur `127.0.0.1:47821`. Le dossier de téléchargement par défaut du pont est le dossier Téléchargements XDG (`$XDG_DOWNLOAD_DIR`, sinon `~/Downloads`).

## Construire sans modifier le système

Depuis le dépôt, avec les dépendances Arch déjà installées :

```sh
bash scripts/build-arch.sh
```

Le script archive uniquement le commit Git choisi (HEAD par défaut), conserve les éditions non commitées, génère les sommes SHA-256, compile le frontend et les deux exécutables release, exécute les tests et produit un `.pkg.tar.zst` dans un nouveau dossier `test-results/arch-package-…`. `SOURCE-COMMIT`, PKGBUILD, `.SRCINFO`, `.PKGINFO` et `.BUILDINFO` permettent de retrouver les sources et l'environnement. Le build remappe les chemins de la machine de construction hors des binaires (`--remap-path-prefix`). Les caches Cargo/npm doivent être disponibles pour ce build hors ligne ; aucune installation automatique de dépendances. Le cache de compilation existant est réutilisé.

Prérequis de construction : base-devel, rust, nodejs, npm, desktop-file-utils et les dépendances du PKGBUILD. Dépendances d'exécution : bibliothèques GTK/WebKit/GLib/DBus, `python` (pont Civitai, hôte Native Messaging, sidecars), `curl` (téléchargements HTTPS de la bibliothèque de modèles), `xdg-utils` (`xdg-open`, ouvrir le dossier d'une image), `xdg-desktop-portal` (boîtes de dialogue de fichiers ; installer aussi un backend, par ex. `xdg-desktop-portal-gtk`). `gio` vient de glib2. Optionnels : `ffmpeg` (aperçus vidéo de LoRA), `python-pytorch` (embeddings sémantiques F09), ComfyUI (externe). Saya Prompt Engine et ComfyUI restent des installations externes. Le paquet ne contient ni bibliothèque privée, ni base utilisateur, ni modèle.

## Installer ou mettre à jour — décision humaine

Fermer Saya Studio et créer d'abord une sauvegarde dans le panneau Données (ou avec `saya-studio-maintenance backup`). Après vérification du paquet :

```sh
sudo pacman -U /chemin/vers/saya-studio-0.1.0-2-x86_64.pkg.tar.zst
```

Lancer ensuite **Saya Studio** depuis DMS/Niri, ou `saya-studio` dans fish. Aucune variable GDK_BACKEND imposée : GTK utilise la session Wayland disponible. Les tests forcés Wayland utilisent des données isolées.

Données Linux : `$XDG_DATA_HOME/studio.saya.desktop`, sinon `~/.local/share/studio.saya.desktop`. L'identifiant Tauri est conservé, donc une mise à jour retrouve la même base. Les migrations connues créent une sauvegarde préalable ; une base inconnue ou plus récente est refusée. Le panneau Données affiche le chemin exact et la restauration vise exclusivement un nouveau dossier. Voir SAUVEGARDES.md.

ComfyUI est contacté sur `127.0.0.1:8188` ; le paquet ne le démarre pas. Aucun job ne doit être soumis sans la confirmation affichée dans l'application. Le suivi de jobs déjà connus peut reprendre à l'ouverture.

## Vérifier un paquet

```sh
python3 scripts/verify-arch-package.py ./test-results/arch-package-…/saya-studio-….pkg.tar.zst
```

Sans rien installer, le script contrôle la liste exacte des fichiers (recoupée avec le `package()` du PKGBUILD), propriétaires et permissions (pas d'écriture publique, exécutables +x), l'absence de chemins de répertoire personnel ou du dépôt source dans les fichiers et les binaires, l'absence d'instrumentation de smoke-test, le lanceur, l'icône, les manifestes et les dépendances par rapport aux bibliothèques liées.

## Désinstallation

```sh
sudo pacman -R saya-studio
```

Pacman retire les fichiers du paquet mais ne touche jamais aux dossiers personnels ; un message rappelle ce qui reste. À supprimer à la main si souhaité :

- `~/.config/google-chrome/NativeMessagingHosts/com.saya.studio.json`, `~/.config/chromium/NativeMessagingHosts/com.saya.studio.json`, `~/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/com.saya.studio.json` (écrits par l'application à chaque démarrage, `$XDG_CONFIG_HOME` respecté) ; ils pointent vers un hôte qui n'existe plus ;
- l'extension, depuis `brave://extensions` / `chrome://extensions` ;
- les données `~/.local/share/studio.saya.desktop` (base, sauvegardes, jeton et jobs du pont Civitai) : à ne supprimer qu'après sauvegarde.

Images, workflows et modèles dans vos dossiers ne sont jamais supprimés. Aucun script de purge n'est fourni.

## Limites à qualifier

Paquet construit pour Arch Linux x86_64 avec les bibliothèques de l'hôte ; ce n'est pas un paquet Debian/RPM. Installation système et intégration effective au menu après installation demandent l'accord humain. Les preuves de lancement, réouverture et dépendances sont détaillées dans PROGRESS.md ; une simple compilation ne prouve pas ces parcours. AppImage est secondaire et dépend de la disponibilité d'outils de bundling compatibles. Windows/macOS restent non testés.

## Livrables qualifiés le 15 octobre 2026

Depuis la racine du dépôt, paquet du snapshot applicatif `d14f9bb` :

```fish
sudo pacman -U ./test-results/arch-package-d14f9bbb-YaIQfK/saya-studio-0.1.0-2-x86_64.pkg.tar.zst
```

SHA-256 : `1355efbe031b42396d511b21fb3dd7c7e34740b74498d7e0ee2c57ba5afef09f`. Bibliothèques directes vérifiées avec leurs propriétaires pacman ; ABI glibc minimale observée 2.39, hôte 2.44. Licence du projet non déclarée : paquet local marqué `unknown`, aucune licence inventée.

AppImage secondaire : `target/release/bundle/appimage/Saya Studio_0.1.0_amd64.AppImage`, 110152184 octets ; SHA-256 `6e2f062d4a3ad589b0f4e7e709ee1144da8205065e1423d79a3f96b7de91a328`. Outils portables Tauri téléchargés dans un cache de test, sans installation système. Création depuis le release existant :

```sh
XDG_CACHE_HOME="$PWD/test-results/appimage-cache" npm run tauri -- bundle --bundles appimage --ci --no-binary-patching
```

Les deux formats ont passé deux ouvertures/fermetures Wayland sous Niri, migration v5→v8 et conservation de 209 images/43 personnages sur copie. L’original SQLite/WAL est inchangé. Un build de test séparé confirme dans le WebView le chemin de la copie, ComfyUI 0.34.0 et la navigation persistée. Aucune capture des images privées. Les nouveaux développements postérieurs à `d14f9bb` ne sont pas inclus dans ces livrables ; reconstruire après validation du prochain checkpoint. Les autres distributions et le menu DMS après installation système restent non testés.

## Build de validation V14 — 17 octobre 2026

Paquet **de développement/validation** (non publié, non installé) construit depuis `fe6bdc8` par `bash scripts/build-arch.sh` :

```fish
# Décision humaine requise ; fermer Saya Studio et sauvegarder d'abord (panneau Données).
sudo pacman -U ./test-results/arch-package-fe6bdc8c-xbGutL/saya-studio-0.2.0.dev.gfe6bdc8c-1-x86_64.pkg.tar.zst
```

- SHA-256 : `b567f26058be18d535a8d007620ee91ff82a567bf9eac95e323978430a61a669` (7 544 765 octets).
- Version `0.2.0.dev.g<commit>` : dérivée du commit, jamais confondue avec une release ; `pkgdesc` porte la mention « development/validation build ».
- `check()` du PKGBUILD exécuté : 222 tests Rust, 56 Node, svelte-check sans erreur.
- Contenu : `/usr/bin/saya-studio`, `/usr/bin/saya-studio-maintenance` (0755), lanceur XDG validé par `desktop-file-validate`, icône hicolor, guides. Bibliothèques dynamiques toutes résolues et couvertes par les dépendances déclarées.
- Lancement réel du binaire release extrait du paquet, sans installation, avec données isolées : fenêtre Niri `saya-studio` (= `StartupWMClass`), base v9 migrée en v19 avec sauvegarde préalable `studio-v9-*.sqlite3`, arrêt propre.

Mise à jour non destructive : l'identifiant de données ne change pas ; à la première ouverture, toute base de version 1 à 18 est sauvegardée (contrôle d'intégrité) puis migrée en une transaction (v19) ; échec = base inchangée et sauvegarde conservée ; base plus récente ou inconnue refusée. Les migrations v1/v5/v8/v9 → v19 sont couvertes par `crates/studio-core/tests/migrations_v16.rs`.

Limites connues de ce build : exécution multipasses validée uniquement sur moteur simulé (aucune génération GPU réelle) ; transport LATENT entre passes refusé ; libération VRAM réelle non mesurée sur ROCm (protocole `docs/RAM-VRAM.md`) ; prompts multilignes impossibles dans `parameters` d'un manifeste v1 (refus explicite) ; menu DMS après installation système, autres distributions, Windows/macOS non testés.
