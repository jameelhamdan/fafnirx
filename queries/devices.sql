--! device_by_token : (instance_id?, hostname?, ip?)
SELECT d.id, d.folder, d.instance_id, d.hostname, d.ip
FROM devices d JOIN users u ON u.id = d.user_id
WHERE d.token_hash = :token_hash AND d.revoked_at IS NULL AND d.kind = 'device' AND u.disabled_at IS NULL;

--! take_over_device
UPDATE devices SET instance_id = :instance, last_seen = :now
WHERE id = :id AND (instance_id IS NULL OR instance_id = :instance OR last_seen IS NULL OR last_seen < :lease_start);

--! record_heartbeat (ip?)
UPDATE devices SET hostname = :hostname, os = :os, ip = :ip, client_version = :client_version,
                   disk_total = :disk_total, disk_free = :disk_free, last_seen = :now
WHERE id = :id;

--! clear_activity
DELETE FROM device_activity WHERE device_id = :device_id;

--! save_activity
INSERT INTO device_activity (device_id, updated_at, phase, current_file, files_done, files_total, bytes_done, bytes_total, speed_bps)
VALUES (:device_id, :now, :phase, :current_file, :files_done, :files_total, :bytes_done, :bytes_total, :speed_bps)
ON CONFLICT (device_id) DO UPDATE SET updated_at = EXCLUDED.updated_at, phase = EXCLUDED.phase,
  current_file = EXCLUDED.current_file, files_done = EXCLUDED.files_done, files_total = EXCLUDED.files_total,
  bytes_done = EXCLUDED.bytes_done, bytes_total = EXCLUDED.bytes_total, speed_bps = EXCLUDED.speed_bps;

--! device_summaries (folder?) : (hostname?, os?, ip?, client_version?, disk_total?, disk_free?, last_seen?, last_collected?, revoked_at?, activity_at?, phase?, current_file?, files_done?, files_total?, bytes_done?, bytes_total?, speed_bps?)
SELECT d.name, d.kind, d.created_at, d.revoked_at, d.hostname, d.os, d.ip, d.client_version, d.disk_total,
       d.disk_free, d.last_seen, d.last_collected, d.total_bytes, d.file_count,
       a.updated_at AS activity_at, a.phase, a.current_file, a.files_done, a.files_total, a.bytes_done, a.bytes_total, a.speed_bps,
       u.name AS user_name, d.folder
FROM devices d
JOIN users u ON u.id = d.user_id
LEFT JOIN device_activity a ON a.device_id = d.id
WHERE (:folder::text IS NULL OR d.folder = :folder) AND (:all::boolean OR d.kind = 'master' OR d.user_id = :user::bigint)
ORDER BY u.name, d.name;

--! user_for_device : (id, role)
SELECT id, role FROM users WHERE name = :name AND disabled_at IS NULL;

--! insert_device
INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES (:name, :kind, :token_hash, :now, :user_id)
ON CONFLICT (user_id, name) DO NOTHING RETURNING folder;

--! set_device_token
UPDATE devices SET token_hash = :token_hash, revoked_at = NULL, instance_id = NULL WHERE folder = :folder AND kind = 'device';

--! release_device
UPDATE devices SET instance_id = NULL WHERE folder = :folder AND kind = 'device';

--! revoke_device
UPDATE devices SET revoked_at = COALESCE(revoked_at, :now) WHERE folder = :folder AND kind = 'device';

--! device_or_local_exists
SELECT 1 FROM devices WHERE folder = :folder AND kind IN ('device', 'local');

--! lock_device_folder : (folder)
SELECT folder FROM devices WHERE id = :id FOR UPDATE;

--! update_totals
UPDATE devices d SET file_count = t.live, total_bytes = t.bytes, deleted_files = t.gone, totals_stale = FALSE, totals_at = :now
FROM (SELECT COUNT(*) FILTER (WHERE f.deleted_at IS NULL) AS live,
             COALESCE(SUM(v.size) FILTER (WHERE f.deleted_at IS NULL), 0)::BIGINT AS bytes,
             COUNT(*) FILTER (WHERE f.deleted_at IS NOT NULL AND f.deleted_by IS DISTINCT FROM 'rename') AS gone
      FROM files f LEFT JOIN versions v ON v.id = f.current_version_id
      WHERE f.device_id = :id) t
WHERE d.id = :id;

--! stale_device_ids
SELECT d.id FROM devices d
WHERE d.totals_stale AND NOT EXISTS (SELECT 1 FROM device_activity a WHERE a.device_id = d.id AND a.updated_at > :syncing_since);

--! clients_last_seen
SELECT folder, last_seen FROM devices WHERE kind = 'device' AND revoked_at IS NULL AND last_seen IS NOT NULL;

--! visible_totals
SELECT COALESCE(SUM(file_count), 0)::BIGINT AS files, COALESCE(SUM(total_bytes), 0)::BIGINT AS bytes,
       COALESCE(SUM(deleted_files), 0)::BIGINT AS deleted
FROM devices d WHERE (:all::boolean OR d.kind = 'master' OR d.user_id = :user::bigint);

--! mark_totals_stale
UPDATE devices SET totals_stale = TRUE WHERE id = ANY(:ids);

--! master_totals
SELECT file_count, total_bytes FROM devices WHERE kind = 'master';

--! device_kind
SELECT kind FROM devices WHERE id = :id;

--! device_by_folder
SELECT id, folder FROM devices WHERE folder = :folder;

--! device_kind_by_folder
SELECT kind FROM devices WHERE folder = :folder;

--! device_id_and_kind_by_folder
SELECT id, kind FROM devices WHERE folder = :folder;

--! ensure_local_device
INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES (:name, 'local', '!', :now, :user_id)
ON CONFLICT (user_id, name) DO UPDATE SET name = devices.name RETURNING id;
