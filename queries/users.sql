--! list_users : (name, role, disabled, created_at, devices, folders)
SELECT u.name, u.role, u.disabled_at IS NOT NULL AS disabled, u.created_at,
       (SELECT COUNT(*) FROM devices d WHERE d.user_id = u.id) AS devices,
       (SELECT COUNT(*) FROM folders f WHERE f.user_id = u.id AND f.kind = 'user') AS folders
FROM users u ORDER BY u.name;

--! user_names
SELECT name FROM users;

--! enabled_users : (name, role)
SELECT name, role FROM users WHERE disabled_at IS NULL ORDER BY name;

--! user_exists
SELECT 1 FROM users WHERE name = :name;

--! insert_user
INSERT INTO users (name, role, password_hash, created_at) VALUES (:name, :role, :password_hash, :now);

--! lock_users
LOCK TABLE users IN SHARE ROW EXCLUSIVE MODE;

--! user_by_name : (id, role, disabled)
SELECT id, role, disabled_at IS NOT NULL AS disabled FROM users WHERE name = :name;

--! other_active_admins
SELECT COUNT(*) AS others FROM users WHERE role = 'admin' AND disabled_at IS NULL AND id <> :id;

--! set_user_role
UPDATE users SET role = :role WHERE id = :id;

--! disable_user
UPDATE users SET disabled_at = COALESCE(disabled_at, :now) WHERE id = :id;

--! enable_user
UPDATE users SET disabled_at = NULL WHERE id = :id;

--! set_password_hash
UPDATE users SET password_hash = :password_hash WHERE id = :id;

--! admin_credentials : (id, password_hash)
SELECT id, password_hash FROM users WHERE name = 'admin';

--! login_credentials : (id, password_hash)
SELECT id, password_hash FROM users WHERE name = :name AND disabled_at IS NULL;

--! password_hash_of
SELECT password_hash FROM users WHERE id = :id;

--! user_who_can_have_devices
SELECT id FROM users WHERE name = :name AND disabled_at IS NULL AND role <> 'readonly';

--! devices_of_user : (id, key)
SELECT id, key FROM devices WHERE user_id = :id ORDER BY id;

--! folders_of_user : (id, key)
-- The folders a user made (the directories of their devices go with the device).
SELECT id, key FROM folders WHERE user_id = :id AND kind = 'user' ORDER BY id;

--! sources_of_user
SELECT name FROM links WHERE direction = 'in' AND user_id = :id ORDER BY name;

--! links_on_folders_of_user : (name, key)
-- What else uses the user's folders and devices: a mirror that holds one, another user's source that writes into one.
SELECT l.name, COALESCE(fo.key, de.key) AS key FROM links l
LEFT JOIN folders fo ON fo.id = l.folder_id LEFT JOIN devices de ON de.id = l.device_id
WHERE fo.user_id = :id OR de.user_id = :id ORDER BY 2, l.name LIMIT 1;

--! delete_memberships_of_user
DELETE FROM shared_members WHERE user_id = :id;

--! delete_file_state_of_user
DELETE FROM file_state WHERE user_id = :id;

--! delete_tags_of_user
DELETE FROM tags WHERE user_id = :id;

--! delete_albums_of_user
DELETE FROM albums WHERE user_id = :id;

--! resolve_alerts_of_user
-- Alerts about the user's devices, folders and sources are keyed `<user>/<name>`.
UPDATE alerts SET resolved_at = :now WHERE resolved_at IS NULL AND starts_with(key, :prefix);

--! delete_user
DELETE FROM users WHERE id = :id;
