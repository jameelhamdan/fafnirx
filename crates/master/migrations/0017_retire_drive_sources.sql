-- The master no longer has a Google Drive source of its own: Drive is added as storage (rclone's `drive` backend).
-- Sources that were made for it are switched off, not removed, so the files they collected stay.
UPDATE sources
SET enabled = 0,
    run_requested = 0,
    last_status = 'failed',
    last_message = 'Google Drive sources are no longer supported. Add the folder again with Add storage source (Google Drive), then remove this one.'
WHERE kind = 'gdrive';
