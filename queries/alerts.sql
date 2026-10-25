--! upsert_alert
INSERT INTO alerts (kind, key, message, created_at, updated_at) VALUES (:kind, :key, :message, :now, :now)
ON CONFLICT (kind, key) WHERE resolved_at IS NULL DO UPDATE SET message = excluded.message, updated_at = excluded.updated_at;

--! resolve_alert
UPDATE alerts SET resolved_at = :now WHERE kind = :kind AND key = :key AND resolved_at IS NULL;
