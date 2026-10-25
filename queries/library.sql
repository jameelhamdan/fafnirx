-- The photo and video library: a view of the files with what the viewing user said about them.
--
-- Every query that lists files takes the same three numbers for who is looking:
--   :uid         the viewing user (their favorites, ratings and tags are the ones shown)
--   :all         true when everything is visible (administrators)
--   :scope_user  the user whose devices are visible besides the master files (-1: nobody's)
-- A file is visible when `(:all OR d.kind = 'master' OR d.user_id = :scope_user)`.
--
-- When a file was taken: as the user corrected it, else as the camera said, else when the file last changed
-- (unix seconds; photos have no time zone, so everything is read as UTC):
--   COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)

--! timeline_days (device?, kind?, tag?, month?)
-- How many files each day holds, newest day first: the skeleton of the timeline. Filters: `scope` is timeline
-- (not archived), favorites or archive; the others narrow it down when given. `month` is YYYY-MM.
SELECT to_char(to_timestamp(COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)) AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day,
       COUNT(*) AS n
FROM files f
JOIN devices d ON d.id = f.device_id
JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash
LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = :uid
WHERE f.deleted_at IS NULL AND f.media IS NOT NULL
  AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
  AND CASE :scope::text
        WHEN 'timeline' THEN NOT COALESCE(s.archived, false)
        WHEN 'favorites' THEN COALESCE(s.favorite, false)
        WHEN 'archive' THEN COALESCE(s.archived, false)
      END
  AND (:device::text IS NULL OR d.folder = :device)
  AND (:kind::text IS NULL OR f.media = :kind)
  AND (:tag::bigint IS NULL OR EXISTS (SELECT 1 FROM file_tags t WHERE t.file_id = f.id AND t.tag_id = :tag))
  AND (:month::text IS NULL
       OR to_char(to_timestamp(COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)) AT TIME ZONE 'UTC', 'YYYY-MM') = :month)
GROUP BY 1 ORDER BY 1 DESC;

--! timeline_assets (device?, kind?, tag?, month?) : Asset(width?, height?, duration_ms?, rating?, lat?, lon?)
-- The files of the timeline with the same filters, newest first.
SELECT f.id, d.folder, f.path, v.size, v.root_hash, m.width, m.height, m.duration_ms,
       COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) AS shown_at,
       COALESCE(s.favorite, false) AS favorite, COALESCE(s.archived, false) AS archived, s.rating,
       COALESCE(s.lat, m.lat) AS lat, COALESCE(s.lon, m.lon) AS lon
FROM files f
JOIN devices d ON d.id = f.device_id
JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash
LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = :uid
WHERE f.deleted_at IS NULL AND f.media IS NOT NULL
  AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
  AND CASE :scope::text
        WHEN 'timeline' THEN NOT COALESCE(s.archived, false)
        WHEN 'favorites' THEN COALESCE(s.favorite, false)
        WHEN 'archive' THEN COALESCE(s.archived, false)
      END
  AND (:device::text IS NULL OR d.folder = :device)
  AND (:kind::text IS NULL OR f.media = :kind)
  AND (:tag::bigint IS NULL OR EXISTS (SELECT 1 FROM file_tags t WHERE t.file_id = f.id AND t.tag_id = :tag))
  AND (:month::text IS NULL
       OR to_char(to_timestamp(COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)) AT TIME ZONE 'UTC', 'YYYY-MM') = :month)
ORDER BY shown_at DESC, f.id DESC LIMIT :limit;

--! map_points (device?, kind?, tag?, month?)
-- Where each file with a place was taken, newest first, for the map.
SELECT f.id, COALESCE(s.lat, m.lat) AS lat, COALESCE(s.lon, m.lon) AS lon,
       COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) AS shown_at
FROM files f
JOIN devices d ON d.id = f.device_id
JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash
LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = :uid
WHERE f.deleted_at IS NULL AND f.media IS NOT NULL
  AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
  AND CASE :scope::text
        WHEN 'timeline' THEN NOT COALESCE(s.archived, false)
        WHEN 'favorites' THEN COALESCE(s.favorite, false)
        WHEN 'archive' THEN COALESCE(s.archived, false)
      END
  AND (:device::text IS NULL OR d.folder = :device)
  AND (:kind::text IS NULL OR f.media = :kind)
  AND (:tag::bigint IS NULL OR EXISTS (SELECT 1 FROM file_tags t WHERE t.file_id = f.id AND t.tag_id = :tag))
  AND (:month::text IS NULL
       OR to_char(to_timestamp(COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)) AT TIME ZONE 'UTC', 'YYYY-MM') = :month)
  AND COALESCE(s.lat, m.lat) IS NOT NULL
ORDER BY shown_at DESC LIMIT :limit;

--! assets_by_ids : Asset(width?, height?, duration_ms?, rating?, lat?, lon?)
-- Some files by id, whatever they are, as long as they are not deleted and the user can see them.
SELECT f.id, d.folder, f.path, v.size, v.root_hash, m.width, m.height, m.duration_ms,
       COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) AS shown_at,
       COALESCE(s.favorite, false) AS favorite, COALESCE(s.archived, false) AS archived, s.rating,
       COALESCE(s.lat, m.lat) AS lat, COALESCE(s.lon, m.lon) AS lon
FROM files f
JOIN devices d ON d.id = f.device_id
JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash
LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = :uid
WHERE f.id = ANY(:ids) AND f.deleted_at IS NULL
  AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
ORDER BY shown_at DESC, f.id DESC;

--! album_assets : Asset(width?, height?, duration_ms?, rating?, lat?, lon?)
-- What is in an album: newest first, or by path (which the caller then sorts the way people read names).
SELECT f.id, d.folder, f.path, v.size, v.root_hash, m.width, m.height, m.duration_ms,
       COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) AS shown_at,
       COALESCE(s.favorite, false) AS favorite, COALESCE(s.archived, false) AS archived, s.rating,
       COALESCE(s.lat, m.lat) AS lat, COALESCE(s.lon, m.lon) AS lon
FROM files f
JOIN devices d ON d.id = f.device_id
JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash
LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = :uid
WHERE f.deleted_at IS NULL AND f.media IS NOT NULL
  AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
  AND EXISTS (SELECT 1 FROM albums a WHERE a.id = :album_id
              AND (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
                   OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix))))
ORDER BY CASE WHEN :by_path::boolean THEN f.path END, shown_at DESC, f.id DESC LIMIT :limit;

--! asset_extras : (mime?, camera_when?, camera_lat?, camera_lon?, exif?, codec?)
-- What the detail view shows besides the asset itself: what the camera said before any correction, and whether
-- the user changed the time or the place.
SELECT m.mime, COALESCE(s.description, '') AS description, m.taken_at AS camera_when, m.lat AS camera_lat,
       m.lon AS camera_lon, s.taken_at IS NOT NULL AS when_edited, s.lat IS NOT NULL AS place_edited,
       m.exif, m.codec, v.created_at AS changed
FROM files f
JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash
LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = :uid
WHERE f.id = :id;

--! trash_page
-- Deleted files the user can see, newest deletion first, with the version that would bring each back.
SELECT f.id, d.folder, f.path, v.size, f.deleted_at AS deleted_at, v.id AS version_id, v.root_hash,
       COUNT(*) OVER () AS total
FROM files f
JOIN devices d ON d.id = f.device_id
JOIN versions v ON v.id = f.current_version_id
WHERE f.deleted_at IS NOT NULL AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
ORDER BY f.deleted_at DESC, f.id DESC LIMIT :limit OFFSET :offset;

--! visible_file_exists
SELECT EXISTS (SELECT 1 FROM files f JOIN devices d ON d.id = f.device_id
               WHERE f.id = :id AND f.deleted_at IS NULL
                 AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)) AS visible;

-- What a user says about a file: favorite, archived, rating, description, corrected time and place. One row per
-- user and file, made the first time they say anything.

--! ensure_file_state
INSERT INTO file_state (file_id, user_id, updated_at) VALUES (:file_id, :uid, :now) ON CONFLICT (user_id, file_id) DO NOTHING;

--! set_favorite
UPDATE file_state SET favorite = :value WHERE file_id = :file_id AND user_id = :uid;

--! set_archived
UPDATE file_state SET archived = :value WHERE file_id = :file_id AND user_id = :uid;

--! set_rating (rating?)
UPDATE file_state SET rating = :rating WHERE file_id = :file_id AND user_id = :uid;

--! set_description
UPDATE file_state SET description = :description WHERE file_id = :file_id AND user_id = :uid;

--! set_taken_at (taken_at?)
UPDATE file_state SET taken_at = :taken_at WHERE file_id = :file_id AND user_id = :uid;

--! set_place (lat?, lon?)
UPDATE file_state SET lat = :lat, lon = :lon WHERE file_id = :file_id AND user_id = :uid;

--! touch_file_state
UPDATE file_state SET updated_at = :now WHERE file_id = :file_id AND user_id = :uid;

--! bulk_set_favorite
-- The same favorite for many files at once: those the user can see. Returns how many.
INSERT INTO file_state (file_id, user_id, updated_at, favorite)
SELECT f.id, :uid, :now, :value FROM files f JOIN devices d ON d.id = f.device_id
WHERE f.id = ANY(:ids) AND f.deleted_at IS NULL AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
ON CONFLICT (user_id, file_id) DO UPDATE SET favorite = excluded.favorite, updated_at = excluded.updated_at;

--! bulk_set_archived
INSERT INTO file_state (file_id, user_id, updated_at, archived)
SELECT f.id, :uid, :now, :value FROM files f JOIN devices d ON d.id = f.device_id
WHERE f.id = ANY(:ids) AND f.deleted_at IS NULL AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
ON CONFLICT (user_id, file_id) DO UPDATE SET archived = excluded.archived, updated_at = excluded.updated_at;

--! bulk_add_tags
INSERT INTO file_tags (file_id, tag_id)
SELECT f.id, t FROM files f JOIN devices d ON d.id = f.device_id CROSS JOIN unnest(:tag_ids::bigint[]) t
WHERE f.id = ANY(:ids) AND f.deleted_at IS NULL AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
ON CONFLICT DO NOTHING;

-- Tags belong to the user who made them.

--! upsert_tag
INSERT INTO tags (name, user_id) VALUES (:name, :uid)
ON CONFLICT (user_id, (lower(name))) DO UPDATE SET name = tags.name RETURNING id;

--! tags_of_file
SELECT t.id, t.name FROM file_tags ft JOIN tags t ON t.id = ft.tag_id
WHERE ft.file_id = :file_id AND t.user_id = :uid ORDER BY lower(t.name);

--! keep_only_tags
-- Only this user's tags are replaced: on a master file the others have theirs.
DELETE FROM file_tags WHERE file_id = :file_id AND NOT (tag_id = ANY(:tag_ids))
  AND tag_id IN (SELECT id FROM tags WHERE user_id = :uid);

--! tag_file
INSERT INTO file_tags (file_id, tag_id) SELECT :file_id, unnest(:tag_ids::bigint[]) ON CONFLICT DO NOTHING;

--! drop_unused_tags
DELETE FROM tags t WHERE NOT EXISTS (SELECT 1 FROM file_tags ft WHERE ft.tag_id = t.id);

--! list_tags
-- The user's tags with how many visible files carry each.
SELECT t.id, t.name, COUNT(d.id) AS files FROM tags t
LEFT JOIN file_tags ft ON ft.tag_id = t.id
LEFT JOIN files f ON f.id = ft.file_id AND f.deleted_at IS NULL
LEFT JOIN devices d ON d.id = f.device_id AND (:all::boolean OR d.kind = 'master' OR d.user_id = :scope_user::bigint)
WHERE t.user_id = :uid GROUP BY t.id ORDER BY lower(t.name);

--! delete_tag
DELETE FROM tags WHERE id = :id AND user_id = :uid;

-- What a mirror keeps of the library, so that albums, tags and favorites survive losing the database.

--! snapshot_albums
SELECT a.id, a.name, a.description, a.created_at, u.name AS owner
FROM albums a JOIN users u ON u.id = a.user_id ORDER BY a.id;

--! snapshot_album_files
SELECT af.album_id, d.folder, f.path FROM album_files af
JOIN files f ON f.id = af.file_id JOIN devices d ON d.id = f.device_id ORDER BY af.album_id, f.path;

--! snapshot_tags
SELECT t.id, t.name, u.name AS owner FROM tags t JOIN users u ON u.id = t.user_id ORDER BY t.id;

--! snapshot_tag_files
SELECT ft.tag_id, d.folder, f.path FROM file_tags ft
JOIN files f ON f.id = ft.file_id JOIN devices d ON d.id = f.device_id ORDER BY ft.tag_id, f.path;

--! snapshot_states : (rating?, taken_at?, lat?, lon?)
SELECT d.folder, f.path, s.favorite, s.archived, s.rating, s.description, s.taken_at, s.lat, s.lon, u.name AS owner
FROM file_state s JOIN files f ON f.id = s.file_id JOIN devices d ON d.id = f.device_id JOIN users u ON u.id = s.user_id
WHERE s.favorite OR s.archived OR s.rating IS NOT NULL OR s.description <> '' OR s.taken_at IS NOT NULL OR s.lat IS NOT NULL
ORDER BY d.folder, f.path, u.name;
