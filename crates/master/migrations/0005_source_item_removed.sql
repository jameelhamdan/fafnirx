-- 0005: a file of a source that someone deleted in the web app stays deleted, even after the retention period has
-- removed its row from `files`, until it changes at the source. Set when it is deleted, cleared when it is fetched again.
ALTER TABLE source_items ADD COLUMN removed_in_web_app BOOLEAN NOT NULL DEFAULT FALSE;
