//! Albums: a user's named set of files, picked by hand and/or following a folder of a device.

use super::Viewer;
use crate::platform::pg::Pg;
use fafnirx_db::client::Params;
use fafnirx_db::queries::albums as q;
use postgres::{Error, GenericClient};

/// An album with what it holds.
pub struct Album {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Live photos and videos in it.
    pub files: i64,
    pub bytes: i64,
    /// The file shown for the album: the one chosen, else the newest. Not set for an album that follows a folder, unless
    /// one was chosen (`library::albums` takes its first video).
    pub cover: Option<i64>,
    /// The folder the album follows, as (device folder name, prefix of the paths below it).
    pub folder: Option<(String, String)>,
}

impl From<q::FindAlbums> for Album {
    fn from(a: q::FindAlbums) -> Self {
        Album {
            id: a.id,
            name: a.name,
            description: a.description,
            created_at: a.created_at,
            updated_at: a.updated_at,
            files: a.files,
            bytes: a.bytes,
            cover: a.cover,
            folder: a.folder_device.map(|device| (device, a.folder_prefix.unwrap_or_default())),
        }
    }
}

/// The albums of a user, newest change first.
pub fn of_user(c: &mut impl GenericClient, user_id: i64) -> Result<Vec<Album>, Error> {
    let params = q::FindAlbumsParams { user_id, id: None };
    Ok(q::find_albums().params(&mut Pg(c), &params).all()?.into_iter().map(Album::from).collect())
}

/// The album with this id, if it is the user's: someone else's album does not exist for them.
pub fn find(c: &mut impl GenericClient, user_id: i64, id: i64) -> Result<Option<Album>, Error> {
    let params = q::FindAlbumsParams { user_id, id: Some(id) };
    Ok(q::find_albums().params(&mut Pg(c), &params).opt()?.map(Album::from))
}

/// The live photos and videos of an album as (file id, path).
pub fn media(c: &mut impl GenericClient, album_id: i64) -> Result<Vec<(i64, String)>, Error> {
    Ok(q::media_of_album().bind(&mut Pg(c), &album_id).all()?.into_iter().map(|f| (f.id, f.path)).collect())
}

/// The id of the user's album that follows this folder (`prefix` is what the paths below it start with).
pub fn following(c: &mut impl GenericClient, user_id: i64, device_id: i64, prefix: &str) -> Result<Option<i64>, Error> {
    q::album_of_folder().params(&mut Pg(c), &q::AlbumOfFolderParams { user_id, device_id, prefix }).opt()
}

/// A new album, optionally following the folder `(device id, prefix)`. Returns its id.
pub fn insert(
    c: &mut impl GenericClient,
    user_id: i64,
    name: &str,
    description: &str,
    folder: Option<(i64, &str)>,
    now: i64,
) -> Result<i64, Error> {
    let params = q::InsertAlbumParams {
        name,
        description,
        now,
        user_id,
        folder_device_id: folder.map(|(device, _)| device),
        folder_prefix: folder.map(|(_, prefix)| prefix),
    };
    q::insert_album().params(&mut Pg(c), &params).one()
}

/// Puts files in the album: those the viewer can see. Returns how many were added.
pub fn add_files(c: &mut impl GenericClient, who: Viewer, album_id: i64, ids: &[i64], now: i64) -> Result<u64, Error> {
    let params = q::AddFilesToAlbumParams { album_id, now, ids, all: who.all, scope_user: who.scope_user };
    q::add_files_to_album().params(&mut Pg(c), &params)
}

/// Puts the photos and videos at a place in the album: a file, or everything below a folder (`path` empty for the
/// whole device), of a device the viewer can see. Returns how many were added.
pub fn add_place(
    c: &mut impl GenericClient,
    who: Viewer,
    album_id: i64,
    device: &str,
    path: &str,
    now: i64,
) -> Result<u64, Error> {
    let params = q::AddPlaceToAlbumParams { album_id, now, device, path, all: who.all, scope_user: who.scope_user };
    q::add_place_to_album().params(&mut Pg(c), &params)
}

pub fn rename(c: &mut impl GenericClient, id: i64, name: &str) -> Result<(), Error> {
    q::rename_album().params(&mut Pg(c), &q::RenameAlbumParams { id, name })?;
    Ok(())
}

pub fn set_description(c: &mut impl GenericClient, id: i64, description: &str) -> Result<(), Error> {
    q::set_album_description().params(&mut Pg(c), &q::SetAlbumDescriptionParams { id, description })?;
    Ok(())
}

/// Whether the file is in the album: picked into it, or below the folder it follows.
pub fn contains(c: &mut impl GenericClient, album_id: i64, file_id: i64) -> Result<bool, Error> {
    q::file_is_in_album().params(&mut Pg(c), &q::FileIsInAlbumParams { album_id, file_id }).one()
}

/// The file shown for the album, or `None` to show the newest again.
pub fn set_cover(c: &mut impl GenericClient, id: i64, cover: Option<i64>) -> Result<(), Error> {
    q::set_album_cover().params(&mut Pg(c), &q::SetAlbumCoverParams { id, cover })?;
    Ok(())
}

/// Notes when the album last changed.
pub fn touch(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    q::touch_album().params(&mut Pg(c), &q::TouchAlbumParams { id, now })?;
    Ok(())
}

/// Deletes the user's album; the files stay where they are. Returns false when they have no such album.
pub fn delete(c: &mut impl GenericClient, user_id: i64, id: i64) -> Result<bool, Error> {
    Ok(q::delete_album().params(&mut Pg(c), &q::DeleteAlbumParams { id, user_id })? > 0)
}

/// Takes files out of the album, and the cover if it is one of them. Returns how many were taken out.
pub fn remove_files(c: &mut impl GenericClient, album_id: i64, ids: &[i64]) -> Result<u64, Error> {
    let removed =
        q::remove_files_from_album().params(&mut Pg(&mut *c), &q::RemoveFilesFromAlbumParams { album_id, ids })?;
    q::clear_cover_among().params(&mut Pg(c), &q::ClearCoverAmongParams { album_id, ids })?;
    Ok(removed)
}

/// The albums of the user that hold a file, as (id, name).
pub fn of_file(c: &mut impl GenericClient, uid: i64, file_id: i64) -> Result<Vec<(i64, String)>, Error> {
    let params = q::AlbumsOfFileParams { file_id, uid };
    Ok(q::albums_of_file().params(&mut Pg(c), &params).all()?.into_iter().map(|a| (a.id, a.name)).collect())
}
