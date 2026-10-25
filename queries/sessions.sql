--! session_user : (id, name, role)
SELECT u.id, u.name, u.role FROM sessions s JOIN users u ON u.id = s.user_id
WHERE s.token_hash = :token_hash AND s.expires_at > :now AND u.disabled_at IS NULL;

--! delete_expired_sessions
DELETE FROM sessions WHERE expires_at <= :now;

--! insert_session
INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (:token_hash, :user_id, :created_at, :expires_at);

--! delete_session
DELETE FROM sessions WHERE token_hash = :token_hash;

--! delete_user_sessions
DELETE FROM sessions WHERE user_id = :user_id;

--! delete_user_sessions_except
DELETE FROM sessions WHERE user_id = :user_id AND token_hash <> :keep;
