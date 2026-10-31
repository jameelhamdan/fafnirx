//! What a mirror keeps of the library (albums, tags, favorites, ...), so that it survives losing the database.
//! Everything is named by user, device folder and path rather than by id, because ids do not survive a restore.

use fafnirx_db::client::GenericClient;
use fafnirx_db::queries::library as q;
use postgres::Error;

pub type Tag = q::SnapshotTags;

pub fn tags(c: &mut impl GenericClient) -> Result<Vec<Tag>, Error> {
    q::snapshot_tags().bind(c).all()
}
