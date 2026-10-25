// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct TimelineDaysParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql, T4: crate::StringSql> {
    pub uid: i64,
    pub all: bool,
    pub scope_user: i64,
    pub scope: T1,
    pub device: Option<T2>,
    pub kind: Option<T3>,
    pub tag: Option<i64>,
    pub month: Option<T4>,
}
#[derive(Debug)]
pub struct TimelineAssetsParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql, T4: crate::StringSql>
{
    pub uid: i64,
    pub all: bool,
    pub scope_user: i64,
    pub scope: T1,
    pub device: Option<T2>,
    pub kind: Option<T3>,
    pub tag: Option<i64>,
    pub month: Option<T4>,
    pub limit: i64,
}
#[derive(Debug)]
pub struct MapPointsParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql, T4: crate::StringSql> {
    pub uid: i64,
    pub all: bool,
    pub scope_user: i64,
    pub scope: T1,
    pub device: Option<T2>,
    pub kind: Option<T3>,
    pub tag: Option<i64>,
    pub month: Option<T4>,
    pub limit: i64,
}
#[derive(Debug)]
pub struct AssetsByIdsParams<T1: crate::ArraySql<Item = i64>> {
    pub uid: i64,
    pub ids: T1,
    pub all: bool,
    pub scope_user: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct AlbumAssetsParams {
    pub uid: i64,
    pub all: bool,
    pub scope_user: i64,
    pub album_id: i64,
    pub by_path: bool,
    pub limit: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct AssetExtrasParams {
    pub uid: i64,
    pub id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct TrashPageParams {
    pub all: bool,
    pub scope_user: i64,
    pub limit: i64,
    pub offset: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct VisibleFileExistsParams {
    pub id: i64,
    pub all: bool,
    pub scope_user: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct EnsureFileStateParams {
    pub file_id: i64,
    pub uid: i64,
    pub now: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct SetFavoriteParams {
    pub value: bool,
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct SetArchivedParams {
    pub value: bool,
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct SetRatingParams {
    pub rating: Option<i16>,
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Debug)]
pub struct SetDescriptionParams<T1: crate::StringSql> {
    pub description: T1,
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct SetTakenAtParams {
    pub taken_at: Option<i64>,
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct SetPlaceParams {
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct TouchFileStateParams {
    pub now: i64,
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Debug)]
pub struct BulkSetFavoriteParams<T1: crate::ArraySql<Item = i64>> {
    pub uid: i64,
    pub now: i64,
    pub value: bool,
    pub ids: T1,
    pub all: bool,
    pub scope_user: i64,
}
#[derive(Debug)]
pub struct BulkSetArchivedParams<T1: crate::ArraySql<Item = i64>> {
    pub uid: i64,
    pub now: i64,
    pub value: bool,
    pub ids: T1,
    pub all: bool,
    pub scope_user: i64,
}
#[derive(Debug)]
pub struct BulkAddTagsParams<T1: crate::ArraySql<Item = i64>, T2: crate::ArraySql<Item = i64>> {
    pub tag_ids: T1,
    pub ids: T2,
    pub all: bool,
    pub scope_user: i64,
}
#[derive(Debug)]
pub struct UpsertTagParams<T1: crate::StringSql> {
    pub name: T1,
    pub uid: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct TagsOfFileParams {
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Debug)]
pub struct KeepOnlyTagsParams<T1: crate::ArraySql<Item = i64>> {
    pub file_id: i64,
    pub tag_ids: T1,
    pub uid: i64,
}
#[derive(Debug)]
pub struct TagFileParams<T1: crate::ArraySql<Item = i64>> {
    pub file_id: i64,
    pub tag_ids: T1,
}
#[derive(Clone, Copy, Debug)]
pub struct ListTagsParams {
    pub all: bool,
    pub scope_user: i64,
    pub uid: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct DeleteTagParams {
    pub id: i64,
    pub uid: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineDays {
    pub day: String,
    pub n: i64,
}
pub struct TimelineDaysBorrowed<'a> {
    pub day: &'a str,
    pub n: i64,
}
impl<'a> From<TimelineDaysBorrowed<'a>> for TimelineDays {
    fn from(TimelineDaysBorrowed { day, n }: TimelineDaysBorrowed<'a>) -> Self {
        Self { day: day.into(), n }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub id: i64,
    pub folder: String,
    pub path: String,
    pub size: i64,
    pub root_hash: Vec<u8>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub shown_at: i64,
    pub favorite: bool,
    pub archived: bool,
    pub rating: Option<i16>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}
pub struct AssetBorrowed<'a> {
    pub id: i64,
    pub folder: &'a str,
    pub path: &'a str,
    pub size: i64,
    pub root_hash: &'a [u8],
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub shown_at: i64,
    pub favorite: bool,
    pub archived: bool,
    pub rating: Option<i16>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}
impl<'a> From<AssetBorrowed<'a>> for Asset {
    fn from(
        AssetBorrowed {
            id,
            folder,
            path,
            size,
            root_hash,
            width,
            height,
            duration_ms,
            shown_at,
            favorite,
            archived,
            rating,
            lat,
            lon,
        }: AssetBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            folder: folder.into(),
            path: path.into(),
            size,
            root_hash: root_hash.into(),
            width,
            height,
            duration_ms,
            shown_at,
            favorite,
            archived,
            rating,
            lat,
            lon,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct MapPoints {
    pub id: i64,
    pub lat: f64,
    pub lon: f64,
    pub shown_at: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct AssetExtras {
    pub mime: Option<String>,
    pub description: String,
    pub camera_when: Option<i64>,
    pub camera_lat: Option<f64>,
    pub camera_lon: Option<f64>,
    pub when_edited: bool,
    pub place_edited: bool,
    pub exif: Option<String>,
    pub codec: Option<String>,
    pub changed: i64,
}
pub struct AssetExtrasBorrowed<'a> {
    pub mime: Option<&'a str>,
    pub description: &'a str,
    pub camera_when: Option<i64>,
    pub camera_lat: Option<f64>,
    pub camera_lon: Option<f64>,
    pub when_edited: bool,
    pub place_edited: bool,
    pub exif: Option<&'a str>,
    pub codec: Option<&'a str>,
    pub changed: i64,
}
impl<'a> From<AssetExtrasBorrowed<'a>> for AssetExtras {
    fn from(
        AssetExtrasBorrowed {
            mime,
            description,
            camera_when,
            camera_lat,
            camera_lon,
            when_edited,
            place_edited,
            exif,
            codec,
            changed,
        }: AssetExtrasBorrowed<'a>,
    ) -> Self {
        Self {
            mime: mime.map(|v| v.into()),
            description: description.into(),
            camera_when,
            camera_lat,
            camera_lon,
            when_edited,
            place_edited,
            exif: exif.map(|v| v.into()),
            codec: codec.map(|v| v.into()),
            changed,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct TrashPage {
    pub id: i64,
    pub folder: String,
    pub path: String,
    pub size: i64,
    pub deleted_at: i64,
    pub version_id: i64,
    pub root_hash: Vec<u8>,
    pub total: i64,
}
pub struct TrashPageBorrowed<'a> {
    pub id: i64,
    pub folder: &'a str,
    pub path: &'a str,
    pub size: i64,
    pub deleted_at: i64,
    pub version_id: i64,
    pub root_hash: &'a [u8],
    pub total: i64,
}
impl<'a> From<TrashPageBorrowed<'a>> for TrashPage {
    fn from(
        TrashPageBorrowed { id, folder, path, size, deleted_at, version_id, root_hash, total }: TrashPageBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            folder: folder.into(),
            path: path.into(),
            size,
            deleted_at,
            version_id,
            root_hash: root_hash.into(),
            total,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct TagsOfFile {
    pub id: i64,
    pub name: String,
}
pub struct TagsOfFileBorrowed<'a> {
    pub id: i64,
    pub name: &'a str,
}
impl<'a> From<TagsOfFileBorrowed<'a>> for TagsOfFile {
    fn from(TagsOfFileBorrowed { id, name }: TagsOfFileBorrowed<'a>) -> Self {
        Self { id, name: name.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListTags {
    pub id: i64,
    pub name: String,
    pub files: i64,
}
pub struct ListTagsBorrowed<'a> {
    pub id: i64,
    pub name: &'a str,
    pub files: i64,
}
impl<'a> From<ListTagsBorrowed<'a>> for ListTags {
    fn from(ListTagsBorrowed { id, name, files }: ListTagsBorrowed<'a>) -> Self {
        Self { id, name: name.into(), files }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotAlbums {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    pub owner: String,
}
pub struct SnapshotAlbumsBorrowed<'a> {
    pub id: i64,
    pub name: &'a str,
    pub description: &'a str,
    pub created_at: i64,
    pub owner: &'a str,
}
impl<'a> From<SnapshotAlbumsBorrowed<'a>> for SnapshotAlbums {
    fn from(SnapshotAlbumsBorrowed { id, name, description, created_at, owner }: SnapshotAlbumsBorrowed<'a>) -> Self {
        Self { id, name: name.into(), description: description.into(), created_at, owner: owner.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotAlbumFiles {
    pub album_id: i64,
    pub folder: String,
    pub path: String,
}
pub struct SnapshotAlbumFilesBorrowed<'a> {
    pub album_id: i64,
    pub folder: &'a str,
    pub path: &'a str,
}
impl<'a> From<SnapshotAlbumFilesBorrowed<'a>> for SnapshotAlbumFiles {
    fn from(SnapshotAlbumFilesBorrowed { album_id, folder, path }: SnapshotAlbumFilesBorrowed<'a>) -> Self {
        Self { album_id, folder: folder.into(), path: path.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotTags {
    pub id: i64,
    pub name: String,
    pub owner: String,
}
pub struct SnapshotTagsBorrowed<'a> {
    pub id: i64,
    pub name: &'a str,
    pub owner: &'a str,
}
impl<'a> From<SnapshotTagsBorrowed<'a>> for SnapshotTags {
    fn from(SnapshotTagsBorrowed { id, name, owner }: SnapshotTagsBorrowed<'a>) -> Self {
        Self { id, name: name.into(), owner: owner.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotTagFiles {
    pub tag_id: i64,
    pub folder: String,
    pub path: String,
}
pub struct SnapshotTagFilesBorrowed<'a> {
    pub tag_id: i64,
    pub folder: &'a str,
    pub path: &'a str,
}
impl<'a> From<SnapshotTagFilesBorrowed<'a>> for SnapshotTagFiles {
    fn from(SnapshotTagFilesBorrowed { tag_id, folder, path }: SnapshotTagFilesBorrowed<'a>) -> Self {
        Self { tag_id, folder: folder.into(), path: path.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotStates {
    pub folder: String,
    pub path: String,
    pub favorite: bool,
    pub archived: bool,
    pub rating: Option<i16>,
    pub description: String,
    pub taken_at: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub owner: String,
}
pub struct SnapshotStatesBorrowed<'a> {
    pub folder: &'a str,
    pub path: &'a str,
    pub favorite: bool,
    pub archived: bool,
    pub rating: Option<i16>,
    pub description: &'a str,
    pub taken_at: Option<i64>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub owner: &'a str,
}
impl<'a> From<SnapshotStatesBorrowed<'a>> for SnapshotStates {
    fn from(
        SnapshotStatesBorrowed {
            folder,
            path,
            favorite,
            archived,
            rating,
            description,
            taken_at,
            lat,
            lon,
            owner,
        }: SnapshotStatesBorrowed<'a>,
    ) -> Self {
        Self {
            folder: folder.into(),
            path: path.into(),
            favorite,
            archived,
            rating,
            description: description.into(),
            taken_at,
            lat,
            lon,
            owner: owner.into(),
        }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct TimelineDaysQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<TimelineDaysBorrowed, postgres::Error>,
    mapper: fn(TimelineDaysBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> TimelineDaysQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(TimelineDaysBorrowed) -> R) -> TimelineDaysQuery<'c, 'a, 's, C, R, N> {
        TimelineDaysQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct AssetQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<AssetBorrowed, postgres::Error>,
    mapper: fn(AssetBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> AssetQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(AssetBorrowed) -> R) -> AssetQuery<'c, 'a, 's, C, R, N> {
        AssetQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct MapPointsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<MapPoints, postgres::Error>,
    mapper: fn(MapPoints) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> MapPointsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(MapPoints) -> R) -> MapPointsQuery<'c, 'a, 's, C, R, N> {
        MapPointsQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct AssetExtrasQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<AssetExtrasBorrowed, postgres::Error>,
    mapper: fn(AssetExtrasBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> AssetExtrasQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(AssetExtrasBorrowed) -> R) -> AssetExtrasQuery<'c, 'a, 's, C, R, N> {
        AssetExtrasQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct TrashPageQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<TrashPageBorrowed, postgres::Error>,
    mapper: fn(TrashPageBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> TrashPageQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(TrashPageBorrowed) -> R) -> TrashPageQuery<'c, 'a, 's, C, R, N> {
        TrashPageQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct BoolQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<bool, postgres::Error>,
    mapper: fn(bool) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> BoolQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(bool) -> R) -> BoolQuery<'c, 'a, 's, C, R, N> {
        BoolQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct I64Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<i64, postgres::Error>,
    mapper: fn(i64) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> I64Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(i64) -> R) -> I64Query<'c, 'a, 's, C, R, N> {
        I64Query {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct TagsOfFileQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<TagsOfFileBorrowed, postgres::Error>,
    mapper: fn(TagsOfFileBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> TagsOfFileQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(TagsOfFileBorrowed) -> R) -> TagsOfFileQuery<'c, 'a, 's, C, R, N> {
        TagsOfFileQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct ListTagsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ListTagsBorrowed, postgres::Error>,
    mapper: fn(ListTagsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListTagsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ListTagsBorrowed) -> R) -> ListTagsQuery<'c, 'a, 's, C, R, N> {
        ListTagsQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct SnapshotAlbumsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SnapshotAlbumsBorrowed, postgres::Error>,
    mapper: fn(SnapshotAlbumsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SnapshotAlbumsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SnapshotAlbumsBorrowed) -> R) -> SnapshotAlbumsQuery<'c, 'a, 's, C, R, N> {
        SnapshotAlbumsQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct SnapshotAlbumFilesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SnapshotAlbumFilesBorrowed, postgres::Error>,
    mapper: fn(SnapshotAlbumFilesBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SnapshotAlbumFilesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SnapshotAlbumFilesBorrowed) -> R) -> SnapshotAlbumFilesQuery<'c, 'a, 's, C, R, N> {
        SnapshotAlbumFilesQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct SnapshotTagsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SnapshotTagsBorrowed, postgres::Error>,
    mapper: fn(SnapshotTagsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SnapshotTagsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SnapshotTagsBorrowed) -> R) -> SnapshotTagsQuery<'c, 'a, 's, C, R, N> {
        SnapshotTagsQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct SnapshotTagFilesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SnapshotTagFilesBorrowed, postgres::Error>,
    mapper: fn(SnapshotTagFilesBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SnapshotTagFilesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SnapshotTagFilesBorrowed) -> R) -> SnapshotTagFilesQuery<'c, 'a, 's, C, R, N> {
        SnapshotTagFilesQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct SnapshotStatesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SnapshotStatesBorrowed, postgres::Error>,
    mapper: fn(SnapshotStatesBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SnapshotStatesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SnapshotStatesBorrowed) -> R) -> SnapshotStatesQuery<'c, 'a, 's, C, R, N> {
        SnapshotStatesQuery {
            client: self.client,
            params: self.params,
            query: self.query,
            cached: self.cached,
            extractor: self.extractor,
            mapper,
        }
    }
    pub fn one(self) -> Result<T, postgres::Error> {
        let row = crate::client::sync::one(self.client, self.query, &self.params, self.cached)?;
        Ok((self.mapper)((self.extractor)(&row)?))
    }
    pub fn all(self) -> Result<Vec<T>, postgres::Error> {
        self.iter()?.collect()
    }
    pub fn opt(self) -> Result<Option<T>, postgres::Error> {
        let opt_row = crate::client::sync::opt(self.client, self.query, &self.params, self.cached)?;
        Ok(opt_row
            .map(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
            .transpose()?)
    }
    pub fn iter(self) -> Result<impl Iterator<Item = Result<T, postgres::Error>> + 'c, postgres::Error> {
        let stream = crate::client::sync::raw(self.client, self.query, crate::slice_iter(&self.params), self.cached)?;
        let mapped = stream.iterator().map(move |res| {
            res.and_then(|row| {
                let extracted = (self.extractor)(&row)?;
                Ok((self.mapper)(extracted))
            })
        });
        Ok(mapped)
    }
}
pub struct TimelineDaysStmt(&'static str, Option<postgres::Statement>);
pub fn timeline_days() -> TimelineDaysStmt {
    TimelineDaysStmt(
        "SELECT to_char(to_timestamp(COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)) AT TIME ZONE 'UTC', 'YYYY-MM-DD') AS day, COUNT(*) AS n FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = $1 WHERE f.deleted_at IS NULL AND f.media IS NOT NULL AND ($2::boolean OR d.kind = 'master' OR d.user_id = $3::bigint) AND CASE $4::text WHEN 'timeline' THEN NOT COALESCE(s.archived, false) WHEN 'favorites' THEN COALESCE(s.favorite, false) WHEN 'archive' THEN COALESCE(s.archived, false) END AND ($5::text IS NULL OR d.folder = $5) AND ($6::text IS NULL OR f.media = $6) AND ($7::bigint IS NULL OR EXISTS (SELECT 1 FROM file_tags t WHERE t.file_id = f.id AND t.tag_id = $7)) AND ($8::text IS NULL OR to_char(to_timestamp(COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)) AT TIME ZONE 'UTC', 'YYYY-MM') = $8) GROUP BY 1 ORDER BY 1 DESC",
        None,
    )
}
impl TimelineDaysStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::StringSql,
    >(
        &'s self,
        client: &'c mut C,
        uid: &'a i64,
        all: &'a bool,
        scope_user: &'a i64,
        scope: &'a T1,
        device: &'a Option<T2>,
        kind: &'a Option<T3>,
        tag: &'a Option<i64>,
        month: &'a Option<T4>,
    ) -> TimelineDaysQuery<'c, 'a, 's, C, TimelineDays, 8> {
        TimelineDaysQuery {
            client,
            params: [uid, all, scope_user, scope, device, kind, tag, month],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<TimelineDaysBorrowed, postgres::Error> {
                Ok(TimelineDaysBorrowed { day: row.try_get(0)?, n: row.try_get(1)? })
            },
            mapper: |it| TimelineDays::from(it),
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        TimelineDaysParams<T1, T2, T3, T4>,
        TimelineDaysQuery<'c, 'a, 's, C, TimelineDays, 8>,
        C,
    > for TimelineDaysStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a TimelineDaysParams<T1, T2, T3, T4>,
    ) -> TimelineDaysQuery<'c, 'a, 's, C, TimelineDays, 8> {
        self.bind(
            client,
            &params.uid,
            &params.all,
            &params.scope_user,
            &params.scope,
            &params.device,
            &params.kind,
            &params.tag,
            &params.month,
        )
    }
}
pub struct TimelineAssetsStmt(&'static str, Option<postgres::Statement>);
pub fn timeline_assets() -> TimelineAssetsStmt {
    TimelineAssetsStmt(
        "SELECT f.id, d.folder, f.path, v.size, v.root_hash, m.width, m.height, m.duration_ms, COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) AS shown_at, COALESCE(s.favorite, false) AS favorite, COALESCE(s.archived, false) AS archived, s.rating, COALESCE(s.lat, m.lat) AS lat, COALESCE(s.lon, m.lon) AS lon FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = $1 WHERE f.deleted_at IS NULL AND f.media IS NOT NULL AND ($2::boolean OR d.kind = 'master' OR d.user_id = $3::bigint) AND CASE $4::text WHEN 'timeline' THEN NOT COALESCE(s.archived, false) WHEN 'favorites' THEN COALESCE(s.favorite, false) WHEN 'archive' THEN COALESCE(s.archived, false) END AND ($5::text IS NULL OR d.folder = $5) AND ($6::text IS NULL OR f.media = $6) AND ($7::bigint IS NULL OR EXISTS (SELECT 1 FROM file_tags t WHERE t.file_id = f.id AND t.tag_id = $7)) AND ($8::text IS NULL OR to_char(to_timestamp(COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)) AT TIME ZONE 'UTC', 'YYYY-MM') = $8) ORDER BY shown_at DESC, f.id DESC LIMIT $9",
        None,
    )
}
impl TimelineAssetsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::StringSql,
    >(
        &'s self,
        client: &'c mut C,
        uid: &'a i64,
        all: &'a bool,
        scope_user: &'a i64,
        scope: &'a T1,
        device: &'a Option<T2>,
        kind: &'a Option<T3>,
        tag: &'a Option<i64>,
        month: &'a Option<T4>,
        limit: &'a i64,
    ) -> AssetQuery<'c, 'a, 's, C, Asset, 9> {
        AssetQuery {
            client,
            params: [uid, all, scope_user, scope, device, kind, tag, month, limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<AssetBorrowed, postgres::Error> {
                Ok(AssetBorrowed {
                    id: row.try_get(0)?,
                    folder: row.try_get(1)?,
                    path: row.try_get(2)?,
                    size: row.try_get(3)?,
                    root_hash: row.try_get(4)?,
                    width: row.try_get(5)?,
                    height: row.try_get(6)?,
                    duration_ms: row.try_get(7)?,
                    shown_at: row.try_get(8)?,
                    favorite: row.try_get(9)?,
                    archived: row.try_get(10)?,
                    rating: row.try_get(11)?,
                    lat: row.try_get(12)?,
                    lon: row.try_get(13)?,
                })
            },
            mapper: |it| Asset::from(it),
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
> crate::client::sync::Params<'c, 'a, 's, TimelineAssetsParams<T1, T2, T3, T4>, AssetQuery<'c, 'a, 's, C, Asset, 9>, C>
    for TimelineAssetsStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a TimelineAssetsParams<T1, T2, T3, T4>,
    ) -> AssetQuery<'c, 'a, 's, C, Asset, 9> {
        self.bind(
            client,
            &params.uid,
            &params.all,
            &params.scope_user,
            &params.scope,
            &params.device,
            &params.kind,
            &params.tag,
            &params.month,
            &params.limit,
        )
    }
}
pub struct MapPointsStmt(&'static str, Option<postgres::Statement>);
pub fn map_points() -> MapPointsStmt {
    MapPointsStmt(
        "SELECT f.id, COALESCE(s.lat, m.lat) AS lat, COALESCE(s.lon, m.lon) AS lon, COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) AS shown_at FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = $1 WHERE f.deleted_at IS NULL AND f.media IS NOT NULL AND ($2::boolean OR d.kind = 'master' OR d.user_id = $3::bigint) AND CASE $4::text WHEN 'timeline' THEN NOT COALESCE(s.archived, false) WHEN 'favorites' THEN COALESCE(s.favorite, false) WHEN 'archive' THEN COALESCE(s.archived, false) END AND ($5::text IS NULL OR d.folder = $5) AND ($6::text IS NULL OR f.media = $6) AND ($7::bigint IS NULL OR EXISTS (SELECT 1 FROM file_tags t WHERE t.file_id = f.id AND t.tag_id = $7)) AND ($8::text IS NULL OR to_char(to_timestamp(COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000)) AT TIME ZONE 'UTC', 'YYYY-MM') = $8) AND COALESCE(s.lat, m.lat) IS NOT NULL ORDER BY shown_at DESC LIMIT $9",
        None,
    )
}
impl MapPointsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<
        'c,
        'a,
        's,
        C: GenericClient,
        T1: crate::StringSql,
        T2: crate::StringSql,
        T3: crate::StringSql,
        T4: crate::StringSql,
    >(
        &'s self,
        client: &'c mut C,
        uid: &'a i64,
        all: &'a bool,
        scope_user: &'a i64,
        scope: &'a T1,
        device: &'a Option<T2>,
        kind: &'a Option<T3>,
        tag: &'a Option<i64>,
        month: &'a Option<T4>,
        limit: &'a i64,
    ) -> MapPointsQuery<'c, 'a, 's, C, MapPoints, 9> {
        MapPointsQuery {
            client,
            params: [uid, all, scope_user, scope, device, kind, tag, month, limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<MapPoints, postgres::Error> {
                Ok(MapPoints {
                    id: row.try_get(0)?,
                    lat: row.try_get(1)?,
                    lon: row.try_get(2)?,
                    shown_at: row.try_get(3)?,
                })
            },
            mapper: |it| MapPoints::from(it),
        }
    }
}
impl<
    'c,
    'a,
    's,
    C: GenericClient,
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        MapPointsParams<T1, T2, T3, T4>,
        MapPointsQuery<'c, 'a, 's, C, MapPoints, 9>,
        C,
    > for MapPointsStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a MapPointsParams<T1, T2, T3, T4>,
    ) -> MapPointsQuery<'c, 'a, 's, C, MapPoints, 9> {
        self.bind(
            client,
            &params.uid,
            &params.all,
            &params.scope_user,
            &params.scope,
            &params.device,
            &params.kind,
            &params.tag,
            &params.month,
            &params.limit,
        )
    }
}
pub struct AssetsByIdsStmt(&'static str, Option<postgres::Statement>);
pub fn assets_by_ids() -> AssetsByIdsStmt {
    AssetsByIdsStmt(
        "SELECT f.id, d.folder, f.path, v.size, v.root_hash, m.width, m.height, m.duration_ms, COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) AS shown_at, COALESCE(s.favorite, false) AS favorite, COALESCE(s.archived, false) AS archived, s.rating, COALESCE(s.lat, m.lat) AS lat, COALESCE(s.lon, m.lon) AS lon FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = $1 WHERE f.id = ANY($2) AND f.deleted_at IS NULL AND ($3::boolean OR d.kind = 'master' OR d.user_id = $4::bigint) ORDER BY shown_at DESC, f.id DESC",
        None,
    )
}
impl AssetsByIdsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        uid: &'a i64,
        ids: &'a T1,
        all: &'a bool,
        scope_user: &'a i64,
    ) -> AssetQuery<'c, 'a, 's, C, Asset, 4> {
        AssetQuery {
            client,
            params: [uid, ids, all, scope_user],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<AssetBorrowed, postgres::Error> {
                Ok(AssetBorrowed {
                    id: row.try_get(0)?,
                    folder: row.try_get(1)?,
                    path: row.try_get(2)?,
                    size: row.try_get(3)?,
                    root_hash: row.try_get(4)?,
                    width: row.try_get(5)?,
                    height: row.try_get(6)?,
                    duration_ms: row.try_get(7)?,
                    shown_at: row.try_get(8)?,
                    favorite: row.try_get(9)?,
                    archived: row.try_get(10)?,
                    rating: row.try_get(11)?,
                    lat: row.try_get(12)?,
                    lon: row.try_get(13)?,
                })
            },
            mapper: |it| Asset::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, AssetsByIdsParams<T1>, AssetQuery<'c, 'a, 's, C, Asset, 4>, C>
    for AssetsByIdsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a AssetsByIdsParams<T1>) -> AssetQuery<'c, 'a, 's, C, Asset, 4> {
        self.bind(client, &params.uid, &params.ids, &params.all, &params.scope_user)
    }
}
pub struct AlbumAssetsStmt(&'static str, Option<postgres::Statement>);
pub fn album_assets() -> AlbumAssetsStmt {
    AlbumAssetsStmt(
        "SELECT f.id, d.folder, f.path, v.size, v.root_hash, m.width, m.height, m.duration_ms, COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) AS shown_at, COALESCE(s.favorite, false) AS favorite, COALESCE(s.archived, false) AS archived, s.rating, COALESCE(s.lat, m.lat) AS lat, COALESCE(s.lon, m.lon) AS lon FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = $1 WHERE f.deleted_at IS NULL AND f.media IS NOT NULL AND ($2::boolean OR d.kind = 'master' OR d.user_id = $3::bigint) AND EXISTS (SELECT 1 FROM albums a WHERE a.id = $4 AND (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id) OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))) ORDER BY CASE WHEN $5::boolean THEN f.path END, shown_at DESC, f.id DESC LIMIT $6",
        None,
    )
}
impl AlbumAssetsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        uid: &'a i64,
        all: &'a bool,
        scope_user: &'a i64,
        album_id: &'a i64,
        by_path: &'a bool,
        limit: &'a i64,
    ) -> AssetQuery<'c, 'a, 's, C, Asset, 6> {
        AssetQuery {
            client,
            params: [uid, all, scope_user, album_id, by_path, limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<AssetBorrowed, postgres::Error> {
                Ok(AssetBorrowed {
                    id: row.try_get(0)?,
                    folder: row.try_get(1)?,
                    path: row.try_get(2)?,
                    size: row.try_get(3)?,
                    root_hash: row.try_get(4)?,
                    width: row.try_get(5)?,
                    height: row.try_get(6)?,
                    duration_ms: row.try_get(7)?,
                    shown_at: row.try_get(8)?,
                    favorite: row.try_get(9)?,
                    archived: row.try_get(10)?,
                    rating: row.try_get(11)?,
                    lat: row.try_get(12)?,
                    lon: row.try_get(13)?,
                })
            },
            mapper: |it| Asset::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, AlbumAssetsParams, AssetQuery<'c, 'a, 's, C, Asset, 6>, C>
    for AlbumAssetsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a AlbumAssetsParams) -> AssetQuery<'c, 'a, 's, C, Asset, 6> {
        self.bind(
            client,
            &params.uid,
            &params.all,
            &params.scope_user,
            &params.album_id,
            &params.by_path,
            &params.limit,
        )
    }
}
pub struct AssetExtrasStmt(&'static str, Option<postgres::Statement>);
pub fn asset_extras() -> AssetExtrasStmt {
    AssetExtrasStmt(
        "SELECT m.mime, COALESCE(s.description, '') AS description, m.taken_at AS camera_when, m.lat AS camera_lat, m.lon AS camera_lon, s.taken_at IS NOT NULL AS when_edited, s.lat IS NOT NULL AS place_edited, m.exif, m.codec, v.created_at AS changed FROM files f JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = $1 WHERE f.id = $2",
        None,
    )
}
impl AssetExtrasStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        uid: &'a i64,
        id: &'a i64,
    ) -> AssetExtrasQuery<'c, 'a, 's, C, AssetExtras, 2> {
        AssetExtrasQuery {
            client,
            params: [uid, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<AssetExtrasBorrowed, postgres::Error> {
                Ok(AssetExtrasBorrowed {
                    mime: row.try_get(0)?,
                    description: row.try_get(1)?,
                    camera_when: row.try_get(2)?,
                    camera_lat: row.try_get(3)?,
                    camera_lon: row.try_get(4)?,
                    when_edited: row.try_get(5)?,
                    place_edited: row.try_get(6)?,
                    exif: row.try_get(7)?,
                    codec: row.try_get(8)?,
                    changed: row.try_get(9)?,
                })
            },
            mapper: |it| AssetExtras::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, AssetExtrasParams, AssetExtrasQuery<'c, 'a, 's, C, AssetExtras, 2>, C>
    for AssetExtrasStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a AssetExtrasParams,
    ) -> AssetExtrasQuery<'c, 'a, 's, C, AssetExtras, 2> {
        self.bind(client, &params.uid, &params.id)
    }
}
pub struct TrashPageStmt(&'static str, Option<postgres::Statement>);
pub fn trash_page() -> TrashPageStmt {
    TrashPageStmt(
        "SELECT f.id, d.folder, f.path, v.size, f.deleted_at AS deleted_at, v.id AS version_id, v.root_hash, COUNT(*) OVER () AS total FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id WHERE f.deleted_at IS NOT NULL AND ($1::boolean OR d.kind = 'master' OR d.user_id = $2::bigint) ORDER BY f.deleted_at DESC, f.id DESC LIMIT $3 OFFSET $4",
        None,
    )
}
impl TrashPageStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        all: &'a bool,
        scope_user: &'a i64,
        limit: &'a i64,
        offset: &'a i64,
    ) -> TrashPageQuery<'c, 'a, 's, C, TrashPage, 4> {
        TrashPageQuery {
            client,
            params: [all, scope_user, limit, offset],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<TrashPageBorrowed, postgres::Error> {
                Ok(TrashPageBorrowed {
                    id: row.try_get(0)?,
                    folder: row.try_get(1)?,
                    path: row.try_get(2)?,
                    size: row.try_get(3)?,
                    deleted_at: row.try_get(4)?,
                    version_id: row.try_get(5)?,
                    root_hash: row.try_get(6)?,
                    total: row.try_get(7)?,
                })
            },
            mapper: |it| TrashPage::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, TrashPageParams, TrashPageQuery<'c, 'a, 's, C, TrashPage, 4>, C>
    for TrashPageStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a TrashPageParams) -> TrashPageQuery<'c, 'a, 's, C, TrashPage, 4> {
        self.bind(client, &params.all, &params.scope_user, &params.limit, &params.offset)
    }
}
pub struct VisibleFileExistsStmt(&'static str, Option<postgres::Statement>);
pub fn visible_file_exists() -> VisibleFileExistsStmt {
    VisibleFileExistsStmt(
        "SELECT EXISTS (SELECT 1 FROM files f JOIN devices d ON d.id = f.device_id WHERE f.id = $1 AND f.deleted_at IS NULL AND ($2::boolean OR d.kind = 'master' OR d.user_id = $3::bigint)) AS visible",
        None,
    )
}
impl VisibleFileExistsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        id: &'a i64,
        all: &'a bool,
        scope_user: &'a i64,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 3> {
        BoolQuery {
            client,
            params: [id, all, scope_user],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, VisibleFileExistsParams, BoolQuery<'c, 'a, 's, C, bool, 3>, C>
    for VisibleFileExistsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a VisibleFileExistsParams) -> BoolQuery<'c, 'a, 's, C, bool, 3> {
        self.bind(client, &params.id, &params.all, &params.scope_user)
    }
}
pub struct EnsureFileStateStmt(&'static str, Option<postgres::Statement>);
pub fn ensure_file_state() -> EnsureFileStateStmt {
    EnsureFileStateStmt(
        "INSERT INTO file_state (file_id, user_id, updated_at) VALUES ($1, $2, $3) ON CONFLICT (user_id, file_id) DO NOTHING",
        None,
    )
}
impl EnsureFileStateStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        file_id: &'a i64,
        uid: &'a i64,
        now: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[file_id, uid, now])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, EnsureFileStateParams, Result<u64, postgres::Error>, C>
    for EnsureFileStateStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a EnsureFileStateParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.file_id, &params.uid, &params.now)
    }
}
pub struct SetFavoriteStmt(&'static str, Option<postgres::Statement>);
pub fn set_favorite() -> SetFavoriteStmt {
    SetFavoriteStmt("UPDATE file_state SET favorite = $1 WHERE file_id = $2 AND user_id = $3", None)
}
impl SetFavoriteStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        value: &'a bool,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[value, file_id, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, SetFavoriteParams, Result<u64, postgres::Error>, C> for SetFavoriteStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetFavoriteParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.value, &params.file_id, &params.uid)
    }
}
pub struct SetArchivedStmt(&'static str, Option<postgres::Statement>);
pub fn set_archived() -> SetArchivedStmt {
    SetArchivedStmt("UPDATE file_state SET archived = $1 WHERE file_id = $2 AND user_id = $3", None)
}
impl SetArchivedStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        value: &'a bool,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[value, file_id, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, SetArchivedParams, Result<u64, postgres::Error>, C> for SetArchivedStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetArchivedParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.value, &params.file_id, &params.uid)
    }
}
pub struct SetRatingStmt(&'static str, Option<postgres::Statement>);
pub fn set_rating() -> SetRatingStmt {
    SetRatingStmt("UPDATE file_state SET rating = $1 WHERE file_id = $2 AND user_id = $3", None)
}
impl SetRatingStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        rating: &'a Option<i16>,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[rating, file_id, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, SetRatingParams, Result<u64, postgres::Error>, C> for SetRatingStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetRatingParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.rating, &params.file_id, &params.uid)
    }
}
pub struct SetDescriptionStmt(&'static str, Option<postgres::Statement>);
pub fn set_description() -> SetDescriptionStmt {
    SetDescriptionStmt("UPDATE file_state SET description = $1 WHERE file_id = $2 AND user_id = $3", None)
}
impl SetDescriptionStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        description: &'a T1,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[description, file_id, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, SetDescriptionParams<T1>, Result<u64, postgres::Error>, C>
    for SetDescriptionStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetDescriptionParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.description, &params.file_id, &params.uid)
    }
}
pub struct SetTakenAtStmt(&'static str, Option<postgres::Statement>);
pub fn set_taken_at() -> SetTakenAtStmt {
    SetTakenAtStmt("UPDATE file_state SET taken_at = $1 WHERE file_id = $2 AND user_id = $3", None)
}
impl SetTakenAtStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        taken_at: &'a Option<i64>,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[taken_at, file_id, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, SetTakenAtParams, Result<u64, postgres::Error>, C> for SetTakenAtStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetTakenAtParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.taken_at, &params.file_id, &params.uid)
    }
}
pub struct SetPlaceStmt(&'static str, Option<postgres::Statement>);
pub fn set_place() -> SetPlaceStmt {
    SetPlaceStmt("UPDATE file_state SET lat = $1, lon = $2 WHERE file_id = $3 AND user_id = $4", None)
}
impl SetPlaceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        lat: &'a Option<f64>,
        lon: &'a Option<f64>,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[lat, lon, file_id, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, SetPlaceParams, Result<u64, postgres::Error>, C> for SetPlaceStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetPlaceParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.lat, &params.lon, &params.file_id, &params.uid)
    }
}
pub struct TouchFileStateStmt(&'static str, Option<postgres::Statement>);
pub fn touch_file_state() -> TouchFileStateStmt {
    TouchFileStateStmt("UPDATE file_state SET updated_at = $1 WHERE file_id = $2 AND user_id = $3", None)
}
impl TouchFileStateStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now, file_id, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, TouchFileStateParams, Result<u64, postgres::Error>, C>
    for TouchFileStateStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a TouchFileStateParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.file_id, &params.uid)
    }
}
pub struct BulkSetFavoriteStmt(&'static str, Option<postgres::Statement>);
pub fn bulk_set_favorite() -> BulkSetFavoriteStmt {
    BulkSetFavoriteStmt(
        "INSERT INTO file_state (file_id, user_id, updated_at, favorite) SELECT f.id, $1, $2, $3 FROM files f JOIN devices d ON d.id = f.device_id WHERE f.id = ANY($4) AND f.deleted_at IS NULL AND ($5::boolean OR d.kind = 'master' OR d.user_id = $6::bigint) ON CONFLICT (user_id, file_id) DO UPDATE SET favorite = excluded.favorite, updated_at = excluded.updated_at",
        None,
    )
}
impl BulkSetFavoriteStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        uid: &'a i64,
        now: &'a i64,
        value: &'a bool,
        ids: &'a T1,
        all: &'a bool,
        scope_user: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[uid, now, value, ids, all, scope_user])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, BulkSetFavoriteParams<T1>, Result<u64, postgres::Error>, C>
    for BulkSetFavoriteStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a BulkSetFavoriteParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.uid, &params.now, &params.value, &params.ids, &params.all, &params.scope_user)
    }
}
pub struct BulkSetArchivedStmt(&'static str, Option<postgres::Statement>);
pub fn bulk_set_archived() -> BulkSetArchivedStmt {
    BulkSetArchivedStmt(
        "INSERT INTO file_state (file_id, user_id, updated_at, archived) SELECT f.id, $1, $2, $3 FROM files f JOIN devices d ON d.id = f.device_id WHERE f.id = ANY($4) AND f.deleted_at IS NULL AND ($5::boolean OR d.kind = 'master' OR d.user_id = $6::bigint) ON CONFLICT (user_id, file_id) DO UPDATE SET archived = excluded.archived, updated_at = excluded.updated_at",
        None,
    )
}
impl BulkSetArchivedStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        uid: &'a i64,
        now: &'a i64,
        value: &'a bool,
        ids: &'a T1,
        all: &'a bool,
        scope_user: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[uid, now, value, ids, all, scope_user])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, BulkSetArchivedParams<T1>, Result<u64, postgres::Error>, C>
    for BulkSetArchivedStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a BulkSetArchivedParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.uid, &params.now, &params.value, &params.ids, &params.all, &params.scope_user)
    }
}
pub struct BulkAddTagsStmt(&'static str, Option<postgres::Statement>);
pub fn bulk_add_tags() -> BulkAddTagsStmt {
    BulkAddTagsStmt(
        "INSERT INTO file_tags (file_id, tag_id) SELECT f.id, t FROM files f JOIN devices d ON d.id = f.device_id CROSS JOIN unnest($1::bigint[]) t WHERE f.id = ANY($2) AND f.deleted_at IS NULL AND ($3::boolean OR d.kind = 'master' OR d.user_id = $4::bigint) ON CONFLICT DO NOTHING",
        None,
    )
}
impl BulkAddTagsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>, T2: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        tag_ids: &'a T1,
        ids: &'a T2,
        all: &'a bool,
        scope_user: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[tag_ids, ids, all, scope_user])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>, T2: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, BulkAddTagsParams<T1, T2>, Result<u64, postgres::Error>, C>
    for BulkAddTagsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a BulkAddTagsParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.tag_ids, &params.ids, &params.all, &params.scope_user)
    }
}
pub struct UpsertTagStmt(&'static str, Option<postgres::Statement>);
pub fn upsert_tag() -> UpsertTagStmt {
    UpsertTagStmt(
        "INSERT INTO tags (name, user_id) VALUES ($1, $2) ON CONFLICT (user_id, (lower(name))) DO UPDATE SET name = tags.name RETURNING id",
        None,
    )
}
impl UpsertTagStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
        uid: &'a i64,
    ) -> I64Query<'c, 'a, 's, C, i64, 2> {
        I64Query {
            client,
            params: [name, uid],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, UpsertTagParams<T1>, I64Query<'c, 'a, 's, C, i64, 2>, C> for UpsertTagStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a UpsertTagParams<T1>) -> I64Query<'c, 'a, 's, C, i64, 2> {
        self.bind(client, &params.name, &params.uid)
    }
}
pub struct TagsOfFileStmt(&'static str, Option<postgres::Statement>);
pub fn tags_of_file() -> TagsOfFileStmt {
    TagsOfFileStmt(
        "SELECT t.id, t.name FROM file_tags ft JOIN tags t ON t.id = ft.tag_id WHERE ft.file_id = $1 AND t.user_id = $2 ORDER BY lower(t.name)",
        None,
    )
}
impl TagsOfFileStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> TagsOfFileQuery<'c, 'a, 's, C, TagsOfFile, 2> {
        TagsOfFileQuery {
            client,
            params: [file_id, uid],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<TagsOfFileBorrowed, postgres::Error> {
                Ok(TagsOfFileBorrowed { id: row.try_get(0)?, name: row.try_get(1)? })
            },
            mapper: |it| TagsOfFile::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, TagsOfFileParams, TagsOfFileQuery<'c, 'a, 's, C, TagsOfFile, 2>, C>
    for TagsOfFileStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a TagsOfFileParams,
    ) -> TagsOfFileQuery<'c, 'a, 's, C, TagsOfFile, 2> {
        self.bind(client, &params.file_id, &params.uid)
    }
}
pub struct KeepOnlyTagsStmt(&'static str, Option<postgres::Statement>);
pub fn keep_only_tags() -> KeepOnlyTagsStmt {
    KeepOnlyTagsStmt(
        "DELETE FROM file_tags WHERE file_id = $1 AND NOT (tag_id = ANY($2)) AND tag_id IN (SELECT id FROM tags WHERE user_id = $3)",
        None,
    )
}
impl KeepOnlyTagsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        file_id: &'a i64,
        tag_ids: &'a T1,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[file_id, tag_ids, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, KeepOnlyTagsParams<T1>, Result<u64, postgres::Error>, C>
    for KeepOnlyTagsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a KeepOnlyTagsParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.file_id, &params.tag_ids, &params.uid)
    }
}
pub struct TagFileStmt(&'static str, Option<postgres::Statement>);
pub fn tag_file() -> TagFileStmt {
    TagFileStmt("INSERT INTO file_tags (file_id, tag_id) SELECT $1, unnest($2::bigint[]) ON CONFLICT DO NOTHING", None)
}
impl TagFileStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        file_id: &'a i64,
        tag_ids: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[file_id, tag_ids])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, TagFileParams<T1>, Result<u64, postgres::Error>, C> for TagFileStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a TagFileParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.file_id, &params.tag_ids)
    }
}
pub struct DropUnusedTagsStmt(&'static str, Option<postgres::Statement>);
pub fn drop_unused_tags() -> DropUnusedTagsStmt {
    DropUnusedTagsStmt("DELETE FROM tags t WHERE NOT EXISTS (SELECT 1 FROM file_tags ft WHERE ft.tag_id = t.id)", None)
}
impl DropUnusedTagsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(&'s self, client: &'c mut C) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[])
    }
}
pub struct ListTagsStmt(&'static str, Option<postgres::Statement>);
pub fn list_tags() -> ListTagsStmt {
    ListTagsStmt(
        "SELECT t.id, t.name, COUNT(d.id) AS files FROM tags t LEFT JOIN file_tags ft ON ft.tag_id = t.id LEFT JOIN files f ON f.id = ft.file_id AND f.deleted_at IS NULL LEFT JOIN devices d ON d.id = f.device_id AND ($1::boolean OR d.kind = 'master' OR d.user_id = $2::bigint) WHERE t.user_id = $3 GROUP BY t.id ORDER BY lower(t.name)",
        None,
    )
}
impl ListTagsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        all: &'a bool,
        scope_user: &'a i64,
        uid: &'a i64,
    ) -> ListTagsQuery<'c, 'a, 's, C, ListTags, 3> {
        ListTagsQuery {
            client,
            params: [all, scope_user, uid],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ListTagsBorrowed, postgres::Error> {
                Ok(ListTagsBorrowed { id: row.try_get(0)?, name: row.try_get(1)?, files: row.try_get(2)? })
            },
            mapper: |it| ListTags::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, ListTagsParams, ListTagsQuery<'c, 'a, 's, C, ListTags, 3>, C>
    for ListTagsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a ListTagsParams) -> ListTagsQuery<'c, 'a, 's, C, ListTags, 3> {
        self.bind(client, &params.all, &params.scope_user, &params.uid)
    }
}
pub struct DeleteTagStmt(&'static str, Option<postgres::Statement>);
pub fn delete_tag() -> DeleteTagStmt {
    DeleteTagStmt("DELETE FROM tags WHERE id = $1 AND user_id = $2", None)
}
impl DeleteTagStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        id: &'a i64,
        uid: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[id, uid])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, DeleteTagParams, Result<u64, postgres::Error>, C> for DeleteTagStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a DeleteTagParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.id, &params.uid)
    }
}
pub struct SnapshotAlbumsStmt(&'static str, Option<postgres::Statement>);
pub fn snapshot_albums() -> SnapshotAlbumsStmt {
    SnapshotAlbumsStmt(
        "SELECT a.id, a.name, a.description, a.created_at, u.name AS owner FROM albums a JOIN users u ON u.id = a.user_id ORDER BY a.id",
        None,
    )
}
impl SnapshotAlbumsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> SnapshotAlbumsQuery<'c, 'a, 's, C, SnapshotAlbums, 0> {
        SnapshotAlbumsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SnapshotAlbumsBorrowed, postgres::Error> {
                Ok(SnapshotAlbumsBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    description: row.try_get(2)?,
                    created_at: row.try_get(3)?,
                    owner: row.try_get(4)?,
                })
            },
            mapper: |it| SnapshotAlbums::from(it),
        }
    }
}
pub struct SnapshotAlbumFilesStmt(&'static str, Option<postgres::Statement>);
pub fn snapshot_album_files() -> SnapshotAlbumFilesStmt {
    SnapshotAlbumFilesStmt(
        "SELECT af.album_id, d.folder, f.path FROM album_files af JOIN files f ON f.id = af.file_id JOIN devices d ON d.id = f.device_id ORDER BY af.album_id, f.path",
        None,
    )
}
impl SnapshotAlbumFilesStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> SnapshotAlbumFilesQuery<'c, 'a, 's, C, SnapshotAlbumFiles, 0> {
        SnapshotAlbumFilesQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SnapshotAlbumFilesBorrowed, postgres::Error> {
                Ok(SnapshotAlbumFilesBorrowed {
                    album_id: row.try_get(0)?,
                    folder: row.try_get(1)?,
                    path: row.try_get(2)?,
                })
            },
            mapper: |it| SnapshotAlbumFiles::from(it),
        }
    }
}
pub struct SnapshotTagsStmt(&'static str, Option<postgres::Statement>);
pub fn snapshot_tags() -> SnapshotTagsStmt {
    SnapshotTagsStmt(
        "SELECT t.id, t.name, u.name AS owner FROM tags t JOIN users u ON u.id = t.user_id ORDER BY t.id",
        None,
    )
}
impl SnapshotTagsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> SnapshotTagsQuery<'c, 'a, 's, C, SnapshotTags, 0> {
        SnapshotTagsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SnapshotTagsBorrowed, postgres::Error> {
                Ok(SnapshotTagsBorrowed { id: row.try_get(0)?, name: row.try_get(1)?, owner: row.try_get(2)? })
            },
            mapper: |it| SnapshotTags::from(it),
        }
    }
}
pub struct SnapshotTagFilesStmt(&'static str, Option<postgres::Statement>);
pub fn snapshot_tag_files() -> SnapshotTagFilesStmt {
    SnapshotTagFilesStmt(
        "SELECT ft.tag_id, d.folder, f.path FROM file_tags ft JOIN files f ON f.id = ft.file_id JOIN devices d ON d.id = f.device_id ORDER BY ft.tag_id, f.path",
        None,
    )
}
impl SnapshotTagFilesStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> SnapshotTagFilesQuery<'c, 'a, 's, C, SnapshotTagFiles, 0> {
        SnapshotTagFilesQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SnapshotTagFilesBorrowed, postgres::Error> {
                Ok(SnapshotTagFilesBorrowed { tag_id: row.try_get(0)?, folder: row.try_get(1)?, path: row.try_get(2)? })
            },
            mapper: |it| SnapshotTagFiles::from(it),
        }
    }
}
pub struct SnapshotStatesStmt(&'static str, Option<postgres::Statement>);
pub fn snapshot_states() -> SnapshotStatesStmt {
    SnapshotStatesStmt(
        "SELECT d.folder, f.path, s.favorite, s.archived, s.rating, s.description, s.taken_at, s.lat, s.lon, u.name AS owner FROM file_state s JOIN files f ON f.id = s.file_id JOIN devices d ON d.id = f.device_id JOIN users u ON u.id = s.user_id WHERE s.favorite OR s.archived OR s.rating IS NOT NULL OR s.description <> '' OR s.taken_at IS NOT NULL OR s.lat IS NOT NULL ORDER BY d.folder, f.path, u.name",
        None,
    )
}
impl SnapshotStatesStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> SnapshotStatesQuery<'c, 'a, 's, C, SnapshotStates, 0> {
        SnapshotStatesQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SnapshotStatesBorrowed, postgres::Error> {
                Ok(SnapshotStatesBorrowed {
                    folder: row.try_get(0)?,
                    path: row.try_get(1)?,
                    favorite: row.try_get(2)?,
                    archived: row.try_get(3)?,
                    rating: row.try_get(4)?,
                    description: row.try_get(5)?,
                    taken_at: row.try_get(6)?,
                    lat: row.try_get(7)?,
                    lon: row.try_get(8)?,
                    owner: row.try_get(9)?,
                })
            },
            mapper: |it| SnapshotStates::from(it),
        }
    }
}
