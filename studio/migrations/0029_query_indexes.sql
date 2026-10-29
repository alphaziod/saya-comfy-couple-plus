-- Query indexes measured with EXPLAIN QUERY PLAN on a large synthetic database.
-- job_outputs(asset_id) is already covered by job_outputs_asset (0006).
CREATE INDEX IF NOT EXISTS job_events_job ON job_events(job_id,id);
CREATE INDEX IF NOT EXISTS generation_jobs_created ON generation_jobs(created_at);
CREATE INDEX IF NOT EXISTS generation_jobs_comfy_prompt ON generation_jobs(comfy_prompt_id);
CREATE INDEX IF NOT EXISTS pipeline_run_steps_job ON pipeline_run_steps(job_id);
CREATE INDEX IF NOT EXISTS assets_capture ON assets(capture_role,capture_phase);
INSERT INTO schema_migrations(version,name) VALUES(29,'query-indexes');
