--! upsert_alert
INSERT INTO alerts (kind, key, message, created_at, updated_at) VALUES (:kind, :key, :message, :now, :now)
ON CONFLICT (kind, key) WHERE resolved_at IS NULL DO UPDATE SET message = excluded.message, updated_at = excluded.updated_at;

--! touch_alert
-- Marks the open alert as raised again, when its message is the same one.
UPDATE alerts SET updated_at = :now WHERE kind = :kind AND key = :key AND message = :message AND resolved_at IS NULL;

--! resolve_alert
UPDATE alerts SET resolved_at = :now WHERE kind = :kind AND key = :key AND resolved_at IS NULL;

--! resolve_stale_alerts
UPDATE alerts SET resolved_at = :now WHERE kind = :kind AND key = :key AND resolved_at IS NULL AND updated_at < :before;

--! expire_quiet_alerts
UPDATE alerts SET resolved_at = :now
WHERE kind = :kind AND resolved_at IS NULL AND COALESCE(updated_at, created_at) < :before;

--! open_alerts : (updated_at?)
SELECT id, kind, key, message, created_at, updated_at FROM alerts WHERE resolved_at IS NULL ORDER BY created_at DESC, id;

--! alert_key
SELECT key FROM alerts WHERE id = :id;

--! dismiss_alert
UPDATE alerts SET resolved_at = :now WHERE id = :id AND resolved_at IS NULL;

--! purge_resolved_alerts
DELETE FROM alerts WHERE resolved_at IS NOT NULL AND resolved_at < :before;
