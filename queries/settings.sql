--! get_setting
SELECT value FROM settings WHERE key = :key;

--! set_setting
INSERT INTO settings (key, value) VALUES (:key, :value) ON CONFLICT (key) DO UPDATE SET value = excluded.value;
