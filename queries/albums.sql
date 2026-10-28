-- Albums: a user's named set of files, picked by hand and/or following a folder of a device.
--
-- A file is in an album when it was picked into it, or it is below the folder the album shows:
--   EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
--   OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix))
-- The visibility numbers (:all, :scope_user) are the ones explained in library.sql.

--! find_albums (id?) : (chosen?, newest?, folder_device?, folder_prefix?)
-- The albums of a user with what each holds, newest change first; or the one with this id.
SELECT a.id, a.name, a.description, a.created_at, a.updated_at,
       (SELECT COUNT(*) FROM files f
        WHERE (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
               OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))
          AND f.deleted_at IS NULL AND f.media IS NOT NULL) AS files,
       (SELECT COALESCE(SUM(v.size), 0)::BIGINT FROM files f JOIN versions v ON v.id = f.current_version_id
        WHERE (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
               OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))
          AND f.deleted_at IS NULL AND f.media IS NOT NULL) AS bytes,
       -- The cover is chosen in code (`library::albums::with_cover`): the file picked, else a poster picture (`album_poster`),
       -- else for an album of a folder its first video (names sort "naturally" there, see `media_of_album`), else the newest
       -- file of an album of picked files.
       (SELECT f.id FROM files f WHERE f.id = a.cover_file_id AND f.deleted_at IS NULL) AS chosen,
       CASE WHEN a.folder_device_id IS NULL THEN
                (SELECT f.id FROM files f JOIN versions v ON v.id = f.current_version_id
                 LEFT JOIN file_meta m ON m.root_hash = v.root_hash
                 LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = a.user_id
                 WHERE (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
                        OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))
                   AND f.deleted_at IS NULL AND f.media IS NOT NULL
                 ORDER BY COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) DESC, f.id DESC LIMIT 1)
       END AS newest,
       (SELECT d.folder FROM devices d WHERE d.id = a.folder_device_id) AS folder_device,
       a.folder_prefix
FROM albums a
WHERE a.user_id = :user_id AND (:id::uuid IS NULL OR a.id = :id)
ORDER BY a.updated_at DESC, a.id DESC;

--! media_of_album
-- The live photos and videos of an album as (id, path), to choose its cover among.
SELECT f.id, f.path FROM files f JOIN albums a ON a.id = :album_id
WHERE (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
       OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))
  AND f.deleted_at IS NULL AND f.media IS NOT NULL;

--! album_of_folder
SELECT id FROM albums WHERE user_id = :user_id AND folder_device_id = :device_id AND folder_prefix = :prefix;

--! insert_album (folder_device_id?, folder_prefix?)
INSERT INTO albums (name, description, created_at, updated_at, user_id, folder_device_id, folder_prefix)
VALUES (:name, :description, :now, :now, :user_id, :folder_device_id, :folder_prefix) RETURNING id;

--! add_files_to_album
-- Puts files in the album: those the user can see. Returns how many were added.
INSERT INTO album_files (album_id, file_id, added_at)
SELECT :album_id, f.id, :now FROM files f JOIN devices d ON d.id = f.device_id
WHERE f.id = ANY(:ids) AND f.deleted_at IS NULL AND visible(d, :scope_user::bigint, :all::boolean)
ON CONFLICT DO NOTHING;

--! add_place_to_album
-- Puts the photos and videos at a place in the album: a file, or everything below a folder, of a device the user
-- can see. `path` is empty for the whole device.
INSERT INTO album_files (album_id, file_id, added_at)
SELECT :album_id, f.id, :now FROM files f JOIN devices d ON d.id = f.device_id
WHERE d.folder = :device AND f.deleted_at IS NULL AND f.media IS NOT NULL
  AND (:path = '' OR f.path = :path OR starts_with(f.path, :path || '/'))
  AND visible(d, :scope_user::bigint, :all::boolean)
ON CONFLICT DO NOTHING;

--! rename_album
UPDATE albums SET name = :name WHERE id = :id;

--! set_album_description
UPDATE albums SET description = :description WHERE id = :id;

--! file_is_in_album
SELECT EXISTS (SELECT 1 FROM albums a, files f WHERE a.id = :album_id AND f.id = :file_id
               AND (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
                    OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))) AS member;

--! set_album_cover (cover?)
UPDATE albums SET cover_file_id = :cover WHERE id = :id;

--! touch_album
UPDATE albums SET updated_at = :now WHERE id = :id;

--! delete_album
DELETE FROM albums WHERE id = :id AND user_id = :user_id;

--! remove_files_from_album
DELETE FROM album_files WHERE album_id = :album_id AND file_id = ANY(:ids);

--! clear_cover_among
UPDATE albums SET cover_file_id = NULL WHERE id = :album_id AND cover_file_id = ANY(:ids);

--! albums_of_file
SELECT a.id, a.name FROM album_files af JOIN albums a ON a.id = af.album_id
WHERE af.file_id = :file_id AND a.user_id = :uid ORDER BY lower(a.name);

--! albums_under_folder : (folder_prefix?)
-- The albums, whoever's, that follow this folder of a device or a folder below it. The whole device (an empty prefix) is
-- not a folder that can move, so it is left out.
SELECT id, user_id, name, folder_prefix FROM albums
WHERE folder_device_id = :device_id AND folder_prefix <> '' AND starts_with(folder_prefix, :prefix);

--! folder_is_empty
-- Whether no live file of the device is below this folder (`prefix` ends with a slash).
SELECT NOT EXISTS (SELECT 1 FROM files WHERE device_id = :device_id AND deleted_at IS NULL AND starts_with(path, :prefix)) AS empty;

--! move_album_folder
-- The folder of the album moved: it follows it, and keeps the name of the folder when it had that.
UPDATE albums SET folder_prefix = :prefix, name = :name, updated_at = :now WHERE id = :id;

--! album_poster
-- The picture that stands for an album: a file named poster, cover, folder, thumb or thumbnail (.jpg, .jpeg, .png, .webp)
-- in it, the one nearest to the top of its folders first, then by that order of names.
SELECT f.id FROM files f JOIN albums a ON a.id = :album_id
WHERE (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id)
       OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))
  AND f.deleted_at IS NULL AND f.media = 'image'
  AND lower(regexp_replace(f.path, '^.*/', '')) ~ '^(poster|cover|folder|thumb|thumbnail)\.(jpg|jpeg|png|webp)$'
ORDER BY length(f.path) - length(replace(f.path, '/', '')),
         array_position(ARRAY['poster', 'cover', 'folder', 'thumb', 'thumbnail'],
                        lower(regexp_replace(regexp_replace(f.path, '^.*/', ''), '\.[^.]*$', ''))),
         f.path
LIMIT 1;
