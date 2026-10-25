//! What a user says about a file: favorite, archived, rating, description, and a corrected time and place. One row
//! per user and file, made the first time they say anything. It is theirs: on a master file everybody has their own.

use super::Viewer;
use crate::platform::pg::Pg;
use fafnirx_db::client::Params;
use fafnirx_db::queries::library as q;
use postgres::{Error, GenericClient};
use uuid::Uuid;

/// Whether the file exists, is not deleted and the viewer can see it.
pub fn file_is_visible(c: &mut impl GenericClient, who: Viewer, file_id: Uuid) -> Result<bool, Error> {
    let params = q::VisibleFileExistsParams { id: file_id, all: who.all, scope_user: who.scope_user };
    q::visible_file_exists().params(&mut Pg(c), &params).one()
}

/// Makes sure the user has a row for the file, so the changes below have something to change.
pub fn ensure(c: &mut impl GenericClient, uid: i64, file_id: Uuid, now: i64) -> Result<(), Error> {
    q::ensure_file_state().params(&mut Pg(c), &q::EnsureFileStateParams { file_id, uid, now })?;
    Ok(())
}

pub fn set_favorite(c: &mut impl GenericClient, uid: i64, file_id: Uuid, value: bool) -> Result<(), Error> {
    q::set_favorite().params(&mut Pg(c), &q::SetFavoriteParams { value, file_id, uid })?;
    Ok(())
}

pub fn set_archived(c: &mut impl GenericClient, uid: i64, file_id: Uuid, value: bool) -> Result<(), Error> {
    q::set_archived().params(&mut Pg(c), &q::SetArchivedParams { value, file_id, uid })?;
    Ok(())
}

/// 1 to 5 stars, or none.
pub fn set_rating(c: &mut impl GenericClient, uid: i64, file_id: Uuid, rating: Option<i16>) -> Result<(), Error> {
    q::set_rating().params(&mut Pg(c), &q::SetRatingParams { rating, file_id, uid })?;
    Ok(())
}

pub fn set_description(c: &mut impl GenericClient, uid: i64, file_id: Uuid, description: &str) -> Result<(), Error> {
    q::set_description().params(&mut Pg(c), &q::SetDescriptionParams { description, file_id, uid })?;
    Ok(())
}

/// A corrected time (unix seconds), or `None` to go back to what the camera said.
pub fn set_taken_at(c: &mut impl GenericClient, uid: i64, file_id: Uuid, taken_at: Option<i64>) -> Result<(), Error> {
    q::set_taken_at().params(&mut Pg(c), &q::SetTakenAtParams { taken_at, file_id, uid })?;
    Ok(())
}

/// A corrected place, or `None` to go back to what the camera said.
pub fn set_place(c: &mut impl GenericClient, uid: i64, file_id: Uuid, place: Option<(f64, f64)>) -> Result<(), Error> {
    let (lat, lon) = place.unzip();
    q::set_place().params(&mut Pg(c), &q::SetPlaceParams { lat, lon, file_id, uid })?;
    Ok(())
}

/// Notes when the user last changed something about the file.
pub fn touch(c: &mut impl GenericClient, uid: i64, file_id: Uuid, now: i64) -> Result<(), Error> {
    q::touch_file_state().params(&mut Pg(c), &q::TouchFileStateParams { now, file_id, uid })?;
    Ok(())
}

/// The same favorite for many files at once: those the viewer can see. Returns how many files changed.
pub fn bulk_set_favorite(
    c: &mut impl GenericClient,
    who: Viewer,
    ids: &[Uuid],
    value: bool,
    now: i64,
) -> Result<u64, Error> {
    let params = q::BulkSetFavoriteParams { uid: who.uid, now, value, ids, all: who.all, scope_user: who.scope_user };
    q::bulk_set_favorite().params(&mut Pg(c), &params)
}

/// The same archived state for many files at once: those the viewer can see. Returns how many files changed.
pub fn bulk_set_archived(
    c: &mut impl GenericClient,
    who: Viewer,
    ids: &[Uuid],
    value: bool,
    now: i64,
) -> Result<u64, Error> {
    let params = q::BulkSetArchivedParams { uid: who.uid, now, value, ids, all: who.all, scope_user: who.scope_user };
    q::bulk_set_archived().params(&mut Pg(c), &params)
}
