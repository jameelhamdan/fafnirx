-- 0004: a mirror can name a remote of the rclone config file ("name:path") instead of spelling out a backend and its
-- options. Empty for the mirrors that do not.
ALTER TABLE mirrors ADD COLUMN remote TEXT NOT NULL DEFAULT '';
