-- v28 : l'app possède les images (G1, brief points 4 et 6).
-- 1) Colonnes de capture, toutes nullable : les assets existants (imports,
--    bibliothèques déjà indexées, anciennes livraisons dans results/) ne sont
--    PAS déplacés ni retouchés ; la base sait déjà où ils sont. Seules les
--    NOUVELLES captures organisées par l'app portent ces champs.
--    capture_role : 'pull' = essai (<jour>/pull/<famille>/), 'final' = image
--    d'un workflow complet terminé (<jour>/final/). capture_phase : numéro de
--    passe connu à la livraison (runs multipasses) ; NULL sinon.
ALTER TABLE assets ADD COLUMN capture_role TEXT CHECK(capture_role IS NULL OR capture_role IN ('pull','final'));
ALTER TABLE assets ADD COLUMN capture_phase INTEGER CHECK(capture_phase IS NULL OR capture_phase>=1);
ALTER TABLE assets ADD COLUMN capture_day TEXT;
ALTER TABLE assets ADD COLUMN capture_famille TEXT;
-- 2) Navigation précédente/suivante (G1, brief point 6) : la clause galerie
--    dédoublonnait par chemin via une sous-requête corrélée qui relisait chaque
--    ligne complète (metadata_json inline : Gio au total). Cet index couvrant
--    répond aux requêtes galerie/voisins sans toucher la table. Mesuré sur une
--    copie de la base réelle (6107 assets, 3,7 Gio) : gallery_neighbors
--    787 ms → 12 ms, +2,2 Mio d'index, ~4 ms par écriture d'annotation.
CREATE INDEX assets_gallery_cover ON assets(availability,catalog_removed,absolute_path,modified_at,id,relative_path,width,height,metadata_state,favorite,note,review,content_sha256,byte_size);
INSERT INTO schema_migrations(version,name) VALUES(28,'gallery-captures');
