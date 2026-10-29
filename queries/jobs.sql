-- The queue of background work: reading metadata and making pictures.
-- A job that worked stays as 'done' for a day (`purge_done_jobs`), so the queue can show what happened lately.
--
-- The queries that list jobs take the same numbers for who is looking as the library ones (see library.sql): a job names
-- its device in the payload, and is visible when that device is.

--! enqueue_job
-- Work that is already waiting or running is not queued a second time.
INSERT INTO jobs (kind, payload, created_at)
SELECT :kind, :payload, :now
WHERE NOT EXISTS (SELECT 1 FROM jobs WHERE kind = :kind AND payload = :payload AND status IN ('queued', 'running'));

--! requeue_running_jobs
-- A restart interrupts whatever was running: it runs again.
UPDATE jobs SET status = 'queued' WHERE status = 'running';

--! claim_job
-- Picks one queued job that is due and marks it running. One statement, so a job is only claimed once.
UPDATE jobs SET status = 'running', attempts = attempts + 1, started_at = :now
WHERE id = (SELECT id FROM jobs
            WHERE status = 'queued' AND run_after <= :now
            ORDER BY seq LIMIT 1 FOR UPDATE SKIP LOCKED)
RETURNING id, kind, payload, attempts;

--! complete_job (note?)
UPDATE jobs SET status = 'done', finished_at = :now, error = NULL, note = :note WHERE id = :id;

--! retry_job
-- A job that failed: queued again after `run_after`, or failed for good.
UPDATE jobs SET status = :status, run_after = :run_after, error = :error, finished_at = :now WHERE id = :id;

--! purge_done_jobs
DELETE FROM jobs WHERE status = 'done' AND finished_at < :before;

--! queue_counts
-- How many jobs there are of each kind in each state (the ones done since `since` count as done).
SELECT j.kind, j.status, COUNT(*) AS n FROM jobs j
WHERE (j.status <> 'done' OR j.finished_at >= :since)
  AND (:all::boolean OR (j.payload::jsonb ->> 'device') IN
       (SELECT d.folder FROM devices d WHERE visible(d, :scope_user::bigint, false)))
GROUP BY j.kind, j.status;

--! queue_items : (started_at?, finished_at?, error?, note?)
-- What is running, failed and waiting, then what was done lately; at most `limit` jobs.
SELECT j.id, j.kind, j.status, j.attempts, j.run_after, j.created_at, j.started_at, j.finished_at, j.error, j.note, j.payload
FROM jobs j
WHERE (j.status <> 'done' OR j.finished_at >= :since)
  AND (:all::boolean OR (j.payload::jsonb ->> 'device') IN
       (SELECT d.folder FROM devices d WHERE visible(d, :scope_user::bigint, false)))
ORDER BY CASE j.status WHEN 'running' THEN 0 WHEN 'failed' THEN 1 WHEN 'queued' THEN 2 ELSE 3 END,
         CASE WHEN j.status = 'done' THEN -j.finished_at ELSE j.seq END
LIMIT :limit;

--! retry_failed_jobs (id?)
-- Failed jobs run again from the start: one, or all of them.
UPDATE jobs SET status = 'queued', attempts = 0, run_after = 0, error = NULL
WHERE status = 'failed' AND (:id::uuid IS NULL OR id = :id);

--! drop_jobs (id?)
-- Jobs of one state (failed, or queued and not started) are forgotten: one, or all of them.
DELETE FROM jobs WHERE status = :status AND (:id::uuid IS NULL OR id = :id);

--! media_to_read
-- The live photos and videos to read again: those never read, videos read without details (before the video module was
-- set up, or when it failed), or all of them. Newest first.
SELECT d.folder, f.path, v.root_hash FROM files f
JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id
LEFT JOIN file_meta m ON m.root_hash = v.root_hash
WHERE f.deleted_at IS NULL AND f.media IS NOT NULL
  AND (:everything::boolean OR m.root_hash IS NULL OR (f.media = 'video' AND m.duration_ms IS NULL))
ORDER BY f.id DESC LIMIT :limit;

--! pending_jobs_of_file
-- The work not finished for a file (queued, running or failed), to tell someone who opens it that it is still being processed.
SELECT j.kind, j.status FROM jobs j
WHERE j.status IN ('queued', 'running', 'failed')
  AND j.payload::jsonb ->> 'device' = :device AND j.payload::jsonb ->> 'path' = :path;
