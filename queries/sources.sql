--! list_sources : (last_run_at?, last_ok_at?, last_status?, last_message?)
SELECT s.name, u.name AS owner, s.config, s.interval_secs, s.enabled, s.last_run_at, s.last_ok_at, s.last_status,
       s.last_message, d.total_bytes AS bytes, d.file_count AS files, s.device_id, d.folder
FROM sources s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = d.user_id
WHERE :all::boolean OR d.user_id = :user::bigint ORDER BY u.name, s.name;

--! source_by_owner
SELECT s.id, d.folder FROM sources s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = d.user_id
WHERE u.name = :owner AND s.name = :name;

--! insert_source_device
INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES (:name, 'source', '!', :now, :user_id)
ON CONFLICT (user_id, name) DO NOTHING RETURNING id, folder;

--! insert_source
INSERT INTO sources (name, kind, config, interval_secs, device_id, created_at)
VALUES (:name, 'gdrive', :config, :interval_secs, :device_id, :now);

--! source_device
SELECT s.device_id, d.folder FROM sources s JOIN devices d ON d.id = s.device_id WHERE s.id = :id;

--! delete_source_items
DELETE FROM source_items WHERE source_id = :source_id;

--! delete_source
DELETE FROM sources WHERE id = :id;

--! request_source_run
UPDATE sources SET run_requested = 1 WHERE id = :id;

--! toggle_source
UPDATE sources SET enabled = 1 - enabled WHERE id = :id;

--! claim_source
UPDATE sources SET last_status = 'running', last_run_at = :now, run_requested = 0, last_message = ''
WHERE id = (SELECT id FROM sources
            WHERE (run_requested = 1 OR (enabled = 1 AND (last_run_at IS NULL OR last_run_at + interval_secs <= :now)))
              AND last_status IS DISTINCT FROM 'running'
            ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED)
RETURNING id, kind, config, device_id, (SELECT d.folder FROM devices d WHERE d.id = device_id) AS folder;

--! finish_source (ok_at?)
UPDATE sources SET last_status = :status, last_message = :message, last_ok_at = COALESCE(:ok_at, last_ok_at) WHERE id = :id;

--! upsert_source_item
INSERT INTO source_items (source_id, remote_id, path, remote_version) VALUES (:source_id, :remote_id, :path, :remote_version)
ON CONFLICT (source_id, remote_id) DO UPDATE SET path = excluded.path, remote_version = excluded.remote_version,
                                                 removed_in_web_app = FALSE;

--! source_items
SELECT remote_id, path, remote_version FROM source_items WHERE source_id = :source_id;

--! live_paths
SELECT path FROM files WHERE device_id = :device_id AND deleted_at IS NULL;

--! removed_in_web_app
SELECT path FROM files WHERE device_id = :device_id AND deleted_at IS NOT NULL AND deleted_by = 'web'
UNION SELECT path FROM source_items WHERE source_id = :source_id AND removed_in_web_app;

--! delete_source_item
DELETE FROM source_items WHERE source_id = :source_id AND remote_id = :remote_id;

--! fail_interrupted_sources
-- A restart interrupts whatever was running: it is shown as failed until it runs again.
UPDATE sources SET last_status = 'failed', last_message = 'interrupted by a restart' WHERE last_status = 'running';
