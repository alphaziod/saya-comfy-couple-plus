# Mesures RAM et VRAM entre passes

Dans Pipelines, ouvrir « RAM et VRAM ». « Mesurer la mémoire » effectue uniquement des lectures. La RAM Saya Studio et celle du processus moteur sont des RSS Linux. Les compteurs amdgpu et la VRAM globale incluent les autres applications. Les réservations Torch sont rapportées par ComfyUI ; une API CUDA dans le code PyTorch peut également être utilisée par ROCm. CPU et MPS sont exclus des métriques VRAM.

Après une passe terminée, l'utilisateur peut confirmer « Décharger entre les passes ». Le noyau vérifie les fichiers persistés, refuse une passe soumise ou une file moteur occupée, puis envoie `/free` avec `unload_models` et `free_memory`. La commande est asynchrone côté ComfyUI. Les mesures avant/après sont conservées dans l'historique ; une baisse des réservations Torch sur les mêmes périphériques est nécessaire pour annoncer une libération. Un succès HTTP, une baisse de VRAM globale ou une mesure absente ne suffisent pas.

## Protocole ROCm nécessitant une autorisation de calcul

1. Utiliser une instance ComfyUI sans autre client soumettant des jobs. Choisir un pipeline IMAGE compatible et un budget de calcul explicitement autorisé.
2. Mesurer au repos, puis exécuter uniquement la première passe confirmée. Vérifier son SHA-256, sa taille et son état enregistré.
3. Mesurer de nouveau. Confirmer le déchargement. Comparer réservations Torch, VRAM globale et RSS séparément dans l'historique.
4. Fermer et rouvrir Studio. Vérifier la sortie persistée, puis confirmer la seconde passe seulement si son calcul a aussi été autorisé.
5. Conserver les mesures et les logs locaux. Si aucune baisse n'est mesurée dans la fenêtre de trois secondes, le résultat reste « non mesuré », même si le moteur répond 200. Une mesure ultérieure ne doit pas être présentée comme une causalité prouvée.

La vérification de file n'est pas atomique avec les soumissions d'autres clients. Les opérations Studio sont sérialisées, mais une instance dédiée reste nécessaire pour une mesure contrôlée. La reprise est celle d'un pipeline IMAGE, pas celle de l'état interne d'un sampler. Aucun transfert LATENT n'est implémenté.

Validation effectuée le 2026-10-17 : tests CPU et serveur simulé, smoke natif Niri avec redémarrage. Aucun chargement GPU ni déchargement réel n'a été déclenché pour cette validation.
