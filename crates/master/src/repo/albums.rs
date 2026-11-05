//! Albums: a user's shortcut to a folder of a device.

use fafnirx_db::client::GenericClient;
use fafnirx_db::client::Params;
use fafnirx_db::queries::albums as q;
use postgres::Error;
use uuid::Uuid;

/// An album with what it holds.
pub struct Album {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// Live photos and videos in it.
    pub files: i64,
    pub bytes: i64,
    /// The file chosen as the cover, if one was (`library::albums` falls back on a poster, the first video, the first picture).
    pub cover: Option<Uuid>,
    /// The folder the album shows: its device folder name.
    pub folder: String,
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
            cover: a.chosen,
            folder: a.folder_device,
        }
    }
}

/// The albums of a user, newest change first.
pub fn of_user(c: &mut impl GenericClient, user_id: i64) -> Result<Vec<Album>, Error> {
    let params = q::FindAlbumsParams { user_id, id: None };
    Ok(q::find_albums().params(c, &params).all()?.into_iter().map(Album::from).collect())
}

/// The album with this id, if it is the user's: someone else's album does not exist for them.
pub fn find(c: &mut impl GenericClient, user_id: i64, id: Uuid) -> Result<Option<Album>, Error> {
    let params = q::FindAlbumsParams { user_id, id: Some(id) };
    Ok(q::find_albums().params(c, &params).opt()?.map(Album::from))
}

/// The live photos and videos of an album as (file id, path).
pub fn media(c: &mut impl GenericClient, album_id: Uuid) -> Result<Vec<(Uuid, String)>, Error> {
    Ok(q::media_of_album().bind(c, &album_id).all()?.into_iter().map(|f| (f.id, f.path)).collect())
}

/// The id of the user's album of this folder.
pub fn of_folder(c: &mut impl GenericClient, user_id: i64, device_id: i64) -> Result<Option<Uuid>, Error> {
    q::album_of_folder().bind(c, &user_id, &device_id).opt()
}

/// A new album showing the folder with this device id. Returns its id.
pub fn insert(c: &mut impl GenericClient, user_id: i64, name: &str, description: &str, folder_device_id: i64, now: i64) -> Result<Uuid, Error> {
    let params = q::InsertAlbumParams { name, description, now, user_id, folder_device_id };
    q::insert_album().params(c, &params).one()
}

pub fn rename(c: &mut impl GenericClient, id: Uuid, name: &str) -> Result<(), Error> {
    q::rename_album().params(c, &q::RenameAlbumParams { id, name }).map(drop)
}

pub fn set_description(c: &mut impl GenericClient, id: Uuid, description: &str) -> Result<(), Error> {
    q::set_album_description().params(c, &q::SetAlbumDescriptionParams { id, description }).map(drop)
}

/// Whether the file is in the folder the album shows.
pub fn contains(c: &mut impl GenericClient, album_id: Uuid, file_id: Uuid) -> Result<bool, Error> {
    q::file_is_in_album().params(c, &q::FileIsInAlbumParams { album_id, file_id }).one()
}

/// The file shown for the album, or `None` to choose one again.
pub fn set_cover(c: &mut impl GenericClient, id: Uuid, cover: Option<Uuid>) -> Result<(), Error> {
    q::set_album_cover().params(c, &q::SetAlbumCoverParams { id, cover }).map(drop)
}

/// Notes when the album last changed.
pub fn touch(c: &mut impl GenericClient, id: Uuid, now: i64) -> Result<(), Error> {
    q::touch_album().params(c, &q::TouchAlbumParams { id, now }).map(drop)
}

/// Deletes the user's album; the files stay where they are. Returns false when they have no such album.
pub fn delete(c: &mut impl GenericClient, user_id: i64, id: Uuid) -> Result<bool, Error> {
    Ok(q::delete_album().params(c, &q::DeleteAlbumParams { id, user_id })? > 0)
}

/// The albums of the user that hold a file, as (id, name).
pub fn of_file(c: &mut impl GenericClient, uid: i64, file_id: Uuid) -> Result<Vec<(Uuid, String)>, Error> {
    let params = q::AlbumsOfFileParams { file_id, uid };
    Ok(q::albums_of_file().params(c, &params).all()?.into_iter().map(|a| (a.id, a.name)).collect())
}
