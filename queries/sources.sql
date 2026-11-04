--! list_sources : (last_run_at?, last_ok_at?, last_status?, last_message?)
-- `files` and `bytes` are what the source collected and is still there, not what the folder holds.
SELECT s.name, s.kind, u.name AS owner, s.config, s.interval_secs, s.enabled, s.last_run_at, s.last_ok_at, s.last_status,
       s.last_message, s.device_id, d.folder, s.folder_prefix,
       (SELECT COUNT(*) FROM source_items i JOIN files f ON f.device_id = s.device_id AND f.path = s.folder_prefix || i.path AND f.deleted_at IS NULL
        WHERE i.source_id = s.id) AS files,
       (SELECT COALESCE(SUM(v.size), 0)::BIGINT FROM source_items i JOIN files f ON f.device_id = s.device_id AND f.path = s.folder_prefix || i.path AND f.deleted_at IS NULL
        JOIN versions v ON v.id = f.current_version_id WHERE i.source_id = s.id) AS bytes
FROM sources s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = s.user_id
WHERE :all::boolean OR s.user_id = :user::bigint ORDER BY u.name, s.name;

--! source_by_owner
SELECT s.id, d.folder FROM sources s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = s.user_id
WHERE u.name = :owner AND s.name = :name;

--! insert_source_device
INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES (:name, 'local', '!', :now, :user_id)
ON CONFLICT (user_id, name) DO NOTHING RETURNING id, folder;

--! insert_source
INSERT INTO sources (name, kind, config, interval_secs, device_id, folder_prefix, user_id, created_at)
VALUES (:name, :kind, :config, :interval_secs, :device_id, :folder_prefix, :user_id, :now);

--! source_name_taken
SELECT EXISTS (SELECT 1 FROM sources WHERE user_id = :user_id AND name = :name AND id <> :id) AS taken;

--! source_for_edit : (last_status?)
SELECT s.id, s.name, s.kind, s.config, s.interval_secs, s.device_id, s.folder_prefix, s.user_id, s.last_status
FROM sources s WHERE s.id = :id FOR UPDATE;

--! update_source
UPDATE sources SET name = :name, config = :config, interval_secs = :interval_secs, device_id = :device_id, folder_prefix = :folder_prefix
WHERE id = :id;

--! source_destination
SELECT s.device_id, d.folder, s.folder_prefix FROM sources s JOIN devices d ON d.id = s.device_id WHERE s.id = :id;

--! delete_source_items
DELETE FROM source_items WHERE source_id = :source_id;

--! delete_source
DELETE FROM sources WHERE id = :id;

--! request_source_run
UPDATE sources SET run_requested = 1 WHERE id = :id;

--! continue_source_run
UPDATE sources SET run_requested = 1 WHERE id = :id AND enabled = 1;

--! toggle_source
UPDATE sources SET enabled = 1 - enabled WHERE id = :id;

--! claim_source
UPDATE sources SET last_status = 'running', last_run_at = :now, run_requested = 0, last_message = ''
WHERE id = (SELECT id FROM sources
            WHERE (run_requested = 1 OR (enabled = 1 AND (last_run_at IS NULL OR last_run_at + interval_secs <= :now)))
              AND last_status IS DISTINCT FROM 'running'
            ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED)
RETURNING id, kind, config, device_id, folder_prefix, (SELECT d.folder FROM devices d WHERE d.id = device_id) AS folder,
          (SELECT u.name FROM users u WHERE u.id = user_id) || '/' || name AS key;

--! finish_source (ok_at?)
UPDATE sources SET last_status = :status, last_message = :message, last_ok_at = COALESCE(:ok_at, last_ok_at) WHERE id = :id;

--! upsert_source_item
INSERT INTO source_items (source_id, remote_id, path, remote_version) VALUES (:source_id, :remote_id, :path, :remote_version)
ON CONFLICT (source_id, remote_id) DO UPDATE SET path = excluded.path, remote_version = excluded.remote_version,
                                                 removed_in_web_app = FALSE;

--! source_items
SELECT remote_id, path, remote_version FROM source_items WHERE source_id = :source_id;

--! live_paths
-- The live files below the source's folder, named from there (the way its items are).
SELECT substr(path, length(:prefix::text) + 1) AS path FROM files
WHERE device_id = :device_id AND deleted_at IS NULL AND starts_with(path, :prefix::text);

--! removed_in_web_app
SELECT substr(path, length(:prefix::text) + 1) AS path FROM files
WHERE device_id = :device_id AND deleted_at IS NOT NULL AND deleted_by = 'web' AND starts_with(path, :prefix::text)
UNION SELECT path FROM source_items WHERE source_id = :source_id AND removed_in_web_app;

--! delete_source_item
DELETE FROM source_items WHERE source_id = :source_id AND remote_id = :remote_id;

--! fail_interrupted_sources
-- A restart interrupts whatever was running: it is shown as failed until it runs again.
UPDATE sources SET last_status = 'failed', last_message = 'interrupted by a restart' WHERE last_status = 'running';

--! move_source_folders
-- A folder moved: the sources that write into it, or into a folder below it, write into the new place. What they collected is
-- named below their folder, so it moves with them. `from` and `to` end with a slash.
UPDATE sources SET folder_prefix = :to || substr(folder_prefix, char_length(:from) + 1)
WHERE device_id = :device_id AND starts_with(folder_prefix, :from);
