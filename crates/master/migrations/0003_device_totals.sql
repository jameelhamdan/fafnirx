-- 0003: totals kept on the device row. Adding up every file is proportional to the number of files, so the dashboard and
-- the device list read these instead. A write that touches a device marks them stale; they are added up again once the
-- device is done syncing (see devices/totals.rs).
ALTER TABLE devices
    ADD COLUMN file_count    BIGINT  NOT NULL DEFAULT 0,       -- live files
    ADD COLUMN total_bytes   BIGINT  NOT NULL DEFAULT 0,       -- their current versions
    ADD COLUMN deleted_files BIGINT  NOT NULL DEFAULT 0,       -- files in the trash
    ADD COLUMN totals_stale  BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN totals_at     BIGINT;
