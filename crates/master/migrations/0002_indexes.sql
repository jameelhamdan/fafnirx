-- 0002: the indexes behind every lookup, listing and search of the web API and the device API.

-- Substring search of the file browser (ILIKE '%text%'). The extension lives in the public schema, so every
-- schema of the database finds its operator class there.
CREATE EXTENSION IF NOT EXISTS pg_trgm SCHEMA public;
CREATE INDEX files_path_search ON files USING gin (path public.gin_trgm_ops);

-- Folder browsing and syncing a folder: path LIKE 'dir/%' within one device. The unique (device_id, path) index
-- answers equality; this one answers prefixes, which a btree in a locale other than C cannot.
CREATE INDEX files_path_prefix ON files (device_id, path text_pattern_ops);

-- Which files hold a version as their current one: every Negotiate and Commit of a device looks up content by hash.
CREATE INDEX files_current_version ON files (current_version_id);

-- The Trash page (newest deletion first) and retention (deleted before a day).
CREATE INDEX files_deleted ON files (deleted_at DESC, id DESC) WHERE deleted_at IS NOT NULL;

-- Retention of versions that were pruned.
CREATE INDEX versions_pruned ON versions (pruned_at) WHERE pruned_at IS NOT NULL;

-- The photo library: whether a file is a photo or a video comes from its name, kept in a column so that the
-- timeline, the map, the albums and the type filters do not run a pattern over every path. The extensions are
-- those of library/meta.rs (a test keeps them equal).
ALTER TABLE files ADD COLUMN media TEXT GENERATED ALWAYS AS (
    CASE
        WHEN lower(path) ~ '\.(jpg|jpeg|png|gif|webp|bmp|tif|tiff|heic|heif|avif)$' THEN 'image'
        WHEN lower(path) ~ '\.(mp4|mov|mkv|avi|webm|m4v|wmv|mts|3gp|mpg|mpeg)$' THEN 'video'
    END
) STORED;
CREATE INDEX files_library ON files (id) WHERE deleted_at IS NULL AND media IS NOT NULL;

-- The Favorites and Archive pages, and the map.
CREATE INDEX file_state_favorite ON file_state (file_id) WHERE favorite;
CREATE INDEX file_state_archived ON file_state (file_id) WHERE archived;
CREATE INDEX file_state_placed ON file_state (file_id) WHERE lat IS NOT NULL;
CREATE INDEX file_meta_placed ON file_meta (root_hash) WHERE lat IS NOT NULL;

-- The change log: the history of one file, and the activity of the last days (and its retention).
CREATE INDEX events_path ON events (device_id, path, id DESC);
CREATE INDEX events_ts ON events (ts);

-- A device is found by its token on every call it makes.
CREATE UNIQUE INDEX devices_token ON devices (token_hash) WHERE kind = 'device';

-- The next job to run, in the order the workers claim them (photos and videos before the slow copies).
CREATE INDEX jobs_queue ON jobs ((kind = 'proxy'), id) WHERE status = 'queued';

-- Resolved alerts are only kept for a while (see monitoring/jobs.rs).
CREATE INDEX alerts_resolved ON alerts (resolved_at) WHERE resolved_at IS NOT NULL;
