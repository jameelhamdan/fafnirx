-- Master folders become folders like any other: a row of `devices` (kind `master`, no owner) whose name is its path in the master
-- files and whose `folder` is `master:<name>`. The master files as a whole (device 0) are gone: every master file is in a master
-- folder, as every other file is in a folder of a user or of a computer.

CREATE OR REPLACE FUNCTION devices_set_folder() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.kind = 'master' THEN
        NEW.folder := 'master:' || NEW.name;
    ELSE
        NEW.folder := (SELECT name FROM users WHERE id = NEW.user_id) || '/' || NEW.name;
    END IF;
    RETURN NEW;
END $$;

-- The master folders that were made, with the address computers keep for them.
INSERT INTO devices (name, kind, token_hash, created_at, public_id)
SELECT prefix, 'master', '!', created_at, public_id FROM master_folders;

-- A top-level folder of the master files that was not one yet becomes one, unless that would put it inside another or around one.
INSERT INTO devices (name, kind, token_hash, created_at)
SELECT DISTINCT split_part(f.path, '/', 1), 'master', '!', EXTRACT(EPOCH FROM now())::BIGINT
FROM files f
WHERE f.device_id = 0 AND position('/' IN f.path) > 0 AND lower(split_part(f.path, '/', 1)) <> 'users'
  AND NOT EXISTS (SELECT 1 FROM master_folders m WHERE m.prefix = split_part(f.path, '/', 1) OR starts_with(m.prefix, split_part(f.path, '/', 1) || '/')
                                                    OR starts_with(split_part(f.path, '/', 1), m.prefix || '/'));

-- Everything that was a path in the master files is a path in the master folder it is in, without the folder's own path.
UPDATE files f SET device_id = d.id, path = substr(f.path, length(d.name) + 2)
FROM devices d WHERE f.device_id = 0 AND d.kind = 'master' AND d.id <> 0 AND starts_with(f.path, d.name || '/');

UPDATE events e SET device_id = d.id, path = substr(e.path, length(d.name) + 2),
                    detail = CASE WHEN e.action = 'rename' AND starts_with(e.detail, d.name || '/') THEN substr(e.detail, length(d.name) + 2) ELSE e.detail END
FROM devices d WHERE e.device_id = 0 AND d.kind = 'master' AND d.id <> 0 AND starts_with(e.path, d.name || '/');

UPDATE mirror_items i SET device_id = d.id, path = substr(i.path, length(d.name) + 2)
FROM devices d WHERE i.device_id = 0 AND d.kind = 'master' AND d.id <> 0 AND starts_with(i.path, d.name || '/');

UPDATE albums a SET folder_device_id = d.id,
                    folder_prefix = CASE WHEN a.folder_prefix = d.name THEN '' ELSE substr(a.folder_prefix, length(d.name) + 2) END
FROM devices d WHERE a.folder_device_id = 0 AND d.kind = 'master' AND d.id <> 0
  AND (a.folder_prefix = d.name OR starts_with(a.folder_prefix, d.name || '/'));

UPDATE sources s SET device_id = d.id,
                     folder_prefix = CASE WHEN s.folder_prefix = d.name THEN '' ELSE substr(s.folder_prefix, length(d.name) + 2) END
FROM devices d WHERE s.device_id = 0 AND d.kind = 'master' AND d.id <> 0
  AND (s.folder_prefix = d.name OR starts_with(s.folder_prefix, d.name || '/'));

-- A mirror's scope names a place in the live tree: the master folders are `master:<name>` now, and the whole master `master:`.
UPDATE mirrors SET scope = 'master:' || substr(scope, 9) WHERE scope = '_master' OR starts_with(scope, '_master/');

-- What was not in a master folder (files directly in the master files, or in a folder that would have overlapped one) is no
-- longer recorded: it stays on disk, and the layout alert says so until it is put in a folder.
DELETE FROM albums WHERE folder_device_id = 0;
DELETE FROM sources WHERE device_id = 0;
DELETE FROM versions WHERE file_id IN (SELECT id FROM files WHERE device_id = 0);
DELETE FROM files WHERE device_id = 0;
DELETE FROM events WHERE device_id = 0;
DELETE FROM mirror_items WHERE device_id = 0;
DELETE FROM devices WHERE id = 0;

-- The lookup of the picture next to a video was a job that stored nothing; it is made when asked for.
DELETE FROM jobs WHERE kind = 'artwork';

-- An album is a shortcut to a whole folder, not to a place inside one.
DELETE FROM albums WHERE folder_prefix <> '';
DROP INDEX albums_folder;
ALTER TABLE albums DROP COLUMN folder_prefix;
CREATE UNIQUE INDEX albums_folder ON albums (user_id, folder_device_id);

-- Sources (inbound) and mirrors (outbound) are one thing: a link between a folder and some storage that runs on a schedule. A source
-- writes into one whole folder of its owner's; a mirror holds one whole folder, or everything (no folder). What the storage is
-- (rclone's remote, type, path, options; a mirror's passwords and runs to keep) is the `config` text, as a source's always was.
-- A mirror that held less than a folder (a user, or all master folders) is dropped; one that held a part of a folder holds it all.
DELETE FROM mirrors m WHERE m.scope <> '' AND NOT EXISTS
    (SELECT 1 FROM devices d WHERE d.folder = m.scope OR starts_with(m.scope, d.folder || '/'));
UPDATE source_items i SET path = s.folder_prefix || i.path FROM sources s WHERE s.id = i.source_id AND s.folder_prefix <> '';

CREATE TABLE links (
    id            BIGINT GENERATED BY DEFAULT AS IDENTITY PRIMARY KEY,
    direction     TEXT NOT NULL CHECK (direction IN ('in', 'out')),
    user_id       BIGINT REFERENCES users(id),
    name          TEXT NOT NULL CHECK (name ~ '^[a-z0-9][a-z0-9._-]{0,62}$'),
    kind          TEXT NOT NULL,
    config        TEXT NOT NULL,
    folder_id     BIGINT REFERENCES devices(id),
    interval_secs BIGINT NOT NULL,
    enabled       BIGINT NOT NULL DEFAULT 1,
    run_requested BIGINT NOT NULL DEFAULT 0,
    last_run_at   BIGINT,
    last_ok_at    BIGINT,
    last_status   TEXT,
    last_message  TEXT,
    last_files    BIGINT,
    last_bytes    BIGINT,
    created_at    BIGINT NOT NULL,
    CONSTRAINT links_owner CHECK ((direction = 'in') = (user_id IS NOT NULL AND folder_id IS NOT NULL))
);
CREATE UNIQUE INDEX links_in_name ON links (user_id, name) WHERE direction = 'in';
CREATE UNIQUE INDEX links_out_name ON links (name) WHERE direction = 'out';

INSERT INTO links
    (id, direction, user_id, name, kind, config, folder_id, interval_secs, enabled, run_requested, last_run_at, last_ok_at, last_status, last_message, created_at)
OVERRIDING SYSTEM VALUE
SELECT id, 'in', user_id, name, kind, config, device_id, interval_secs, enabled, run_requested, last_run_at, last_ok_at, last_status, last_message, created_at
FROM sources;
SELECT setval(pg_get_serial_sequence('links', 'id'), COALESCE(MAX(id), 0) + 1, false) FROM links;
ALTER TABLE mirrors ADD COLUMN link_id BIGINT;
WITH made AS (
    INSERT INTO links (direction, name, kind, config, folder_id, interval_secs, enabled, run_requested, last_run_at, last_ok_at, last_status, last_message, last_files, last_bytes, created_at)
    SELECT 'out', m.name, m.kind,
           jsonb_build_object('remote', m.remote, 'remote_type', m.remote_type, 'remote_path', m.remote_path, 'options', m.options,
                              'crypt_password', m.crypt_password, 'crypt_salt', m.crypt_salt, 'keep_runs', m.keep_runs)::text,
           (SELECT d.id FROM devices d WHERE m.scope <> '' AND (d.folder = m.scope OR starts_with(m.scope, d.folder || '/')) ORDER BY length(d.folder) DESC LIMIT 1),
           m.interval_secs, m.enabled, m.run_requested, m.last_run_at, m.last_ok_at, m.last_status, m.last_message, m.last_files, m.last_bytes, m.created_at
    FROM mirrors m ORDER BY m.id
    RETURNING id, name
)
UPDATE mirrors m SET link_id = made.id FROM made WHERE made.name = m.name;

-- What a link has copied: a source's files by the remote's id for them, a mirror's by the hash of what it holds (hex).
CREATE TABLE link_items (
    link_id            BIGINT NOT NULL REFERENCES links(id) ON DELETE CASCADE,
    device_id          BIGINT NOT NULL,
    path               TEXT NOT NULL,
    remote_id          TEXT NOT NULL DEFAULT '',
    version            TEXT NOT NULL,
    removed_in_web_app BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (link_id, device_id, path)
);
CREATE UNIQUE INDEX link_items_remote ON link_items (link_id, remote_id) WHERE remote_id <> '';
INSERT INTO link_items (link_id, device_id, path, remote_id, version, removed_in_web_app)
SELECT i.source_id, s.device_id, i.path, i.remote_id, i.remote_version, i.removed_in_web_app FROM source_items i JOIN sources s ON s.id = i.source_id;
INSERT INTO link_items (link_id, device_id, path, version)
SELECT m.link_id, i.device_id, i.path, encode(i.root_hash, 'hex') FROM mirror_items i JOIN mirrors m ON m.id = i.mirror_id;
DROP TABLE source_items, mirror_items, sources, mirrors;

DROP TABLE master_folders;
ALTER TABLE devices DROP COLUMN totals_at;
