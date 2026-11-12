--! activity_by_day
SELECT e.ts / 86400 AS day, e.action, COUNT(*) AS n
FROM events e JOIN folders fo ON fo.id = e.folder_id
WHERE e.ts >= :since AND visible(fo, :user::bigint, :all::boolean)
GROUP BY 1, 2;

--! recent_events (folder?) : (detail?)
SELECT e.id, e.ts, e.action, e.path, e.detail, fo.key AS folder
FROM events e JOIN folders fo ON fo.id = e.folder_id
WHERE (:folder::text IS NULL OR fo.key = :folder OR starts_with(fo.key, :folder || '/')) AND visible(fo, :user::bigint, :all::boolean)
ORDER BY e.id DESC LIMIT :limit;

--! purge_events
DELETE FROM events WHERE ts < :before;

--! insert_event (hash?, detail?)
INSERT INTO events (folder_id, path, action, ts, hash, detail) VALUES (:folder_id, :path, :action, :ts, :hash, :detail);

--! file_events : (detail?)
SELECT e.id, e.ts, e.action, e.path, e.detail, fo.key AS folder
FROM events e JOIN folders fo ON fo.id = e.folder_id
WHERE e.folder_id = :folder_id AND e.path = :path ORDER BY e.id DESC LIMIT :limit;
