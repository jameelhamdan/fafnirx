-- 0011: a job can say what it found (the picture a lookup chose, for instance), and the renames recorded before a rename was
-- told apart from a delete are corrected: the old path of a rename was logged as a delete and showed in the Trash.
ALTER TABLE jobs ADD COLUMN note TEXT;

-- The old path is kept (hidden) until retention, as a rename leaves it now; the wrong `delete` events go.
UPDATE files f SET deleted_by = 'rename'
FROM events e
WHERE e.device_id = f.device_id AND e.path = f.path AND e.action = 'delete' AND e.detail LIKE 'renamed to %'
  AND f.deleted_at IS NOT NULL AND f.deleted_by IS NULL;
DELETE FROM events WHERE action = 'delete' AND detail LIKE 'renamed to %';
