//! Tags. A tag belongs to the user who made it; a file can carry any number of its user's tags.

use super::Viewer;
use crate::platform::pg::Pg;
use fafnirx_db::client::Params;
use fafnirx_db::queries::library as q;
use postgres::{Error, GenericClient};

/// A tag with how many files the viewer can see that carry it.
pub type Counted = q::ListTags;

/// The id of the user's tag with this name (case does not matter), made when it is new.
pub fn ensure(c: &mut impl GenericClient, uid: i64, name: &str) -> Result<i64, Error> {
    q::upsert_tag().params(&mut Pg(c), &q::UpsertTagParams { name, uid }).one()
}

/// The tags the user put on a file, as (id, name).
pub fn of_file(c: &mut impl GenericClient, uid: i64, file_id: i64) -> Result<Vec<(i64, String)>, Error> {
    let params = q::TagsOfFileParams { file_id, uid };
    Ok(q::tags_of_file().params(&mut Pg(c), &params).all()?.into_iter().map(|t| (t.id, t.name)).collect())
}

/// Makes `tag_ids` the user's tags of the file: the others of the user come off, the others' tags stay.
pub fn replace_on_file(c: &mut impl GenericClient, uid: i64, file_id: i64, tag_ids: &[i64]) -> Result<(), Error> {
    q::keep_only_tags().params(&mut Pg(&mut *c), &q::KeepOnlyTagsParams { file_id, tag_ids, uid })?;
    q::tag_file().params(&mut Pg(c), &q::TagFileParams { file_id, tag_ids })?;
    Ok(())
}

/// Puts the tags on the files the viewer can see. Returns how many were added.
pub fn add_to_files(c: &mut impl GenericClient, who: Viewer, ids: &[i64], tag_ids: &[i64]) -> Result<u64, Error> {
    let params = q::BulkAddTagsParams { tag_ids, ids, all: who.all, scope_user: who.scope_user };
    q::bulk_add_tags().params(&mut Pg(c), &params)
}

/// Removes the tags no file carries any more.
pub fn drop_unused(c: &mut impl GenericClient) -> Result<(), Error> {
    q::drop_unused_tags().bind(&mut Pg(c))?;
    Ok(())
}

/// The viewer's tags with how many visible files carry each.
pub fn counted(c: &mut impl GenericClient, who: Viewer) -> Result<Vec<Counted>, Error> {
    let params = q::ListTagsParams { all: who.all, scope_user: who.scope_user, uid: who.uid };
    q::list_tags().params(&mut Pg(c), &params).all()
}

/// Deletes one of the user's tags. Returns false when they have no such tag.
pub fn delete(c: &mut impl GenericClient, uid: i64, id: i64) -> Result<bool, Error> {
    Ok(q::delete_tag().params(&mut Pg(c), &q::DeleteTagParams { id, uid })? > 0)
}
