--! prune_versions
UPDATE versions SET pruned_at = :now
WHERE file_id = :file_id AND pruned_at IS NULL
  AND id NOT IN (SELECT id FROM versions WHERE file_id = :file_id AND pruned_at IS NULL ORDER BY id DESC LIMIT :keep);

--! lock_file_row : (current_version_id?, deleted_at?)
SELECT id, current_version_id, deleted_at FROM files WHERE device_id = :device_id AND path = :path FOR UPDATE;

--! version_has_content
SELECT root_hash = :root AS same FROM versions WHERE id = :version_id;

--! ensure_file
INSERT INTO files (device_id, path) VALUES (:device_id, :path)
ON CONFLICT (device_id, path) DO UPDATE SET path = excluded.path RETURNING id;

--! insert_version
INSERT INTO versions (file_id, root_hash, size, mtime, mode, created_at)
VALUES (:file_id, :root, :size, :mtime, :mode, :created_at) RETURNING id;

--! make_current
UPDATE files SET current_version_id = :version_id, deleted_at = NULL WHERE id = :file_id;

--! set_version_mtime
UPDATE versions SET mtime = :mtime WHERE id = :version_id;

--! content_known
SELECT EXISTS (SELECT 1 FROM file_meta WHERE root_hash = :root) AS known;

--! live_path_with_root
SELECT f.path FROM files f JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id AND f.deleted_at IS NULL AND v.root_hash = :root
ORDER BY f.path = :prefer DESC, f.id LIMIT 1;

--! deleted_in_web_app
SELECT 1 FROM files WHERE device_id = :device_id AND path = :path AND deleted_at IS NOT NULL AND deleted_by = 'web';

--! live_content : (root_hash, size)
SELECT v.root_hash, v.size FROM files f JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id AND f.path = :path AND f.deleted_at IS NULL;

--! delete_path (deleted_by?) : (current_version_id?)
UPDATE files SET deleted_at = :ts, deleted_by = :deleted_by WHERE device_id = :device_id AND path = :path AND deleted_at IS NULL
RETURNING current_version_id;

--! keep_source_item_deleted
UPDATE source_items SET removed_in_web_app = TRUE
WHERE path = :path AND source_id IN (SELECT id FROM sources WHERE device_id = :device_id);

--! touch_device
UPDATE devices SET last_seen = :now, totals_stale = TRUE WHERE id = :id;

--! touch_collected
UPDATE devices SET last_seen = :now, last_collected = :now, totals_stale = TRUE WHERE id = :id;

--! mark_stale
UPDATE devices SET totals_stale = TRUE WHERE id = :id;

--! file_info : (current_version_id?, deleted_at?, mime?, width?, height?, taken_at?, exif?)
SELECT f.id, f.current_version_id, f.deleted_at, v.size, v.mtime, v.created_at, v.root_hash,
       m.mime, m.width, m.height, m.taken_at, m.exif
FROM files f JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash
WHERE f.device_id = :device_id AND f.path = :path;

--! file_versions : (pruned_at?)
SELECT id, created_at, size, mtime, root_hash, pruned_at FROM versions WHERE file_id = :file_id ORDER BY id DESC;

--! expand_paths
SELECT path FROM files
WHERE device_id = :device_id AND (:path = '' OR path = :path OR path LIKE :below)
  AND current_version_id IS NOT NULL AND (:include_deleted::boolean OR deleted_at IS NULL)
ORDER BY path;

--! version_to_restore : (current_version_id?, deleted_at?)
SELECT f.current_version_id, f.deleted_at, v.root_hash, v.size, v.mtime, v.mode
FROM files f JOIN versions v ON v.file_id = f.id
WHERE f.device_id = :device_id AND f.path = :path AND v.id = :version_id;

--! folders_below : (changed?, deleted_at?)
SELECT split_part(substr(f.path, :from), '/', 1) AS name, COUNT(f.id) AS files, COALESCE(SUM(v.size), 0)::BIGINT AS bytes,
       MAX(v.created_at) AS changed, CASE WHEN bool_and(f.deleted_at IS NOT NULL) THEN MAX(f.deleted_at) END AS deleted_at
FROM files f LEFT JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id AND f.path LIKE :below AND (:include_deleted::boolean OR f.deleted_at IS NULL)
      AND strpos(substr(f.path, :from), '/') > 0
GROUP BY 1 ORDER BY lower(split_part(substr(f.path, :from), '/', 1));

--! file_row : (current_version_id?, deleted_at?)
SELECT id, current_version_id, deleted_at FROM files WHERE device_id = :device_id AND path = :path;

--! version_of_file
SELECT root_hash, size FROM versions WHERE id = :version_id AND file_id = :file_id;

--! file_by_id : (codec?, deleted_at?)
SELECT d.folder, f.path, f.current_version_id, f.deleted_at, v.root_hash, v.size, m.codec
FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash WHERE f.id = :id;

--! current_mtime
SELECT v.mtime FROM files f JOIN versions v ON v.id = f.current_version_id WHERE f.device_id = :device_id AND f.path = :path;

--! live_files_below
SELECT f.path, v.size, v.mtime FROM files f JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id AND f.deleted_at IS NULL AND (:path = '' OR f.path = :path OR f.path LIKE :below);
