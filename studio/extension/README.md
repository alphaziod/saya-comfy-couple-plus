# Saya Studio — Civitai Bridge

## Charger l’extension
1. Brave/Chromium → `brave://extensions` / `chrome://extensions`.
2. Activer le mode développeur.
3. « Charger l’extension non empaquetée » → ce dossier `extension/`.

## Lancer le bridge
```bash
cd /chemin/vers/le/dépôt   # racine du clone de Saya Studio
python3 scripts/civitai_bridge.py
```
Le bridge écoute uniquement `127.0.0.1:47821`. La clé Civitai est lue dans
`~/.config/civitai/token` et n’est jamais exposée à la page ni à l’extension.

## Utilisation
Sur une page modèle Civitai, le bouton original est masqué et remplacé par :
- **Télécharger avec Saya Studio** ;
- **Téléchargement normal** (petit bouton secondaire).

Les fichiers arrivent par défaut dans
le dossier Téléchargements XDG (`$XDG_DOWNLOAD_DIR`, sinon `~/Downloads`), sans
écrasement. Le dossier se change avec `--download-root` ou
`SAYA_CIVITAI_DOWNLOAD_ROOT` ; la destination des options de l’extension doit rester
dans une racine autorisée du bridge.

## Limites actuelles
- Le bridge est un service Python séparé, pas encore intégré au binaire Tauri.
- Pas de reprise de téléchargement interrompu.
- La file est en mémoire ; un redémarrage du bridge perd l’historique.
- Le catalogue Saya doit être rescané sur ce dossier pour voir le fichier.

## Appairage sécurisé
L’extension n’accepte plus un simple identifiant déclaratif. Copier le jeton
`~/.local/share/studio.saya.desktop/civitai-token` dans les options de
l’extension. Sans ce jeton, `/health` et `/download` répondent 403.
Le jeton n’est jamais injecté dans la page Civitai.
