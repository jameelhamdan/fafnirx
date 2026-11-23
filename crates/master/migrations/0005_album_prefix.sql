-- An album shows a folder, or a folder inside one (`folder_prefix`, without slashes at the ends; empty for the whole folder):
-- a season of a show can be an album of its own.

ALTER TABLE albums ADD COLUMN folder_prefix TEXT NOT NULL DEFAULT '';
DROP INDEX albums_folder;
CREATE UNIQUE INDEX albums_folder ON albums (user_id, folder_id, folder_prefix);
