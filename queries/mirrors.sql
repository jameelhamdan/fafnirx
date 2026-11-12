--! mirror_key
-- All that is needed, with the storage, to restore a backup mirror.
SELECT kind, config FROM links WHERE direction = 'out' AND name = :name;

--! live_files_for_mirror (folder_id?, device_id?)
-- Every live file of the folder (or the folders of the device) a mirror holds, all of them when it holds none in particular.
SELECT f.folder_id, fo.key AS folder, f.path, v.root_hash, v.size, v.mtime
FROM files f JOIN folders fo ON fo.id = f.folder_id JOIN versions v ON v.id = f.current_version_id
WHERE f.deleted_at IS NULL AND (:folder_id::bigint IS NULL OR f.folder_id = :folder_id::bigint)
  AND (:device_id::bigint IS NULL OR fo.device_id = :device_id::bigint)
ORDER BY fo.key, f.path;

--! mirror_scopes
-- What a mirror can be told to hold: a device (its folders together) or one folder.
SELECT key FROM devices UNION ALL SELECT key FROM folders ORDER BY 1;

--! folder_keys
SELECT id, key FROM folders;
