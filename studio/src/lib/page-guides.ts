export const pageGuides: Record<string, {title:string;next:string}> = {
  "casting": {title:"Tester, choisir, finaliser",next:"Les réglages viennent de Génération. Lance le casting : chaque essai part en passe 1 seule. Clique une image pour enregistrer le personnage (fiche réutilisable, sans lancer la finition) ou pour lancer le workflow complet (6 passes) sur ce résultat. Annuler / Pause / Reprendre pilotent le run ; la pause s'arrêtera à la fin de l'essai en cours — ComfyUI ne permet pas de suspendre un sampler. Tes options, ciblages, exclusions, filtres et scripts sont sauvegardés automatiquement."},
  "scenes": {
    "title": "Préparer personnages, action et décor",
    "next": "Créez une scène, choisissez un workflow configuré, puis vérifiez les textes avant de préparer la génération."
  },
  "assistant": {
    "title": "Agents & API",
    "next": "Cette page sert aux agents et aux scripts. Vous n’avez rien à y régler au quotidien ; les outils avancés restent repliés."
  },
  "workflows": {
    "title": "Configurer un workflow API",
    "next": "Importez une copie JSON API. Choisissez les champs modifiables et leur rôle, puis préparez la requête. Le lancement demande une confirmation."
  },
  "people": {
    "title": "Parcourir les images par personnage",
    "next": "Cliquez sur un portrait pour ouvrir les images associées. Les fiches se créent et se modifient dans Personnages."
  },
  "ablab": {
    "title": "Comparer plusieurs réglages",
    "next": "Créez une campagne, définissez les valeurs à comparer et préparez la grille. Chaque génération demande sa confirmation ; vous choisissez les résultats gagnants."
  },
  "modules": {
    "title": "Réutiliser un groupe de nodes",
    "next": "Extrayez un module depuis un workflow importé. Choisissez ensuite un workflow de destination, reliez les entrées et prévisualisez la copie."
  },
  "pipelines": {
    "title": "Assembler votre propre suite de passes",
    "next": "Choisissez les workflows et leurs liaisons, puis enregistrez le pipeline. Pour le preset Saya à six passes, utilisez Génération."
  },
  "graph": {
    "title": "Modifier les nodes et leurs connexions",
    "next": "Ouvrez un workflow, sélectionnez un node pour ses réglages, puis enregistrez une copie. La préparation API permet de vérifier la conversion."
  },
  "engine": {
    "title": "Voir si ComfyUI est prêt",
    "next": "Consultez la connexion, les calculs en cours et la mémoire. La liste des modèles disponibles se charge avec le bouton dédié."
  },
  "data": {
    "title": "Sauvegarder ou restaurer les données Studio",
    "next": "Choisissez un nouveau dossier et estimez la taille avant la copie. Les images originales et les modèles doivent être sauvegardés séparément."
  }
};
