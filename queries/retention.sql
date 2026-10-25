--! lock_expired_files
SELECT id, device_id FROM files WHERE deleted_at IS NOT NULL AND deleted_at < :cutoff FOR UPDATE;

--! delete_versions_of_files
DELETE FROM versions WHERE file_id = ANY(:file_ids) RETURNING id;

--! delete_files
DELETE FROM files WHERE id = ANY(:file_ids);

--! delete_pruned_versions
DELETE FROM versions WHERE pruned_at IS NOT NULL AND pruned_at < :cutoff RETURNING id;

--! all_version_ids
SELECT id FROM versions;
