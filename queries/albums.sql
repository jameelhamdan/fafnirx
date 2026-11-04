-- Albums: a user's shortcut to a folder of a device (or to the whole device).
--
-- A file is in an album when it is below the folder the album shows:
--   a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)
-- The visibility numbers (:all, :scope_user) are the ones explained in library.sql.

--! find_albums (id?) : (chosen?)
-- The albums of a user with what each holds, newest change first; or the one with this id.
SELECT a.id, a.name, a.description, a.created_at, a.updated_at,
       (SELECT COUNT(*) FROM files f
        WHERE a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)
          AND f.deleted_at IS NULL AND f.media IS NOT NULL) AS files,
       (SELECT COALESCE(SUM(v.size), 0)::BIGINT FROM files f JOIN versions v ON v.id = f.current_version_id
        WHERE a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)
          AND f.deleted_at IS NULL AND f.media IS NOT NULL) AS bytes,
       -- The cover is chosen in code (`library::albums::with_cover`): the file picked, else a poster picture (`album_poster`),
       -- else its first video (names sort "naturally" there, see `media_of_album`), else its first picture.
       (SELECT f.id FROM files f WHERE f.id = a.cover_file_id AND f.deleted_at IS NULL) AS chosen,
       (SELECT d.folder FROM devices d WHERE d.id = a.folder_device_id) AS folder_device,
       a.folder_prefix
FROM albums a
WHERE a.user_id = :user_id AND (:id::uuid IS NULL OR a.id = :id)
ORDER BY a.updated_at DESC, a.id DESC;

--! media_of_album
-- The live photos and videos of an album as (id, path), to choose its cover among.
SELECT f.id, f.path FROM files f JOIN albums a ON a.id = :album_id
WHERE a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)
  AND f.deleted_at IS NULL AND f.media IS NOT NULL;

--! album_of_folder
SELECT id FROM albums WHERE user_id = :user_id AND folder_device_id = :device_id AND folder_prefix = :prefix;

--! insert_album
INSERT INTO albums (name, description, created_at, updated_at, user_id, folder_device_id, folder_prefix)
VALUES (:name, :description, :now, :now, :user_id, :folder_device_id, :folder_prefix) RETURNING id;

--! rename_album
UPDATE albums SET name = :name WHERE id = :id;

--! set_album_description
UPDATE albums SET description = :description WHERE id = :id;

--! file_is_in_album
SELECT EXISTS (SELECT 1 FROM albums a, files f WHERE a.id = :album_id AND f.id = :file_id
               AND a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)) AS member;

--! set_album_cover (cover?)
UPDATE albums SET cover_file_id = :cover WHERE id = :id;

--! touch_album
UPDATE albums SET updated_at = :now WHERE id = :id;

--! delete_album
DELETE FROM albums WHERE id = :id AND user_id = :user_id;

--! albums_of_file
SELECT a.id, a.name FROM albums a JOIN files f ON f.id = :file_id
WHERE a.user_id = :uid AND a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)
ORDER BY lower(a.name);

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
WHERE a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)
  AND f.deleted_at IS NULL AND f.media = 'image'
  AND lower(regexp_replace(f.path, '^.*/', '')) ~ '^(poster|cover|folder|thumb|thumbnail)\.(jpg|jpeg|png|webp)$'
ORDER BY length(f.path) - length(replace(f.path, '/', '')),
         array_position(ARRAY['poster', 'cover', 'folder', 'thumb', 'thumbnail'],
                        lower(regexp_replace(regexp_replace(f.path, '^.*/', ''), '\.[^.]*$', ''))),
         f.path
LIMIT 1;
