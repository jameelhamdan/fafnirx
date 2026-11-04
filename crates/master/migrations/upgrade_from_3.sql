-- The one-time step from a database made by a 3.x release (schema version 19) to the schema in 0001_schema.sql: what its
-- unreleased migrations 20 to 24 did (a folder id that never changes, albums that are folders only, sources that write into
-- any folder, master folders, one kind of folder), then the rename of the table a computer's folder state is kept in.
-- Run by platform/migrations.rs on such a database, once. Delete it, and its code there, when no such database is left.

-- 0020: a folder can be renamed. The name a client or the web app uses can change, so each device gets an id that never does
-- (clients keep it and ask the master for the folder by it), and the folder (`<user>/<name>`) is made again when the name changes.
-- Moving the files on disk and rewriting what is kept under the old name is done by the master (`devices::rename`).

ALTER TABLE devices ADD COLUMN public_id UUID NOT NULL DEFAULT gen_random_uuid();
CREATE UNIQUE INDEX devices_public_id ON devices (public_id);

DROP TRIGGER devices_name_fixed ON devices;
DROP FUNCTION devices_name_is_fixed();
CREATE TRIGGER devices_folder_renamed BEFORE UPDATE OF name ON devices
    FOR EACH ROW EXECUTE FUNCTION devices_set_folder();

-- 0021: an album is a shortcut to a folder and nothing more. Albums of picked files are deleted (the photos stay where they
-- are; an album is made again from the folder), and so is the table that held the picks.

DELETE FROM albums WHERE folder_device_id IS NULL;
DROP TABLE album_files;
ALTER TABLE albums DROP CONSTRAINT albums_folder_both;
ALTER TABLE albums ALTER COLUMN folder_device_id SET NOT NULL;
ALTER TABLE albums ALTER COLUMN folder_prefix SET NOT NULL;
DROP INDEX albums_folder;
CREATE UNIQUE INDEX albums_folder ON albums (user_id, folder_device_id, folder_prefix);

-- 0022: a source writes into a folder of its owner's choosing (their own, a shared one, or one in the master files), at any
-- subfolder of it, and is named by its owner and name rather than by a hidden device. Existing sources keep writing where they
-- did: their device becomes an ordinary local folder, which can be renamed like any other.

ALTER TABLE sources ADD COLUMN user_id BIGINT REFERENCES users(id);
UPDATE sources s SET user_id = d.user_id FROM devices d WHERE d.id = s.device_id;
ALTER TABLE sources ALTER COLUMN user_id SET NOT NULL;
-- What the files of the source start with below its folder: empty for the folder itself, else the path and a slash.
ALTER TABLE sources ADD COLUMN folder_prefix TEXT NOT NULL DEFAULT '';
CREATE UNIQUE INDEX sources_user_name ON sources (user_id, name);

UPDATE devices SET kind = 'local' WHERE kind = 'source';

-- 0023: a master folder is a named subfolder of the master files that computers can sync both ways. The master files as a whole
-- are never one. Its name is its path; `public_id` is the stable address clients keep.
CREATE TABLE master_folders (
    id         BIGSERIAL PRIMARY KEY,
    public_id  UUID NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    prefix     TEXT NOT NULL UNIQUE CHECK (prefix <> '' AND prefix !~ '^/|/$|//'),
    created_at BIGINT NOT NULL
);

-- 0024: a folder is a folder. A shared folder was a folder with members, a local folder one without; both are a `local` device
-- now, and sharing is the rows of `shared_members` (any folder of a user can have them). A source is no kind of device any more
-- (0022 made its folder a local one), so neither is left in the list.
UPDATE devices SET kind = 'local' WHERE kind IN ('shared', 'source');
ALTER TABLE devices DROP CONSTRAINT devices_kind;
ALTER TABLE devices ADD CONSTRAINT devices_kind CHECK (kind IN ('device', 'local', 'master'));

ALTER TABLE device_shared_state RENAME TO device_folder_state;
ALTER TABLE device_folder_state RENAME COLUMN shared TO folder;
ALTER TABLE device_folder_state RENAME CONSTRAINT device_shared_state_device_id_fkey TO device_folder_state_device_id_fkey;
ALTER TABLE device_folder_state RENAME CONSTRAINT device_shared_state_pkey TO device_folder_state_pkey;
ALTER INDEX device_shared_state_folder RENAME TO device_folder_state_folder;
