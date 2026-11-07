--! device_by_token : (instance_id?, hostname?, ip?)
SELECT d.id, d.folder, d.user_id, d.instance_id, d.hostname, d.ip
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
       COALESCE(u.name, '') AS user_name, d.folder
FROM devices d
LEFT JOIN users u ON u.id = d.user_id
LEFT JOIN device_activity a ON a.device_id = d.id
WHERE (:folder::text IS NULL OR d.folder = :folder) AND visible(d, :user::bigint, :all::boolean)
ORDER BY d.folder;

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
UPDATE devices d SET file_count = t.live, total_bytes = t.bytes, deleted_files = t.gone, totals_stale = FALSE
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
FROM devices d WHERE visible(d, :user::bigint, :all::boolean);

--! mark_totals_stale
UPDATE devices SET totals_stale = TRUE WHERE id = ANY(:ids);

--! device_kind
SELECT kind FROM devices WHERE id = :id;

--! device_by_folder
SELECT id, folder FROM devices WHERE folder = :folder;

--! device_kind_by_folder
SELECT kind FROM devices WHERE folder = :folder;

--! device_id_and_kind_by_folder
SELECT id, kind FROM devices WHERE folder = :folder;

--! folders_for_user (key?) : (last_collected?)
-- Every folder a user can use, whoever owns it: their own, those whose owner added them, and the master folders (`visible`, the one
-- rule for what a user sees). `folder` is how
-- clients name it (`<owner>/<name>`, `master:<path>`). A read-only account never writes. `key` narrows it to one: the name of one's
-- own, its full name or its stable id.
SELECT d.id, d.public_id, d.name, d.folder, COALESCE(o.name, '') AS owner, COALESCE(d.user_id = :user_id, FALSE) AS mine,
       me.role <> 'readonly' AND (d.kind = 'master' OR d.user_id = :user_id OR COALESCE(m.can_write, FALSE)) AS can_write,
       d.kind = 'master' OR EXISTS (SELECT 1 FROM shared_members s WHERE s.folder_id = d.id) AS shared, d.kind = 'master' AS master,
       d.created_at, d.file_count AS files, d.total_bytes AS bytes, d.last_collected
FROM devices d
JOIN users me ON me.id = :user_id AND me.disabled_at IS NULL
LEFT JOIN users o ON o.id = d.user_id
LEFT JOIN shared_members m ON m.folder_id = d.id AND m.user_id = :user_id
WHERE d.kind IN ('local', 'master') AND visible(d, :user_id, false)
  AND (:key::text IS NULL OR (d.user_id = :user_id AND d.name = :key) OR d.folder = :key OR d.public_id::text = :key)
ORDER BY d.folder;

--! clear_folder_state
DELETE FROM device_folder_state WHERE device_id = :device_id;

--! save_folder_state
INSERT INTO device_folder_state (device_id, folder, files, bytes, last_cycle, last_error, held, failing, mode, updated_at)
VALUES (:device_id, :folder, :files, :bytes, :last_cycle, :last_error, :held, :failing, :mode, :now)
ON CONFLICT (device_id, folder) DO UPDATE SET files = EXCLUDED.files, bytes = EXCLUDED.bytes, last_cycle = EXCLUDED.last_cycle,
  last_error = EXCLUDED.last_error, held = EXCLUDED.held, failing = EXCLUDED.failing, mode = EXCLUDED.mode, updated_at = EXCLUDED.updated_at;

--! folder_states_of_device
SELECT folder, files, bytes, last_cycle, last_error, held, failing, mode, updated_at
FROM device_folder_state WHERE device_id = :device_id ORDER BY folder;

--! folder_states_of_folder
-- The computers that have a folder attached: all of them for the owner, otherwise those of `user`.
SELECT d.folder AS computer, s.files, s.bytes, s.last_cycle, s.last_error, s.held, s.failing, s.mode, s.updated_at
FROM device_folder_state s JOIN devices d ON d.id = s.device_id
WHERE s.folder = :folder AND (:all::boolean OR d.user_id = :user::bigint)
ORDER BY d.folder;

--! shared_members
SELECT u.name AS user_name, m.can_write FROM shared_members m JOIN users u ON u.id = m.user_id
WHERE m.folder_id = :folder_id ORDER BY u.name;

--! add_shared_member
INSERT INTO shared_members (folder_id, user_id, can_write, created_at) VALUES (:folder_id, :user_id, :can_write, :now)
ON CONFLICT (folder_id, user_id) DO UPDATE SET can_write = EXCLUDED.can_write;

--! remove_shared_member
DELETE FROM shared_members WHERE folder_id = :folder_id AND user_id = :user_id;

--! folder_has_live_files
SELECT EXISTS (SELECT 1 FROM files WHERE device_id = :id AND deleted_at IS NULL) AS live;

--! device_file_ids
SELECT id FROM files WHERE device_id = :id;

--! delete_device_events
DELETE FROM events WHERE device_id = :id;

--! delete_folder_device
-- A folder with a source writing into it, or a mirror holding it, is not removed.
DELETE FROM devices WHERE id = :id AND kind IN ('local', 'master') AND NOT EXISTS (SELECT 1 FROM links l WHERE l.folder_id = devices.id);

--! ensure_local_device
INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES (:name, 'local', '!', :now, :user_id)
ON CONFLICT (user_id, name) DO UPDATE SET name = devices.name RETURNING id;

--! device_for_rename : (user_id?)
-- The row of a folder that can be renamed (one of a user, or a master folder), locked so that nothing else renames it meanwhile.
SELECT d.id, d.name, d.kind, d.folder, d.user_id, COALESCE(u.name, '') AS user_name
FROM devices d LEFT JOIN users u ON u.id = d.user_id
WHERE d.folder = :folder AND d.kind IN ('local', 'master') FOR UPDATE OF d;

--! name_taken (user_id?)
SELECT EXISTS (SELECT 1 FROM devices WHERE user_id IS NOT DISTINCT FROM :user_id::bigint AND name = :name AND kind IN ('device', 'local', 'master')) AS taken;

--! master_devices
SELECT id, folder FROM devices WHERE kind = 'master' ORDER BY name;

--! insert_master_folder
INSERT INTO devices (name, kind, token_hash, created_at) VALUES (:name, 'master', '!', :now) ON CONFLICT DO NOTHING RETURNING folder;

--! master_folder_by_public_id
SELECT id, name, folder FROM devices WHERE public_id = :public_id AND kind = 'master';

--! master_folder_names_under
-- The master folders at, inside or around `name`, for the rule that master folders do not nest.
SELECT name FROM devices WHERE kind = 'master' AND (lower(name) = lower(:name) OR starts_with(lower(name), lower(:name) || '/') OR starts_with(lower(:name), lower(name) || '/'));

--! rename_device
-- The trigger `devices_folder_renamed` makes the folder again.
UPDATE devices SET name = :name WHERE id = :id RETURNING folder;

--! rename_folder_in_jobs
UPDATE jobs SET payload = jsonb_set(payload::jsonb, '{device}', to_jsonb(:new::text))::text
WHERE payload::jsonb ->> 'device' = :old;

--! rename_folder_in_alerts
UPDATE alerts SET key = :new WHERE key = :old AND resolved_at IS NULL
  AND NOT EXISTS (SELECT 1 FROM alerts o WHERE o.kind = alerts.kind AND o.key = :new AND o.resolved_at IS NULL);

--! rename_folder_in_folder_state
UPDATE device_folder_state SET folder = :new WHERE folder = :old;

--! device_version_ids
SELECT v.id FROM versions v JOIN files f ON f.id = v.file_id WHERE f.device_id = :id;

--! folder_links : (last_status?, owner?)
-- The sources that write into a folder and the mirrors that hold it (or everything): a user's own sources, or all of them.
SELECT l.direction, l.name, u.name AS owner, l.kind, l.last_status
FROM links l LEFT JOIN users u ON u.id = l.user_id
WHERE (l.direction = 'in' AND l.folder_id = :id AND (:all::boolean OR l.user_id = :user::bigint))
   OR (l.direction = 'out' AND :all::boolean AND (l.folder_id IS NULL OR l.folder_id = :id))
ORDER BY l.name;

--! folder_album
-- The album of the user that is a shortcut to the folder.
SELECT id, name FROM albums WHERE folder_device_id = :id AND user_id = :user::bigint LIMIT 1;
