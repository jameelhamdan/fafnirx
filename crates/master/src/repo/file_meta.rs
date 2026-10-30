//! What was read from the content of a file (kind, size, time and place taken, EXIF), once per content.

use crate::platform::pg::Pg;
use fafnirx_db::client::Params;
use fafnirx_db::queries::files as q;
use postgres::{Error, GenericClient};

/// What was read from one content.
pub struct Meta<'a> {
    pub mime: &'a str,
    pub kind: &'a str,
    pub width: Option<i64>,
    pub height: Option<i64>,
    /// When it was taken, unix seconds.
    pub taken_at: Option<i64>,
    /// The EXIF data as JSON text.
    pub exif: &'a str,
    pub duration_ms: Option<i64>,
    pub codec: Option<&'a str>,
    pub place: Option<(f64, f64)>,
    /// The caption and the keywords (a JSON array) written into the file.
    pub description: Option<&'a str>,
    pub keywords: &'a str,
}

/// Remembers what was read from a content, replacing what was read before.
pub fn save(c: &mut impl GenericClient, root: &[u8], m: &Meta, read_at: i64) -> Result<(), Error> {
    let (lat, lon) = m.place.unzip();
    let params = q::SaveFileMetaParams {
        root,
        mime: m.mime,
        width: m.width,
        height: m.height,
        taken_at: m.taken_at,
        exif: m.exif,
        read_at,
        kind: m.kind,
        duration_ms: m.duration_ms,
        codec: m.codec,
        lat,
        lon,
        caption: m.description,
        keywords: Some(m.keywords),
    };
    q::save_file_meta().params(&mut Pg(c), &params).map(drop)
}

/// Remembers where in a video its poster frame is (milliseconds).
pub fn set_poster_ms(c: &mut impl GenericClient, root: &[u8], poster_ms: i64) -> Result<(), Error> {
    q::set_poster_ms().bind(&mut Pg(c), &poster_ms, &root).map(drop)
}

/// Where in a video its poster frame was found; none when it was not looked for (or the file is not a video).
pub fn poster_ms(c: &mut impl GenericClient, root: &[u8]) -> Result<Option<i64>, Error> {
    Ok(q::poster_ms_of().bind(&mut Pg(c), &root).opt()?.and_then(|r| r.poster_ms))
}

/// Whether the path of a device still holds this content (a job made for it may be out of date).
pub fn path_holds(c: &mut impl GenericClient, device: &str, path: &str, root: &[u8]) -> Result<bool, Error> {
    q::file_holds_content().params(&mut Pg(c), &q::FileHoldsContentParams { device, path, root }).one()
}
