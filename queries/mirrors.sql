--! list_mirrors : (last_run_at?, last_ok_at?, last_status?, last_message?, last_files?, last_bytes?)
SELECT name, kind, remote, remote_type, options, remote_path, scope, interval_secs, keep_runs, enabled, last_run_at, last_ok_at,
       last_status, last_message, last_files, last_bytes
FROM mirrors ORDER BY name;

--! mirror_id
SELECT id FROM mirrors WHERE name = :name;

--! insert_mirror
INSERT INTO mirrors (name, kind, crypt_password, crypt_salt, remote, remote_type, options, remote_path, scope, interval_secs, keep_runs, created_at)
VALUES (:name, :kind, :crypt_password, :crypt_salt, :remote, :remote_type, :options, :remote_path, :scope, :interval_secs, :keep_runs, :now)
ON CONFLICT (name) DO NOTHING;

--! mirror_key
-- The two passwords of a backup mirror: all that is needed, with the storage, to restore it.
SELECT kind, remote, remote_type, options, remote_path, crypt_password, crypt_salt FROM mirrors WHERE name = :name;

--! delete_mirror_items
DELETE FROM mirror_items WHERE mirror_id = :mirror_id;

--! delete_mirror
DELETE FROM mirrors WHERE id = :id;

--! request_mirror_run
UPDATE mirrors SET run_requested = 1 WHERE id = :id;

--! toggle_mirror
UPDATE mirrors SET enabled = 1 - enabled WHERE id = :id;

--! claim_mirror
UPDATE mirrors SET last_status = 'running', last_run_at = :now, run_requested = 0, last_message = ''
WHERE id = (SELECT id FROM mirrors
            WHERE (run_requested = 1 OR (enabled = 1 AND (last_run_at IS NULL OR last_run_at + interval_secs <= :now)))
              AND last_status IS DISTINCT FROM 'running'
            ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED)
RETURNING id, name, kind, crypt_password, crypt_salt, remote_type, options, remote_path, scope, keep_runs, remote;

--! finish_mirror (ok_at?)
UPDATE mirrors SET last_status = :status, last_message = :message, last_ok_at = COALESCE(:ok_at, last_ok_at) WHERE id = :id;

--! set_mirror_totals
UPDATE mirrors SET last_files = :files, last_bytes = :bytes WHERE id = :id;

--! live_files_for_mirror
-- Every live file at or below the folder `scope` (all of them when it is empty).
SELECT f.device_id, d.folder, f.path, v.root_hash, v.size, v.mtime
FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
WHERE f.deleted_at IS NULL AND (:scope::text = '' OR starts_with(d.folder || '/' || f.path, :scope::text || '/'))
ORDER BY d.folder, f.path;

--! mirror_items
SELECT device_id, path, root_hash FROM mirror_items WHERE mirror_id = :mirror_id;

--! upsert_mirror_item
INSERT INTO mirror_items (mirror_id, device_id, path, root_hash) VALUES (:mirror_id, :device_id, :path, :root_hash)
ON CONFLICT (mirror_id, device_id, path) DO UPDATE SET root_hash = excluded.root_hash;

--! delete_mirror_item
DELETE FROM mirror_items WHERE mirror_id = :mirror_id AND device_id = :device_id AND path = :path;

--! device_folders
SELECT id, folder FROM devices;

--! scope_places
SELECT id, folder, kind FROM devices;

--! folders_below
-- The folders directly inside `prefix` (`a/b/`, or empty for the top) among the live files of a device.
SELECT DISTINCT split_part(substr(f.path, length(:prefix::text) + 1), '/', 1) AS name
FROM files f
WHERE f.device_id = :device_id AND f.deleted_at IS NULL AND starts_with(f.path, :prefix::text)
  AND strpos(substr(f.path, length(:prefix::text) + 1), '/') > 0
ORDER BY 1 LIMIT 500;

--! fail_interrupted_mirrors
-- A restart interrupts whatever was running: it is shown as failed until it runs again.
UPDATE mirrors SET last_status = 'failed', last_message = 'interrupted by a restart' WHERE last_status = 'running';
