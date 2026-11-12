--! insert_source_folder
INSERT INTO folders (name, kind, created_at, user_id) VALUES (:name, 'user', :now, :user_id)
ON CONFLICT (user_id, name) WHERE kind = 'user' DO NOTHING RETURNING id, key;

--! live_paths
-- The live files of the folder a source writes into.
SELECT path FROM files WHERE folder_id = :folder_id AND deleted_at IS NULL;

--! removed_in_web_app
-- Paths deleted in the web app: still in the trash, or already removed by retention (then only the record of the link remembers it).
SELECT path FROM files WHERE folder_id = :folder_id AND deleted_at IS NOT NULL AND deleted_by = 'web'
UNION SELECT path FROM link_items WHERE link_id = :link_id AND removed_in_web_app;
