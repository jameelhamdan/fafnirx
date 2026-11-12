-- Devices: the devices and phones that send files. Their files are in folders (`folders.sql`).

--! device_by_token : (instance_id?, hostname?, ip?)
SELECT d.id, d.key, d.user_id, d.instance_id, d.hostname, d.ip
FROM devices d JOIN users u ON u.id = d.user_id
WHERE d.token_hash = :token_hash AND d.revoked_at IS NULL AND u.disabled_at IS NULL;

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

--! device_summaries (key?) : (hostname?, os?, ip?, client_version?, disk_total?, disk_free?, last_seen?, last_collected?, revoked_at?, activity_at?, phase?, current_file?, files_done?, files_total?, bytes_done?, bytes_total?, speed_bps?)
-- The devices a user can see (all for an administrator), or the one with this key, with what their folders hold together.
SELECT d.name, d.created_at, d.revoked_at, d.hostname, d.os, d.ip, d.client_version, d.disk_total,
       d.disk_free, d.last_seen, t.last_collected, t.total_bytes, t.file_count,
       a.updated_at AS activity_at, a.phase, a.current_file, a.files_done, a.files_total, a.bytes_done, a.bytes_total, a.speed_bps,
       u.name AS user_name, d.key
FROM devices d
JOIN users u ON u.id = d.user_id
LEFT JOIN device_activity a ON a.device_id = d.id
CROSS JOIN LATERAL (SELECT MAX(f.last_collected) AS last_collected, COALESCE(SUM(f.total_bytes), 0)::BIGINT AS total_bytes,
                           COALESCE(SUM(f.file_count), 0)::BIGINT AS file_count
                    FROM folders f WHERE f.device_id = d.id) t
WHERE (:key::text IS NULL OR d.key = :key) AND (:all::boolean OR d.user_id = :user::bigint)
ORDER BY d.key;

--! user_for_device : (id, role)
SELECT id, role FROM users WHERE name = :name AND disabled_at IS NULL;

--! insert_device
INSERT INTO devices (name, token_hash, created_at, user_id) VALUES (:name, :token_hash, :now, :user_id)
ON CONFLICT (user_id, name) DO NOTHING RETURNING key;

--! set_device_token
UPDATE devices SET token_hash = :token_hash, revoked_at = NULL, instance_id = NULL WHERE key = :key;

--! release_device
UPDATE devices SET instance_id = NULL WHERE key = :key;

--! revoke_device
UPDATE devices SET revoked_at = COALESCE(revoked_at, :now) WHERE key = :key;

--! device_exists
SELECT 1 FROM devices WHERE key = :key;

--! device_id_by_key
SELECT id FROM devices WHERE key = :key;

--! devices_last_seen
SELECT key, last_seen FROM devices WHERE revoked_at IS NULL AND last_seen IS NOT NULL;

--! delete_device
-- Its folder state and activity go with it. Delete its folders first.
DELETE FROM devices WHERE id = :id;

--! folder_states_of_device
SELECT f.key, s.files, s.bytes, s.last_cycle, s.last_error, s.held, s.failing, s.mode, s.updated_at
FROM device_folder_state s JOIN folders f ON f.id = s.folder_id WHERE s.device_id = :device_id ORDER BY f.key;

--! clear_folder_state
DELETE FROM device_folder_state WHERE device_id = :device_id;

--! save_folder_state
INSERT INTO device_folder_state (device_id, folder_id, files, bytes, last_cycle, last_error, held, failing, mode, updated_at)
VALUES (:device_id, :folder_id, :files, :bytes, :last_cycle, :last_error, :held, :failing, :mode, :now)
ON CONFLICT (device_id, folder_id) DO UPDATE SET files = EXCLUDED.files, bytes = EXCLUDED.bytes, last_cycle = EXCLUDED.last_cycle,
  last_error = EXCLUDED.last_error, held = EXCLUDED.held, failing = EXCLUDED.failing, mode = EXCLUDED.mode, updated_at = EXCLUDED.updated_at;

--! device_states_of_folder
-- The devices that have a folder attached: all of them for an administrator, otherwise those of `user`.
SELECT d.key AS device, s.files, s.bytes, s.last_cycle, s.last_error, s.held, s.failing, s.mode, s.updated_at
FROM device_folder_state s JOIN devices d ON d.id = s.device_id
WHERE s.folder_id = :folder_id AND (:all::boolean OR d.user_id = :user::bigint)
ORDER BY d.key;
