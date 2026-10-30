-- Pictures (thumbnails and previews) are no longer made ahead for every file: a job makes one when somebody looks at the file.
-- The jobs of the old kind, which name no size, are of no use.
DELETE FROM jobs WHERE kind = 'thumb';
