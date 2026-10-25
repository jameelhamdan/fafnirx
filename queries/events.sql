--! activity_by_day
SELECT e.ts / 86400 AS day, e.action, COUNT(*) AS n
FROM events e JOIN devices d ON d.id = e.device_id
WHERE e.ts >= :since AND (:all::boolean OR d.kind = 'master' OR d.user_id = :user::bigint)
GROUP BY 1, 2;

--! recent_events (folder?) : (detail?)
SELECT e.id, e.ts, e.action, e.path, e.detail, d.folder AS device
FROM events e JOIN devices d ON d.id = e.device_id
WHERE (:folder::text IS NULL OR d.folder = :folder) AND (:all::boolean OR d.kind = 'master' OR d.user_id = :user::bigint)
ORDER BY e.id DESC LIMIT :limit;

--! purge_events
DELETE FROM events WHERE ts < :before;

--! insert_event (hash?, detail?)
INSERT INTO events (device_id, path, action, ts, hash, detail) VALUES (:device_id, :path, :action, :ts, :hash, :detail);

--! file_events : (detail?)
SELECT e.id, e.ts, e.action, e.path, e.detail, d.folder AS device
FROM events e JOIN devices d ON d.id = e.device_id
WHERE e.device_id = :device_id AND e.path = :path ORDER BY e.id DESC LIMIT :limit;
