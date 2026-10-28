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
UPDATE files SET current_version_id = :version_id, deleted_at = NULL, deleted_by = NULL WHERE id = :file_id;

--! set_version_mtime
UPDATE versions SET mtime = :mtime WHERE id = :version_id;

--! content_known
SELECT EXISTS (SELECT 1 FROM file_meta WHERE root_hash = :root) AS known;

--! live_path_with_root
SELECT f.path FROM files f JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id AND f.deleted_at IS NULL AND v.root_hash = :root
ORDER BY f.path = :prefer DESC, f.id LIMIT 1;

--! case_clash
-- A live path that is the same name as `path` spelled differently, other than the one `path` is renamed from (empty for none).
SELECT f.path FROM files f
WHERE f.device_id = :device_id AND f.deleted_at IS NULL AND lower(f.path) = lower(:path) AND f.path <> :path
  AND f.path <> :renamed_from
  AND NOT EXISTS (SELECT 1 FROM files s WHERE s.device_id = :device_id AND s.path = :path AND s.deleted_at IS NULL)
LIMIT 1;

--! folder_spellings
-- The other spellings of a folder (`prefix`, without a trailing slash) that live paths have: the folder part of paths that are
-- the same name as `prefix` but not spelled the same.
SELECT DISTINCT left(f.path, char_length(:prefix)) AS spelling FROM files f
WHERE f.device_id = :device_id AND f.deleted_at IS NULL
  AND lower(f.path) LIKE lower(replace(replace(replace(:prefix, '\', '\\'), '%', '\%'), '_', '\_')) || '/%'
  AND left(f.path, char_length(:prefix) + 1) <> :prefix || '/'
LIMIT 5;

--! deleted_in_web_app
SELECT 1 FROM files WHERE device_id = :device_id AND path = :path AND deleted_at IS NOT NULL AND deleted_by = 'web';

--! live_content : (root_hash, size)
SELECT v.root_hash, v.size FROM files f JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id AND f.path = :path AND f.deleted_at IS NULL;

--! delete_path (deleted_by?) : (current_version_id?)
UPDATE files SET deleted_at = :ts, deleted_by = :deleted_by WHERE device_id = :device_id AND path = :path AND deleted_at IS NULL
RETURNING current_version_id;

--! vacate_renamed : (current_version_id?)
-- The old path of a rename leaves the live tree. It is not a deletion: nothing is logged, and the row is hidden from the
-- trash and from every listing (`deleted_by = 'rename'`) until retention removes it with the versions it still holds.
UPDATE files SET deleted_at = :ts, deleted_by = 'rename' WHERE device_id = :device_id AND path = :path AND deleted_at IS NULL
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
  AND current_version_id IS NOT NULL AND (deleted_at IS NULL OR (:include_deleted::boolean AND deleted_by IS DISTINCT FROM 'rename'))
ORDER BY path;

--! version_to_restore : (current_version_id?, deleted_at?)
SELECT f.current_version_id, f.deleted_at, v.root_hash, v.size, v.mtime, v.mode
FROM files f JOIN versions v ON v.file_id = f.id
WHERE f.device_id = :device_id AND f.path = :path AND v.id = :version_id;

--! folders_below : (changed?, deleted_at?)
SELECT split_part(substr(f.path, :from), '/', 1) AS name, COUNT(f.id) AS files, COALESCE(SUM(v.size), 0)::BIGINT AS bytes,
       MAX(v.created_at) AS changed, CASE WHEN bool_and(f.deleted_at IS NOT NULL) THEN MAX(f.deleted_at) END AS deleted_at
FROM files f LEFT JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id AND f.path LIKE :below AND (f.deleted_at IS NULL OR (:include_deleted::boolean AND f.deleted_by IS DISTINCT FROM 'rename'))
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

--! changes_since : (root_hash?, size?, mtime?, mode?, deleted_at?, deleted_by?)
-- The paths of a device that were written or deleted at or after `since` (everything when 0), as they are now: for a
-- live path its current content, for a path in the trash its deletion. Renames leave the old path deleted.
SELECT f.path, f.deleted_at, f.deleted_by, v.root_hash, v.size, v.mtime, v.mode
FROM files f LEFT JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id
  AND ((f.deleted_at IS NOT NULL AND f.deleted_at >= :since) OR (f.deleted_at IS NULL AND v.created_at >= :since))
ORDER BY f.path;

--! live_files_below
SELECT f.path, v.size, v.mtime FROM files f JOIN versions v ON v.id = f.current_version_id
WHERE f.device_id = :device_id AND f.deleted_at IS NULL AND (:path = '' OR f.path = :path OR f.path LIKE :below);

--! file_location
-- Where a live file is and which content it holds.
SELECT d.folder, f.path, v.root_hash FROM files f
JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
WHERE f.id = :id AND f.deleted_at IS NULL;

--! file_holds_content
-- Whether the path of a device still holds this content.
SELECT EXISTS (SELECT 1 FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
               WHERE d.folder = :device AND f.path = :path AND v.root_hash = :root) AS holds;

--! content_is_stored
-- Whether any version has this content.
SELECT EXISTS (SELECT 1 FROM versions WHERE root_hash = :root) AS stored;

--! save_file_meta (width?, height?, taken_at?, duration_ms?, codec?, lat?, lon?, caption?, keywords?)
-- What was read from a content: kind, size, time and place taken, EXIF. Once per content, not per file.
INSERT INTO file_meta (root_hash, mime, width, height, taken_at, exif, read_at, kind, duration_ms, codec, lat, lon, caption, keywords)
VALUES (:root, :mime, :width, :height, :taken_at, :exif, :read_at, :kind, :duration_ms, :codec, :lat, :lon, :caption, :keywords)
ON CONFLICT (root_hash) DO UPDATE SET mime = excluded.mime, width = excluded.width, height = excluded.height,
    taken_at = excluded.taken_at, exif = excluded.exif, read_at = excluded.read_at, kind = excluded.kind,
    duration_ms = excluded.duration_ms, codec = excluded.codec, lat = excluded.lat, lon = excluded.lon,
    caption = excluded.caption, keywords = excluded.keywords;

--! files_in_folder
-- The live files directly in a folder of a device with their content: `prefix` is the folder's path with a slash at the
-- end, empty for the top of the device. Used to find the pictures and the .nfo that sit next to a video.
SELECT f.path, v.root_hash FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
WHERE d.folder = :device AND f.deleted_at IS NULL AND starts_with(f.path, :prefix)
  AND position('/' IN substr(f.path, length(:prefix) + 1)) = 0;

--! file_codec : (codec?)
-- The video codec read from a content, if it was read.
SELECT codec FROM file_meta WHERE root_hash = :root;

--! version_record
-- What a version holds, to record the same content under another path.
SELECT root_hash, size, mtime, mode FROM versions WHERE id = :version_id;

--! reserved_master_paths
-- Master files whose path starts with `users`, the folder of the users' folders: they cannot be recorded or moved.
SELECT path FROM files WHERE device_id = 0 AND deleted_at IS NULL AND lower(split_part(path, '/', 1)) = 'users'
ORDER BY path LIMIT 10;
