--! enqueue_job
INSERT INTO jobs (kind, payload, created_at) VALUES (:kind, :payload, :now);
