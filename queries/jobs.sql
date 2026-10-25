--! enqueue_job
INSERT INTO jobs (kind, payload, created_at) VALUES (:kind, :payload, :now);

--! requeue_running_jobs
-- A restart interrupts whatever was running: it runs again.
UPDATE jobs SET status = 'queued' WHERE status = 'running';

--! claim_job
-- Picks one queued job that is due and marks it running. One statement, so a job is only claimed once. A lane
-- without `with_proxies` skips the slow video conversions, and proxies come after the other jobs.
UPDATE jobs SET status = 'running', attempts = attempts + 1
WHERE id = (SELECT id FROM jobs
            WHERE status = 'queued' AND run_after <= :now AND (:with_proxies::boolean OR kind <> 'proxy')
            ORDER BY (kind = 'proxy'), id LIMIT 1 FOR UPDATE SKIP LOCKED)
RETURNING id, kind, payload, attempts;

--! complete_job
DELETE FROM jobs WHERE id = :id;

--! retry_job
-- A job that failed: queued again after `run_after`, or failed for good.
UPDATE jobs SET status = :status, run_after = :run_after, error = :error WHERE id = :id;
