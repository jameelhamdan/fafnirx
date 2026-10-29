-- The master no longer makes playable copies of videos (the video module streams them), so the `proxy` jobs waiting in the
-- queue have nothing to do.
DELETE FROM jobs WHERE kind = 'proxy';
