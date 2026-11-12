//! Sources: what is theirs besides being a link (`links`): the folder they write into is a whole folder of their owner's, and
//! what they collected is kept apart from what else is in it.

use fafnirx_db::client::GenericClient;
use fafnirx_db::queries::sources as q;
use postgres::Error;

/// Adds the folder a source stores its files in when its owner named none. `None` when the user already has one of that name.
pub fn insert_folder(c: &mut impl GenericClient, user_id: i64, name: &str, now: i64) -> Result<Option<(i64, String)>, Error> {
    if super::folders::name_taken(c, Some(user_id), name)? {
        return Ok(None);
    }
    Ok(q::insert_source_folder().bind(c, &name, &now, &user_id).opt()?.map(|d| (d.id, d.key)))
}

/// The live files of the folder a source writes into.
pub fn live_paths(c: &mut impl GenericClient, folder_id: i64) -> Result<Vec<String>, Error> {
    q::live_paths().bind(c, &folder_id).all()
}

/// Paths deleted in the web app: still in the trash, or already removed by retention (then only the record of the source
/// remembers it).
pub fn removed_in_web_app(c: &mut impl GenericClient, folder_id: i64, link_id: i64) -> Result<Vec<String>, Error> {
    q::removed_in_web_app().bind(c, &folder_id, &link_id).all()
}
