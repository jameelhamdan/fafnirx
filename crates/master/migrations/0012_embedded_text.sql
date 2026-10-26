-- 0012: the caption and keywords written into a photo (EXIF, IPTC, XMP) are kept next to what else was read from it, and every
-- photo is read again so that they, and the GPS positions the first reader missed, are found.
ALTER TABLE file_meta ADD COLUMN caption TEXT;
ALTER TABLE file_meta ADD COLUMN keywords TEXT;

INSERT INTO jobs (kind, payload, created_at)
SELECT 'meta', jsonb_build_object('device', d.folder, 'path', f.path, 'root', encode(v.root_hash, 'hex'))::text,
       extract(epoch FROM now())::bigint
FROM files f
JOIN devices d ON d.id = f.device_id
JOIN versions v ON v.id = f.current_version_id
WHERE f.deleted_at IS NULL AND f.media = 'image';
