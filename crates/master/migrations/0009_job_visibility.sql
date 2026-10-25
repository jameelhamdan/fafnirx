-- 0009: the queue of background work can be looked at. A job that worked is kept as 'done' for a day (it used to be deleted
-- at once) so that "what happened lately" has an answer, and every job knows when it was started and when it ended.
ALTER TABLE jobs ADD COLUMN started_at BIGINT;
ALTER TABLE jobs ADD COLUMN finished_at BIGINT;
CREATE INDEX jobs_status ON jobs (status, kind);
