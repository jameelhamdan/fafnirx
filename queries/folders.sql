-- Folders: what holds files. A folder of a user (`user`), a master folder (`master`) or a directory a device sends (`backup`).

--! ensure_backup_folder
-- The folder of a directory of a device, made when the device first sends a file of it.
INSERT INTO folders (user_id, device_id, name, kind, created_at)
SELECT user_id, id, :name, 'backup', :now FROM devices WHERE id = :device_id
ON CONFLICT (device_id, name) WHERE kind = 'backup' DO UPDATE SET name = folders.name RETURNING id, key;

--! backup_folder_of_device
SELECT id, key FROM folders WHERE device_id = :device_id AND name = :name;

--! backup_folders_of_device : (id, key, name)
SELECT id, key, name FROM folders WHERE device_id = :device_id ORDER BY name;

--! update_totals
UPDATE folders f SET file_count = t.live, total_bytes = t.bytes, deleted_files = t.gone, totals_stale = FALSE
FROM (SELECT COUNT(*) FILTER (WHERE x.deleted_at IS NULL) AS live,
             COALESCE(SUM(v.size) FILTER (WHERE x.deleted_at IS NULL), 0)::BIGINT AS bytes,
             COUNT(*) FILTER (WHERE x.deleted_at IS NOT NULL AND x.deleted_by IS DISTINCT FROM 'rename') AS gone
      FROM files x LEFT JOIN versions v ON v.id = x.current_version_id
      WHERE x.folder_id = :id) t
WHERE f.id = :id;

--! stale_folder_ids
SELECT f.id FROM folders f
WHERE f.totals_stale AND NOT EXISTS (SELECT 1 FROM device_activity a WHERE a.device_id = f.device_id AND a.updated_at > :syncing_since);

--! visible_totals
SELECT COALESCE(SUM(file_count), 0)::BIGINT AS files, COALESCE(SUM(total_bytes), 0)::BIGINT AS bytes,
       COALESCE(SUM(deleted_files), 0)::BIGINT AS deleted
FROM folders fo WHERE visible(fo, :user::bigint, :all::boolean);

--! mark_totals_stale
UPDATE folders SET totals_stale = TRUE WHERE id = ANY(:ids);

--! lock_folder_key : (key)
SELECT key FROM folders WHERE id = :id FOR UPDATE;

--! folder_kind
SELECT kind FROM folders WHERE id = :id;

--! folder_by_key
SELECT id, key FROM folders WHERE key = :key;

--! folder_id_and_kind_by_key
SELECT id, kind FROM folders WHERE key = :key;

--! visible_folders
-- Every folder a user can see (all for an administrator), by key.
SELECT f.key, f.kind, f.file_count AS files, f.total_bytes AS bytes
FROM folders f
WHERE visible(f, :user::bigint, :all::boolean)
ORDER BY f.key;

--! folder_totals
-- The folders a user can see (all for an administrator) with what each holds, as of its last total; the biggest first.
SELECT f.key, f.total_bytes AS bytes, f.file_count AS files
FROM folders f
WHERE visible(f, :user::bigint, :all::boolean) AND f.file_count > 0
ORDER BY f.total_bytes DESC, f.key;

--! folders_for_user (lookup?) : (last_collected?)
-- Every folder a user can use, whoever owns it: their own, those whose owner added them, and the master folders (`visible`, the one
-- rule for what a user sees). `key` is how clients name it (`<owner>/<name>`, `master:<path>`). A read-only account never writes.
-- `lookup` narrows it to one: the name of one's own, its full key or its stable id.
SELECT f.id, f.public_id, f.name, f.key, COALESCE(o.name, '') AS owner, COALESCE(f.user_id = :user_id, FALSE) AS mine,
       me.role <> 'readonly' AND (f.kind = 'master' OR f.user_id = :user_id OR COALESCE(m.can_write, FALSE)) AS can_write,
       f.kind = 'master' OR EXISTS (SELECT 1 FROM shared_members s WHERE s.folder_id = f.id) AS shared, f.kind = 'master' AS master,
       f.created_at, f.file_count AS files, f.total_bytes AS bytes, f.last_collected
FROM folders f
JOIN users me ON me.id = :user_id AND me.disabled_at IS NULL
LEFT JOIN users o ON o.id = f.user_id
LEFT JOIN shared_members m ON m.folder_id = f.id AND m.user_id = :user_id
WHERE f.kind IN ('user', 'master') AND visible(f, :user_id, false)
  AND (:lookup::text IS NULL OR (f.user_id = :user_id AND f.name = :lookup) OR f.key = :lookup OR f.public_id::text = :lookup)
ORDER BY f.key;

--! shared_members
SELECT u.name AS user_name, m.can_write FROM shared_members m JOIN users u ON u.id = m.user_id
WHERE m.folder_id = :folder_id ORDER BY u.name;

--! add_shared_member
INSERT INTO shared_members (folder_id, user_id, can_write, created_at) VALUES (:folder_id, :user_id, :can_write, :now)
ON CONFLICT (folder_id, user_id) DO UPDATE SET can_write = EXCLUDED.can_write;

--! remove_shared_member
DELETE FROM shared_members WHERE folder_id = :folder_id AND user_id = :user_id;

--! folder_has_live_files
SELECT EXISTS (SELECT 1 FROM files WHERE folder_id = :id AND deleted_at IS NULL) AS live;

--! folder_file_ids
SELECT id FROM files WHERE folder_id = :id;

--! delete_folder_events
DELETE FROM events WHERE folder_id = :id;

--! delete_folder
-- A folder with a source writing into it, or a mirror holding it, is not removed.
DELETE FROM folders WHERE id = :id AND NOT EXISTS (SELECT 1 FROM links l WHERE l.folder_id = folders.id);

--! ensure_user_folder
INSERT INTO folders (name, kind, created_at, user_id) VALUES (:name, 'user', :now, :user_id)
ON CONFLICT (user_id, name) WHERE kind = 'user' DO UPDATE SET name = folders.name RETURNING id;

--! create_user_folder
INSERT INTO folders (name, kind, created_at, user_id) VALUES (:name, 'user', :now, :user_id)
ON CONFLICT DO NOTHING RETURNING key;

--! folder_for_rename : (user_id?)
-- The row of a folder that can be renamed (one of a user, or a master folder), locked so that nothing else renames it meanwhile.
SELECT f.id, f.name, f.kind, f.key, f.user_id, COALESCE(u.name, '') AS user_name
FROM folders f LEFT JOIN users u ON u.id = f.user_id
WHERE f.key = :key AND f.kind IN ('user', 'master') FOR UPDATE OF f;

--! name_taken (user_id?)
-- Whether a user already has a device or folder of this name (they share the directory `files/users/<user>/`), or a master folder.
SELECT EXISTS (SELECT 1 FROM devices WHERE user_id IS NOT DISTINCT FROM :user_id::bigint AND name = :name)
    OR EXISTS (SELECT 1 FROM folders WHERE user_id IS NOT DISTINCT FROM :user_id::bigint AND name = :name AND kind IN ('user', 'master')) AS taken;

--! master_folders
SELECT id, key FROM folders WHERE kind = 'master' ORDER BY name;

--! insert_master_folder
INSERT INTO folders (name, kind, created_at) VALUES (:name, 'master', :now) ON CONFLICT DO NOTHING RETURNING key;

--! master_folder_by_public_id
SELECT id, name, key FROM folders WHERE public_id = :public_id AND kind = 'master';

--! master_folder_names_under
-- The master folders at, inside or around `name`, for the rule that master folders do not nest.
SELECT name FROM folders WHERE kind = 'master' AND (lower(name) = lower(:name) OR starts_with(lower(name), lower(:name) || '/') OR starts_with(lower(:name), lower(name) || '/'));

--! rename_folder
-- The trigger `folders_key_renamed` makes the key again.
UPDATE folders SET name = :name WHERE id = :id RETURNING key;

--! rename_folder_in_jobs
UPDATE jobs SET payload = jsonb_set(payload::jsonb, '{folder}', to_jsonb(:new::text))::text
WHERE payload::jsonb ->> 'folder' = :old;

--! rename_folder_in_alerts
UPDATE alerts SET key = :new WHERE key = :old AND resolved_at IS NULL
  AND NOT EXISTS (SELECT 1 FROM alerts o WHERE o.kind = alerts.kind AND o.key = :new AND o.resolved_at IS NULL);

--! folder_version_ids
SELECT v.id FROM versions v JOIN files f ON f.id = v.file_id WHERE f.folder_id = :id;

--! folder_links : (last_status?, owner?)
-- The sources that write into a folder and the mirrors that hold it (or its device, or everything): a user's own sources, or all of them.
SELECT l.direction, l.name, u.name AS owner, l.kind, l.last_status
FROM links l LEFT JOIN users u ON u.id = l.user_id
WHERE (l.direction = 'in' AND l.folder_id = :id AND (:all::boolean OR l.user_id = :user::bigint))
   OR (l.direction = 'out' AND :all::boolean AND (l.folder_id = :id OR l.device_id = (SELECT device_id FROM folders WHERE id = :id)
                                                  OR (l.folder_id IS NULL AND l.device_id IS NULL)))
ORDER BY l.name;

--! folder_album
-- The album of the user that is a shortcut to the folder.
SELECT id, name FROM albums WHERE folder_id = :id AND user_id = :user::bigint LIMIT 1;
