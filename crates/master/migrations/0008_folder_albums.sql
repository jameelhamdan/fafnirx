-- 0008: an album can show a folder of a device (and everything below it), as it is now: new episodes of a show appear in
-- the album by themselves. `folder_prefix` is the folder's path with a slash at the end ('' for the whole device), so that
-- a file is in the folder when its path starts with it. Files picked one by one (album_files) still count besides.
ALTER TABLE albums ADD COLUMN folder_device_id BIGINT REFERENCES devices(id) ON DELETE CASCADE;
ALTER TABLE albums ADD COLUMN folder_prefix TEXT;
ALTER TABLE albums ADD CONSTRAINT albums_folder_both CHECK ((folder_device_id IS NULL) = (folder_prefix IS NULL));
-- A user has one album per folder.
CREATE UNIQUE INDEX albums_folder ON albums (user_id, folder_device_id, folder_prefix) WHERE folder_device_id IS NOT NULL;
