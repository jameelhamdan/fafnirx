--! mirror_key
-- All that is needed, with the storage, to restore a backup mirror.
SELECT kind, config FROM links WHERE direction = 'out' AND name = :name;

--! live_files_for_mirror (folder_id?)
-- Every live file of the folder a mirror holds (all of them when it holds none in particular).
SELECT f.device_id, d.folder, f.path, v.root_hash, v.size, v.mtime
FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
WHERE f.deleted_at IS NULL AND (:folder_id::bigint IS NULL OR f.device_id = :folder_id::bigint)
ORDER BY d.folder, f.path;

--! device_folders
SELECT id, folder FROM devices;
