--! list_users : (name, role, disabled, created_at, devices)
SELECT u.name, u.role, u.disabled_at IS NOT NULL AS disabled, u.created_at,
       (SELECT COUNT(*) FROM devices d WHERE d.user_id = u.id AND d.kind IN ('device', 'local')) AS devices
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
