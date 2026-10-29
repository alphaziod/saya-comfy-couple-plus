export const labels: Record<string,string> = {
  draft:'Brouillon', prepared:'Préparée', pending:'À lancer', waiting:'En attente',
  submitted:'Envoyée au moteur', submitting:'Envoi en cours', queued:'Dans la file', running:'En cours',
  succeeded:'Réussie', completed:'Terminée', failed:'En échec', cancelled:'Annulée', interrupted:'Interrompue',
  stopped:'Arrêtée', invalidated:'À recalculer', submission_unknown:'Envoi à vérifier', awaiting_review:'À valider',
  planned:'Prévue', blocked:'Bloquée', paused:'En pause'
};
export function stateLabel(state: string | null | undefined): string {
  return state ? labels[state] ?? state : 'État non disponible';
}
