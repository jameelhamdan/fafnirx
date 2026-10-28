-- 0013: shared folders. A shared folder belongs to a user, not to a computer: it is a devices row of kind 'shared' (like a
-- local folder or a source it has no token) whose files live in files/users/<user>/<name>/. Any client of the owner can
-- attach it, send changes to it and receive the changes of the others, so versions, trash, mirrors and the web app work
-- on it like on every other folder.

ALTER TABLE devices DROP CONSTRAINT devices_kind;
ALTER TABLE devices ADD CONSTRAINT devices_kind CHECK (kind IN ('device', 'source', 'local', 'master', 'shared'));

-- The owner uses a shared folder on all their computers without any setup. Other users get in only when the owner adds them
-- here, one by one, in the web app; they can be limited to reading.
CREATE TABLE shared_members (
    folder_id  BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL REFERENCES users(id),
    can_write  BOOLEAN NOT NULL DEFAULT TRUE,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (folder_id, user_id)
);
CREATE INDEX shared_members_user ON shared_members (user_id);

-- What each computer says about the shared folders attached to it, with every heartbeat (they replace the earlier ones).
CREATE TABLE device_shared_state (
    device_id  BIGINT NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    shared     TEXT NOT NULL,          -- <owner>/<name>, as the computer names it
    files      BIGINT NOT NULL,
    bytes      BIGINT NOT NULL,
    last_cycle BIGINT NOT NULL,        -- 0 before the first
    last_error TEXT NOT NULL,
    held       BIGINT NOT NULL,
    failing    BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (device_id, shared)
);
CREATE INDEX device_shared_state_folder ON device_shared_state (shared);

-- A shared folder holds one spelling of a name whatever the case (every kind of computer has to hold it), so a commit looks for the
-- same path, or the same folder, in another case among the live paths of the folder.
CREATE INDEX files_folded ON files (device_id, lower(path) text_pattern_ops) WHERE deleted_at IS NULL;

-- The one rule for what a user sees of the devices and folders: everything (an administrator), the master files, their
-- own, and the shared folders they were added to. Every query that scopes by user calls this.
CREATE FUNCTION visible(dev devices, uid BIGINT, everything BOOLEAN) RETURNS BOOLEAN LANGUAGE sql STABLE AS
$$ SELECT everything OR dev.kind = 'master' OR dev.user_id = uid
       OR EXISTS (SELECT 1 FROM shared_members m JOIN users o ON o.id = dev.user_id
                  WHERE m.folder_id = dev.id AND m.user_id = uid AND o.disabled_at IS NULL) $$;
