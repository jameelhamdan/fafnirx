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
    /// The folder the album shows, as (device folder name, prefix of the paths below it).
    pub folder: (String, String),
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
            folder: (a.folder_device, a.folder_prefix),
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

/// The id of the user's album that follows this folder (`prefix` is what the paths below it start with).
pub fn following(c: &mut impl GenericClient, user_id: i64, device_id: i64, prefix: &str) -> Result<Option<Uuid>, Error> {
    q::album_of_folder().params(c, &q::AlbumOfFolderParams { user_id, device_id, prefix }).opt()
}

/// A new album showing the folder `(device id, prefix)`. Returns its id.
pub fn insert(c: &mut impl GenericClient, user_id: i64, name: &str, description: &str, folder: (i64, &str), now: i64) -> Result<Uuid, Error> {
    let params = q::InsertAlbumParams { name, description, now, user_id, folder_device_id: folder.0, folder_prefix: folder.1 };
    q::insert_album().params(c, &params).one()
}

pub fn rename(c: &mut impl GenericClient, id: Uuid, name: &str) -> Result<(), Error> {
    q::rename_album().params(c, &q::RenameAlbumParams { id, name }).map(drop)
}

pub fn set_description(c: &mut impl GenericClient, id: Uuid, description: &str) -> Result<(), Error> {
    q::set_album_description().params(c, &q::SetAlbumDescriptionParams { id, description }).map(drop)
}

/// Whether the file is below the folder the album shows.
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

/// The folder a rename moved, as the paths with a slash at the end that the old and the new folder start with: what is
/// left of the two folders once the part they share at the end (`S1/` in `Show/S1/e1.mkv` to `Show 2/S1/e1.mkv`) is
/// taken off. `None` when the file stayed in its folder.
fn moved_folder(old: &str, new: &str) -> Option<(String, String)> {
    let dirs = |path: &str| -> Vec<String> {
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        parts.into_iter().map(str::to_string).collect()
    };
    let (mut from, mut to) = (dirs(old), dirs(new));
    while from.last().is_some() && from.last() == to.last() {
        from.pop();
        to.pop();
    }
    if from.is_empty() && to.is_empty() {
        return None;
    }
    let prefix = |parts: &[String]| parts.iter().map(|p| format!("{p}/")).collect::<String>();
    Some((prefix(&from), prefix(&to)))
}

/// After a file was renamed from `old` to `new` on a device: the albums that follow the folder it left follow it to its new
/// place once nothing is left in the old one (a folder is renamed file by file, so this is true after its last file), and
/// an album named after the folder is named after the new one. An album of a folder that exists already is left alone.
/// Returns whether an album changed.
pub fn follow_folder_move(c: &mut impl GenericClient, device_id: i64, old: &str, new: &str, now: i64) -> Result<bool, Error> {
    let Some((from, to)) = moved_folder(old, new) else { return Ok(false) };
    // Only a folder renamed in place (same parent) is followed; one emptied into an unrelated place stays where it was.
    let parent = |p: &str| p.trim_end_matches('/').rsplit_once('/').map_or(String::new(), |(d, _)| d.to_string());
    if from.is_empty() || parent(&from) != parent(&to) {
        return Ok(false);
    }
    follow_folder(c, device_id, &from, &to, now, true)
}

/// A folder was moved from `from` to `to` (both end with a slash) as a whole: the albums that follow it, or a folder below
/// it, follow it to the new place, wherever it is (the folder operation knows; a file-by-file move has to guess, see
/// [`follow_folder_move`]). With `when_emptied` an album waits until nothing is left in the old folder. An album of a folder
/// that exists already is left alone. Returns whether an album changed.
pub fn follow_folder(c: &mut impl GenericClient, device_id: i64, from: &str, to: &str, now: i64, when_emptied: bool) -> Result<bool, Error> {
    let mut moved = false;
    for album in q::albums_under_folder().bind(&mut *c, &device_id, &from).all()? {
        let Some(prefix) = album.folder_prefix else { continue };
        if when_emptied && !q::folder_is_empty().bind(&mut *c, &device_id, &prefix).one()? {
            continue;
        }
        let target = format!("{to}{}", &prefix[from.len()..]);
        if following(c, album.user_id, device_id, &target)?.is_some() {
            continue;
        }
        let last = |p: &str| p.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
        let name = if album.name == last(&prefix) { last(&target) } else { album.name };
        q::move_album_folder().params(&mut *c, &q::MoveAlbumFolderParams { id: album.id, prefix: &target, name: &name, now })?;
        moved = true;
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::moved_folder;

    #[test]
    fn a_renamed_folder_is_what_differs_before_the_part_they_share() {
        let moved = |a, b| moved_folder(a, b);
        assert_eq!(moved("Show/S1/e1.mkv", "Show 2/S1/e1.mkv"), Some(("Show/".into(), "Show 2/".into())));
        assert_eq!(moved("Show/S1/e1.mkv", "Show/S2/e1.mkv"), Some(("Show/S1/".into(), "Show/S2/".into())));
        assert_eq!(moved("a/x.mkv", "b/y.mkv"), Some(("a/".into(), "b/".into())));
        assert_eq!(moved("a/x.mkv", "x.mkv"), Some(("a/".into(), String::new())));
        assert_eq!(moved("a/x.mkv", "a/y.mkv"), None, "only the file was renamed");
        assert_eq!(moved("x.mkv", "y.mkv"), None);
    }
}
