-- Where in a video (milliseconds) its poster frame is: the first frame at 10%, 20%, ... of its length that is not black.
-- Found once, when the video is read, and kept so that the pictures of the video are always taken from the same place.
ALTER TABLE file_meta ADD COLUMN poster_ms BIGINT;
