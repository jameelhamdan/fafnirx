-- Runs of sources and mirrors are rows of `jobs` (kinds `source` and `mirror`), tied to their link; a computer reports its conflict mode.

ALTER TABLE jobs ADD COLUMN link_id BIGINT REFERENCES links (id) ON DELETE CASCADE;
CREATE INDEX jobs_link ON jobs (link_id) WHERE link_id IS NOT NULL;

DROP INDEX jobs_queue;
CREATE INDEX jobs_queue ON jobs (seq) WHERE status = 'queued';

ALTER TABLE device_folder_state ADD COLUMN mode TEXT NOT NULL DEFAULT '';
