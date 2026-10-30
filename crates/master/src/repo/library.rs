//! The library's view of the files: the photos and videos a user may see, with what they said about each, grouped by
//! day, placed on a map, or listed from the trash.
//!
//! Who is looking decides what is shown, so every query takes a [`Viewer`]. Which files a view shows is a
//! [`Filter`]; the web layer checks what a person typed and builds one.

use super::Viewer;
use crate::platform::pg::Pg;
use fafnirx_db::client::Params;
use fafnirx_db::queries::library as q;
use postgres::{Error, GenericClient};
use uuid::Uuid;

/// A photo or video as the grid and the viewer need it, with the user's own say about it.
pub type Asset = q::Asset;

/// Which files a view of the library starts from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// Everything that is not archived.
    Timeline,
    Favorites,
    Archive,
}

impl Scope {
    fn as_str(self) -> &'static str {
        match self {
            Self::Timeline => "timeline",
            Self::Favorites => "favorites",
            Self::Archive => "archive",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Media {
    Image,
    Video,
}

impl Media {
    fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
        }
    }
}

/// Which files a view shows: the scope, narrowed down by whatever else is given.
pub struct Filter<'a> {
    pub scope: Scope,
    /// Only the files of the device with this folder name.
    pub device: Option<&'a str>,
    pub kind: Option<Media>,
    /// Only the files that carry this tag.
    pub tag: Option<Uuid>,
    /// Only the files of this month, `YYYY-MM`.
    pub month: Option<&'a str>,
}

/// How many files each day holds, as (`YYYY-MM-DD`, count), newest day first.
pub fn days(c: &mut impl GenericClient, who: Viewer, f: &Filter) -> Result<Vec<(String, i64)>, Error> {
    let params = q::TimelineDaysParams {
        uid: who.uid,
        all: who.all,
        scope_user: who.scope_user,
        scope: f.scope.as_str(),
        device: f.device,
        kind: f.kind.map(Media::as_str),
        tag: f.tag,
        month: f.month,
    };
    Ok(q::timeline_days().params(&mut Pg(c), &params).all()?.into_iter().map(|d| (d.day, d.n)).collect())
}

/// The files of a view, newest first, at most `limit`.
pub fn assets(c: &mut impl GenericClient, who: Viewer, f: &Filter, limit: i64) -> Result<Vec<Asset>, Error> {
    let params = q::TimelineAssetsParams {
        uid: who.uid,
        all: who.all,
        scope_user: who.scope_user,
        scope: f.scope.as_str(),
        device: f.device,
        kind: f.kind.map(Media::as_str),
        tag: f.tag,
        month: f.month,
        limit,
    };
    q::timeline_assets().params(&mut Pg(c), &params).all()
}

/// Some files by id, as long as they are not deleted and the viewer can see them.
pub fn by_ids(c: &mut impl GenericClient, who: Viewer, ids: &[Uuid]) -> Result<Vec<Asset>, Error> {
    let params = q::AssetsByIdsParams { uid: who.uid, all: who.all, scope_user: who.scope_user, ids };
    q::assets_by_ids().params(&mut Pg(c), &params).all()
}

/// What is in an album, at most `limit`: newest first, or by path (for the caller to sort the way people read names).
pub fn album_assets(
    c: &mut impl GenericClient,
    who: Viewer,
    album_id: Uuid,
    by_path: bool,
    limit: i64,
) -> Result<Vec<Asset>, Error> {
    let params =
        q::AlbumAssetsParams { uid: who.uid, all: who.all, scope_user: who.scope_user, album_id, by_path, limit };
    q::album_assets().params(&mut Pg(c), &params).all()
}

/// What the detail view shows of a file besides the asset itself.
pub type Extras = q::AssetExtras;

/// What the camera said about a file before any correction, whether the viewer corrected the time or the place,
/// and the viewer's description. `None` when there is no such file.
pub fn extras(c: &mut impl GenericClient, uid: i64, id: Uuid) -> Result<Option<Extras>, Error> {
    q::asset_extras().params(&mut Pg(c), &q::AssetExtrasParams { uid, id }).opt()
}

/// A deleted file with the version that would bring it back.
pub type Trashed = q::TrashPage;

/// One page of the deleted files the viewer can see, newest deletion first.
pub fn trash(c: &mut impl GenericClient, who: Viewer, limit: i64, offset: i64) -> Result<Vec<Trashed>, Error> {
    let params = q::TrashPageParams { all: who.all, scope_user: who.scope_user, limit, offset };
    q::trash_page().params(&mut Pg(c), &params).all()
}

/// The photos and videos directly in the folder of a file.
pub fn folder_of(c: &mut impl GenericClient, who: Viewer, id: Uuid, limit: i64) -> Result<Vec<Asset>, Error> {
    let params = q::FolderAssetsParams { uid: who.uid, all: who.all, scope_user: who.scope_user, id, limit };
    q::folder_assets().params(&mut Pg(c), &params).all()
}
