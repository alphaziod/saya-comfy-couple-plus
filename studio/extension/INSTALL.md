# Installation rapide

Le bridge Civitai est lancé automatiquement par Saya Studio (paquet installé).
Pour le lancer à la main depuis un clone du dépôt :

```bash
cd /chemin/vers/le/dépôt   # racine du clone de Saya Studio
python3 scripts/civitai_bridge.py   # installé : /usr/share/saya-studio/civitai_bridge.py
```

Puis charger `extension/` dans Brave via `brave://extensions` (mode développeur).
Sur Civitai, le bouton principal devient **Télécharger avec Saya Studio**.
Le dossier de téléchargement par défaut est le dossier Téléchargements XDG
(`$XDG_DOWNLOAD_DIR`, sinon `~/Downloads`) ; option `--download-root` ou variable
`SAYA_CIVITAI_DOWNLOAD_ROOT` pour le changer.
