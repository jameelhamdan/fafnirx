-- 0014: a mirror can hold part of the tree. `scope` is the folder it copies, as the path below files/ without `users/`: empty
-- for everything, `_master` or `_master/<folder>` for the master files, `<user>` for all of a user's devices and shared
-- folders, `<user>/<device>` or `<user>/<shared folder>` for one, and a folder inside any of them.
ALTER TABLE mirrors ADD COLUMN scope TEXT NOT NULL DEFAULT '';
