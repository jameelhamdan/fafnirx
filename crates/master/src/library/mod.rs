//! The photo and video library: a view over the stored files plus what people say about them. `meta` reads
//! metadata and makes pictures (and runs the job queue), `media` wraps ffmpeg, `photos` is the timeline, map,
//! trash and edits API, `albums` the album API, `stream` the adaptive video streaming (a proxy to the
//! separate `fafnirx-stream` service).

pub mod albums;
pub mod artwork;
pub mod media;
pub mod meta;
pub mod photos;
pub mod stream;
