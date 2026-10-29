-- Casting 1-à-1 (tranche U1, 2026-10-25) : persistance des OUI.
-- Un OUI = Saya garde l'OC testé. Ligne unique par (run, stem) : la revalidation
-- du même shot met la ligne à jour. graine = graine du tirage moteur ; seed = seed
-- du shot (master seed de la passe 1). prefixe/catalogue = traçabilité passe 1.
CREATE TABLE IF NOT EXISTS casting_choices (
  id TEXT PRIMARY KEY,
  run TEXT NOT NULL CHECK(length(run) >= 8),
  stem TEXT NOT NULL,
  name TEXT NOT NULL,
  famille TEXT NOT NULL,
  model TEXT NOT NULL CHECK(model IN ('noobai','illustrious')),
  graine TEXT NOT NULL CHECK(length(graine) > 0),
  seed TEXT NOT NULL CHECK(length(seed) > 0),
  p1 TEXT NOT NULL CHECK(length(p1) > 0),
  anatomy TEXT NOT NULL,
  pose TEXT NOT NULL,
  bg TEXT NOT NULL,
  prefixe TEXT NOT NULL,
  catalogue TEXT NOT NULL,
  decided_at TEXT NOT NULL,
  UNIQUE(run, stem)
) STRICT;
INSERT OR IGNORE INTO schema_migrations(version,name) VALUES(27,'casting-choices');
