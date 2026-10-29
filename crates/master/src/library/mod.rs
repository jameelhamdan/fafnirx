//! The photo and video library: a view over the stored files plus what people say about them. `meta` reads
//! metadata and makes pictures (and runs the job queue), `photos` is the timeline, map, trash and edits API,
//! `albums` the album API. What needs ffmpeg (video details, poster frames) comes from the video module, see `modules`.

pub mod albums;
pub mod artwork;
pub mod embedded;
pub mod meta;
pub mod photos;
