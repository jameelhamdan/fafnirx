--! lock_expired_files
-- At most `limit` of them at a time, so that a large deletion is removed in short transactions.
SELECT id, folder_id FROM files WHERE deleted_at IS NOT NULL AND deleted_at < :cutoff ORDER BY deleted_at LIMIT :limit FOR UPDATE;

--! delete_versions_of_files
DELETE FROM versions WHERE file_id = ANY(:file_ids) RETURNING id;

--! delete_files
DELETE FROM files WHERE id = ANY(:file_ids);

--! delete_pruned_versions
DELETE FROM versions WHERE id IN (SELECT id FROM versions WHERE pruned_at IS NOT NULL AND pruned_at < :cutoff LIMIT :limit) RETURNING id;

--! known_version_ids
-- Which of `ids` have a record (the sweep of the store asks for a few thousand at a time).
SELECT id FROM versions WHERE id = ANY(:ids);
