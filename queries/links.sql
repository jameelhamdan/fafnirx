-- Sources and mirrors: links between a folder and some storage that run on a schedule (the table `links`, direction `in` for a
-- source, `out` for a mirror). What they have in common is asked here, once; the rest is in `sources.sql` and `mirrors.sql`.

--! add_link (user_id?, folder_id?, device_id?)
INSERT INTO links (direction, user_id, name, kind, config, folder_id, device_id, interval_secs, created_at)
VALUES (:direction, :user_id, :name, :kind, :config, :folder_id, :device_id, :interval_secs, :now)
ON CONFLICT DO NOTHING RETURNING id;

--! link_id_by_name (owner?)
-- A source is named by its owner and its name, a mirror by its name.
SELECT l.id FROM links l LEFT JOIN users u ON u.id = l.user_id
WHERE l.direction = :direction AND l.name = :name AND (:owner::text IS NULL OR u.name = :owner::text);

--! link_name_taken (user_id?)
SELECT EXISTS (SELECT 1 FROM links WHERE direction = :direction AND name = :name AND user_id IS NOT DISTINCT FROM :user_id::bigint AND id <> :except) AS taken;

--! link_for_edit : (last_status?, user_id?, folder_id?, device_id?)
SELECT id, name, kind, config, folder_id, device_id, interval_secs, user_id, last_status FROM links WHERE id = :id FOR UPDATE;

--! update_link (folder_id?, device_id?)
UPDATE links SET name = :name, config = :config, folder_id = :folder_id, device_id = :device_id, interval_secs = :interval_secs WHERE id = :id;

--! delete_link
DELETE FROM links WHERE id = :id;

--! request_run
UPDATE links SET run_requested = 1 WHERE id = :id;

--! continue_run
-- More is waiting than one run does: another run follows at once, unless it was switched off meanwhile.
UPDATE links SET run_requested = 1 WHERE id = :id AND enabled = 1;

--! toggle_link
UPDATE links SET enabled = 1 - enabled WHERE id = :id;

--! schedule_link
-- Queues the next run that is due as a job: one that was asked for, or whose interval has passed (a switched off one only runs when
-- asked for), that is not running and has no run waiting. The job names the link; its old, finished run is forgotten.
WITH due AS (
    SELECT id FROM links
    WHERE (run_requested = 1 OR (enabled = 1 AND (last_run_at IS NULL OR last_run_at + interval_secs <= :now)))
      AND last_status IS DISTINCT FROM 'running'
      AND NOT EXISTS (SELECT 1 FROM jobs j WHERE j.link_id = links.id AND j.status IN ('queued', 'running'))
    ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED
), reset AS (
    UPDATE links SET run_requested = 0 FROM due WHERE links.id = due.id RETURNING links.id, links.direction
), forgotten AS (
    DELETE FROM jobs WHERE link_id IN (SELECT id FROM reset) AND status IN ('done', 'failed')
)
INSERT INTO jobs (kind, payload, link_id, created_at)
SELECT CASE WHEN direction = 'in' THEN 'source' ELSE 'mirror' END, json_build_object('link', id)::text, id, :now FROM reset
RETURNING link_id;

--! start_link
-- Its run began: it is running from now on.
UPDATE links SET last_status = 'running', last_run_at = :now, last_message = '' WHERE id = :id;

--! link_to_run : (folder_id?, device_id?, owner?)
-- A link that is due, with the folder or device it is about and the owner, which with the name tell it apart in logs, alerts and live events.
SELECT l.id, l.direction, l.kind, l.name, l.config, l.folder_id, l.device_id, COALESCE(fo.key, de.key, '') AS scope, u.name AS owner
FROM links l LEFT JOIN folders fo ON fo.id = l.folder_id LEFT JOIN devices de ON de.id = l.device_id LEFT JOIN users u ON u.id = l.user_id WHERE l.id = :id;

--! finish_link (ok_at?)
UPDATE links SET last_status = :status, last_message = :message, last_ok_at = COALESCE(:ok_at, last_ok_at) WHERE id = :id;

--! set_link_totals
UPDATE links SET last_files = :files, last_bytes = :bytes WHERE id = :id;

--! fail_interrupted_links
-- A restart interrupts whatever was running: it is shown as failed until it runs again.
UPDATE links SET last_status = 'failed', last_message = 'interrupted by a restart' WHERE last_status = 'running';

--! list_links : (last_run_at?, last_ok_at?, last_status?, last_message?, files?, bytes?)
-- The links of one direction a user may see (all of them for an administrator). A source's `files` and `bytes` are what it collected
-- and is still there, not what the folder holds; a mirror's are what its last run left on the storage.
SELECT l.name, l.kind, COALESCE(u.name, '') AS owner, l.config, COALESCE(fo.key, de.key, '') AS scope, l.interval_secs, l.enabled,
       l.last_run_at, l.last_ok_at, l.last_status, l.last_message,
       CASE WHEN l.direction = 'out' THEN l.last_files ELSE
           (SELECT COUNT(*) FROM link_items i JOIN files f ON f.folder_id = i.folder_id AND f.path = i.path AND f.deleted_at IS NULL
            WHERE i.link_id = l.id) END AS files,
       CASE WHEN l.direction = 'out' THEN l.last_bytes ELSE
           (SELECT COALESCE(SUM(v.size), 0)::BIGINT FROM link_items i JOIN files f ON f.folder_id = i.folder_id AND f.path = i.path AND f.deleted_at IS NULL
            JOIN versions v ON v.id = f.current_version_id WHERE i.link_id = l.id) END AS bytes
FROM links l LEFT JOIN users u ON u.id = l.user_id LEFT JOIN folders fo ON fo.id = l.folder_id LEFT JOIN devices de ON de.id = l.device_id
WHERE l.direction = :direction AND (:all::boolean OR l.user_id = :user::bigint) ORDER BY u.name, l.name;

--! link_items
SELECT folder_id, path, remote_id, version FROM link_items WHERE link_id = :link_id;

--! remember_item
INSERT INTO link_items (link_id, folder_id, path, version) VALUES (:link_id, :folder_id, :path, :version)
ON CONFLICT (link_id, folder_id, path) DO UPDATE SET version = excluded.version;

--! remember_remote_item
-- A file known by the remote's id for it: it keeps the id when it is renamed, and comes back when it changed at the source.
INSERT INTO link_items (link_id, folder_id, path, remote_id, version) VALUES (:link_id, :folder_id, :path, :remote_id, :version)
ON CONFLICT (link_id, remote_id) WHERE remote_id <> '' DO UPDATE SET path = excluded.path, version = excluded.version, removed_in_web_app = FALSE;

--! forget_item
DELETE FROM link_items WHERE link_id = :link_id AND folder_id = :folder_id AND path = :path;

--! forget_remote_item
DELETE FROM link_items WHERE link_id = :link_id AND remote_id = :remote_id;

--! forget_items
DELETE FROM link_items WHERE link_id = :link_id;
