//! What a mirror keeps of the library (albums, tags, favorites, ...), so that it survives losing the database.
//! Everything is named by user, device folder and path rather than by id, because ids do not survive a restore.

use crate::platform::pg::Pg;
use fafnirx_db::queries::library as q;
use postgres::{Error, GenericClient};

pub type Album = q::SnapshotAlbums;
/// A file of an album, by album id.
pub type AlbumFile = q::SnapshotAlbumFiles;
pub type Tag = q::SnapshotTags;
/// A file that carries a tag, by tag id.
pub type TagFile = q::SnapshotTagFiles;
/// What a user said about a file.
pub type State = q::SnapshotStates;

pub fn albums(c: &mut impl GenericClient) -> Result<Vec<Album>, Error> {
    q::snapshot_albums().bind(&mut Pg(c)).all()
}

pub fn album_files(c: &mut impl GenericClient) -> Result<Vec<AlbumFile>, Error> {
    q::snapshot_album_files().bind(&mut Pg(c)).all()
}

pub fn tags(c: &mut impl GenericClient) -> Result<Vec<Tag>, Error> {
    q::snapshot_tags().bind(&mut Pg(c)).all()
}

pub fn tag_files(c: &mut impl GenericClient) -> Result<Vec<TagFile>, Error> {
    q::snapshot_tag_files().bind(&mut Pg(c)).all()
}

/// Every favorite, archived file, rating, description, corrected time and corrected place.
pub fn states(c: &mut impl GenericClient) -> Result<Vec<State>, Error> {
    q::snapshot_states().bind(&mut Pg(c)).all()
}
