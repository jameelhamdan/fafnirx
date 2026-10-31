//! Tags. A tag belongs to the user who made it; a file can carry any number of its user's tags.

use super::Viewer;
use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::library as q;
use postgres::Error;
use uuid::Uuid;

/// A tag with how many files the viewer can see that carry it.
pub type Counted = q::ListTags;

/// The id of the user's tag with this name (case does not matter), made when it is new.
pub fn ensure(c: &mut impl GenericClient, uid: i64, name: &str) -> Result<Uuid, Error> {
    q::upsert_tag().params(c, &q::UpsertTagParams { name, uid }).one()
}

/// The tags the user put on a file, as (id, name).
pub fn of_file(c: &mut impl GenericClient, uid: i64, file_id: Uuid) -> Result<Vec<(Uuid, String)>, Error> {
    let params = q::TagsOfFileParams { file_id, uid };
    Ok(q::tags_of_file().params(c, &params).all()?.into_iter().map(|t| (t.id, t.name)).collect())
}

/// Makes `tag_ids` the user's tags of the file: the others of the user come off, the others' tags stay.
pub fn replace_on_file(c: &mut impl GenericClient, uid: i64, file_id: Uuid, tag_ids: &[Uuid]) -> Result<(), Error> {
    q::keep_only_tags().params(&mut *c, &q::KeepOnlyTagsParams { file_id, tag_ids, uid })?;
    q::tag_file().params(c, &q::TagFileParams { file_id, tag_ids }).map(drop)
}

/// Puts the tags on the files the viewer can see. Returns how many were added.
pub fn add_to_files(c: &mut impl GenericClient, who: Viewer, ids: &[Uuid], tag_ids: &[Uuid]) -> Result<u64, Error> {
    let params = q::BulkAddTagsParams { tag_ids, ids, all: who.all, scope_user: who.scope_user };
    q::bulk_add_tags().params(c, &params)
}

/// The viewer's tags with how many visible files carry each.
pub fn counted(c: &mut impl GenericClient, who: Viewer) -> Result<Vec<Counted>, Error> {
    let params = q::ListTagsParams { all: who.all, scope_user: who.scope_user, uid: who.uid };
    q::list_tags().params(c, &params).all()
}

/// Deletes one of the user's tags. Returns false when they have no such tag.
pub fn delete(c: &mut impl GenericClient, uid: i64, id: Uuid) -> Result<bool, Error> {
    Ok(q::delete_tag().params(c, &q::DeleteTagParams { id, uid })? > 0)
}
