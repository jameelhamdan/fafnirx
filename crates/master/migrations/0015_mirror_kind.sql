-- 0015: a mirror is a `replica` (the files as they are, to browse or copy back) or a `backup` (compressed and encrypted
-- with its own key, restored with rclone). The key is two passwords of rclone's crypt: the password and the salt.
ALTER TABLE mirrors ADD COLUMN kind TEXT NOT NULL DEFAULT 'replica';
ALTER TABLE mirrors ADD COLUMN crypt_password TEXT NOT NULL DEFAULT '';
ALTER TABLE mirrors ADD COLUMN crypt_salt TEXT NOT NULL DEFAULT '';
