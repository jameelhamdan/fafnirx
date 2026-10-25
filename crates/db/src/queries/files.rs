// This file was generated with `cornucopia`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct PruneVersionsParams {
    pub now: i64,
    pub file_id: i64,
    pub keep: i64,
}
#[derive(Debug)]
pub struct LockFileRowParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
}
#[derive(Debug)]
pub struct VersionHasContentParams<T1: crate::BytesSql> {
    pub root: T1,
    pub version_id: i64,
}
#[derive(Debug)]
pub struct EnsureFileParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
}
#[derive(Debug)]
pub struct InsertVersionParams<T1: crate::BytesSql> {
    pub file_id: i64,
    pub root: T1,
    pub size: i64,
    pub mtime: i64,
    pub mode: i64,
    pub created_at: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct MakeCurrentParams {
    pub version_id: i64,
    pub file_id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct SetVersionMtimeParams {
    pub mtime: i64,
    pub version_id: i64,
}
#[derive(Debug)]
pub struct LivePathWithRootParams<T1: crate::BytesSql, T2: crate::StringSql> {
    pub device_id: i64,
    pub root: T1,
    pub prefer: T2,
}
#[derive(Debug)]
pub struct DeletedInWebAppParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
}
#[derive(Debug)]
pub struct LiveContentParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
}
#[derive(Debug)]
pub struct DeletePathParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub ts: i64,
    pub deleted_by: Option<T1>,
    pub device_id: i64,
    pub path: T2,
}
#[derive(Debug)]
pub struct KeepSourceItemDeletedParams<T1: crate::StringSql> {
    pub path: T1,
    pub device_id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct TouchDeviceParams {
    pub now: i64,
    pub id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct TouchCollectedParams {
    pub now: i64,
    pub id: i64,
}
#[derive(Debug)]
pub struct FileInfoParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
}
#[derive(Debug)]
pub struct ExpandPathsParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
    pub below: T2,
    pub include_deleted: bool,
}
#[derive(Debug)]
pub struct VersionToRestoreParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
    pub version_id: i64,
}
#[derive(Debug)]
pub struct FoldersBelowParams<T1: crate::StringSql> {
    pub from: i32,
    pub device_id: i64,
    pub below: T1,
    pub include_deleted: bool,
}
#[derive(Debug)]
pub struct FileRowParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
}
#[derive(Clone, Copy, Debug)]
pub struct VersionOfFileParams {
    pub version_id: i64,
    pub file_id: i64,
}
#[derive(Debug)]
pub struct CurrentMtimeParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
}
#[derive(Debug)]
pub struct LiveFilesBelowParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
    pub below: T2,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct LockFileRow {
    pub id: i64,
    pub current_version_id: Option<i64>,
    pub deleted_at: Option<i64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct LiveContent {
    pub root_hash: Vec<u8>,
    pub size: i64,
}
pub struct LiveContentBorrowed<'a> {
    pub root_hash: &'a [u8],
    pub size: i64,
}
impl<'a> From<LiveContentBorrowed<'a>> for LiveContent {
    fn from(LiveContentBorrowed { root_hash, size }: LiveContentBorrowed<'a>) -> Self {
        Self { root_hash: root_hash.into(), size }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    pub id: i64,
    pub current_version_id: Option<i64>,
    pub deleted_at: Option<i64>,
    pub size: i64,
    pub mtime: i64,
    pub created_at: i64,
    pub root_hash: Vec<u8>,
    pub mime: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub taken_at: Option<i64>,
    pub exif: Option<String>,
}
pub struct FileInfoBorrowed<'a> {
    pub id: i64,
    pub current_version_id: Option<i64>,
    pub deleted_at: Option<i64>,
    pub size: i64,
    pub mtime: i64,
    pub created_at: i64,
    pub root_hash: &'a [u8],
    pub mime: Option<&'a str>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub taken_at: Option<i64>,
    pub exif: Option<&'a str>,
}
impl<'a> From<FileInfoBorrowed<'a>> for FileInfo {
    fn from(
        FileInfoBorrowed {
            id,
            current_version_id,
            deleted_at,
            size,
            mtime,
            created_at,
            root_hash,
            mime,
            width,
            height,
            taken_at,
            exif,
        }: FileInfoBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            current_version_id,
            deleted_at,
            size,
            mtime,
            created_at,
            root_hash: root_hash.into(),
            mime: mime.map(|v| v.into()),
            width,
            height,
            taken_at,
            exif: exif.map(|v| v.into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct FileVersions {
    pub id: i64,
    pub created_at: i64,
    pub size: i64,
    pub mtime: i64,
    pub root_hash: Vec<u8>,
    pub pruned_at: Option<i64>,
}
pub struct FileVersionsBorrowed<'a> {
    pub id: i64,
    pub created_at: i64,
    pub size: i64,
    pub mtime: i64,
    pub root_hash: &'a [u8],
    pub pruned_at: Option<i64>,
}
impl<'a> From<FileVersionsBorrowed<'a>> for FileVersions {
    fn from(
        FileVersionsBorrowed { id, created_at, size, mtime, root_hash, pruned_at }: FileVersionsBorrowed<'a>,
    ) -> Self {
        Self { id, created_at, size, mtime, root_hash: root_hash.into(), pruned_at }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct VersionToRestore {
    pub current_version_id: Option<i64>,
    pub deleted_at: Option<i64>,
    pub root_hash: Vec<u8>,
    pub size: i64,
    pub mtime: i64,
    pub mode: i64,
}
pub struct VersionToRestoreBorrowed<'a> {
    pub current_version_id: Option<i64>,
    pub deleted_at: Option<i64>,
    pub root_hash: &'a [u8],
    pub size: i64,
    pub mtime: i64,
    pub mode: i64,
}
impl<'a> From<VersionToRestoreBorrowed<'a>> for VersionToRestore {
    fn from(
        VersionToRestoreBorrowed {
            current_version_id,
            deleted_at,
            root_hash,
            size,
            mtime,
            mode,
        }: VersionToRestoreBorrowed<'a>,
    ) -> Self {
        Self { current_version_id, deleted_at, root_hash: root_hash.into(), size, mtime, mode }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct FoldersBelow {
    pub name: String,
    pub files: i64,
    pub bytes: i64,
    pub changed: Option<i64>,
    pub deleted_at: Option<i64>,
}
pub struct FoldersBelowBorrowed<'a> {
    pub name: &'a str,
    pub files: i64,
    pub bytes: i64,
    pub changed: Option<i64>,
    pub deleted_at: Option<i64>,
}
impl<'a> From<FoldersBelowBorrowed<'a>> for FoldersBelow {
    fn from(FoldersBelowBorrowed { name, files, bytes, changed, deleted_at }: FoldersBelowBorrowed<'a>) -> Self {
        Self { name: name.into(), files, bytes, changed, deleted_at }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct FileRow {
    pub id: i64,
    pub current_version_id: Option<i64>,
    pub deleted_at: Option<i64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct VersionOfFile {
    pub root_hash: Vec<u8>,
    pub size: i64,
}
pub struct VersionOfFileBorrowed<'a> {
    pub root_hash: &'a [u8],
    pub size: i64,
}
impl<'a> From<VersionOfFileBorrowed<'a>> for VersionOfFile {
    fn from(VersionOfFileBorrowed { root_hash, size }: VersionOfFileBorrowed<'a>) -> Self {
        Self { root_hash: root_hash.into(), size }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct FileById {
    pub folder: String,
    pub path: String,
    pub current_version_id: i64,
    pub deleted_at: Option<i64>,
    pub root_hash: Vec<u8>,
    pub size: i64,
    pub codec: Option<String>,
}
pub struct FileByIdBorrowed<'a> {
    pub folder: &'a str,
    pub path: &'a str,
    pub current_version_id: i64,
    pub deleted_at: Option<i64>,
    pub root_hash: &'a [u8],
    pub size: i64,
    pub codec: Option<&'a str>,
}
impl<'a> From<FileByIdBorrowed<'a>> for FileById {
    fn from(
        FileByIdBorrowed { folder, path, current_version_id, deleted_at, root_hash, size, codec }: FileByIdBorrowed<'a>,
    ) -> Self {
        Self {
            folder: folder.into(),
            path: path.into(),
            current_version_id,
            deleted_at,
            root_hash: root_hash.into(),
            size,
            codec: codec.map(|v| v.into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct LiveFilesBelow {
    pub path: String,
    pub size: i64,
    pub mtime: i64,
}
pub struct LiveFilesBelowBorrowed<'a> {
    pub path: &'a str,
    pub size: i64,
    pub mtime: i64,
}
impl<'a> From<LiveFilesBelowBorrowed<'a>> for LiveFilesBelow {
    fn from(LiveFilesBelowBorrowed { path, size, mtime }: LiveFilesBelowBorrowed<'a>) -> Self {
        Self { path: path.into(), size, mtime }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct LockFileRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<LockFileRow, postgres::Error>,
    mapper: fn(LockFileRow) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> LockFileRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(LockFileRow) -> R) -> LockFileRowQuery<'c, 'a, 's, C, R, N> {
        LockFileRowQuery {
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
pub struct StringQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<&str, postgres::Error>,
    mapper: fn(&str) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> StringQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(&str) -> R) -> StringQuery<'c, 'a, 's, C, R, N> {
        StringQuery {
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
pub struct I32Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<i32, postgres::Error>,
    mapper: fn(i32) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> I32Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(i32) -> R) -> I32Query<'c, 'a, 's, C, R, N> {
        I32Query {
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
pub struct LiveContentQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<LiveContentBorrowed, postgres::Error>,
    mapper: fn(LiveContentBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> LiveContentQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(LiveContentBorrowed) -> R) -> LiveContentQuery<'c, 'a, 's, C, R, N> {
        LiveContentQuery {
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
pub struct Optioni64Query<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<Option<i64>, postgres::Error>,
    mapper: fn(Option<i64>) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> Optioni64Query<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(Option<i64>) -> R) -> Optioni64Query<'c, 'a, 's, C, R, N> {
        Optioni64Query {
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
pub struct FileInfoQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<FileInfoBorrowed, postgres::Error>,
    mapper: fn(FileInfoBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FileInfoQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(FileInfoBorrowed) -> R) -> FileInfoQuery<'c, 'a, 's, C, R, N> {
        FileInfoQuery {
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
pub struct FileVersionsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<FileVersionsBorrowed, postgres::Error>,
    mapper: fn(FileVersionsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FileVersionsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(FileVersionsBorrowed) -> R) -> FileVersionsQuery<'c, 'a, 's, C, R, N> {
        FileVersionsQuery {
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
pub struct VersionToRestoreQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<VersionToRestoreBorrowed, postgres::Error>,
    mapper: fn(VersionToRestoreBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> VersionToRestoreQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(VersionToRestoreBorrowed) -> R) -> VersionToRestoreQuery<'c, 'a, 's, C, R, N> {
        VersionToRestoreQuery {
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
pub struct FoldersBelowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<FoldersBelowBorrowed, postgres::Error>,
    mapper: fn(FoldersBelowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FoldersBelowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(FoldersBelowBorrowed) -> R) -> FoldersBelowQuery<'c, 'a, 's, C, R, N> {
        FoldersBelowQuery {
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
pub struct FileRowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<FileRow, postgres::Error>,
    mapper: fn(FileRow) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FileRowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(FileRow) -> R) -> FileRowQuery<'c, 'a, 's, C, R, N> {
        FileRowQuery {
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
pub struct VersionOfFileQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<VersionOfFileBorrowed, postgres::Error>,
    mapper: fn(VersionOfFileBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> VersionOfFileQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(VersionOfFileBorrowed) -> R) -> VersionOfFileQuery<'c, 'a, 's, C, R, N> {
        VersionOfFileQuery {
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
pub struct FileByIdQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<FileByIdBorrowed, postgres::Error>,
    mapper: fn(FileByIdBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FileByIdQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(FileByIdBorrowed) -> R) -> FileByIdQuery<'c, 'a, 's, C, R, N> {
        FileByIdQuery {
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
pub struct LiveFilesBelowQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<LiveFilesBelowBorrowed, postgres::Error>,
    mapper: fn(LiveFilesBelowBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> LiveFilesBelowQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(LiveFilesBelowBorrowed) -> R) -> LiveFilesBelowQuery<'c, 'a, 's, C, R, N> {
        LiveFilesBelowQuery {
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
pub struct PruneVersionsStmt(&'static str, Option<postgres::Statement>);
pub fn prune_versions() -> PruneVersionsStmt {
    PruneVersionsStmt(
        "UPDATE versions SET pruned_at = $1 WHERE file_id = $2 AND pruned_at IS NULL AND id NOT IN (SELECT id FROM versions WHERE file_id = $2 AND pruned_at IS NULL ORDER BY id DESC LIMIT $3)",
        None,
    )
}
impl PruneVersionsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        file_id: &'a i64,
        keep: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now, file_id, keep])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, PruneVersionsParams, Result<u64, postgres::Error>, C>
    for PruneVersionsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a PruneVersionsParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.file_id, &params.keep)
    }
}
pub struct LockFileRowStmt(&'static str, Option<postgres::Statement>);
pub fn lock_file_row() -> LockFileRowStmt {
    LockFileRowStmt(
        "SELECT id, current_version_id, deleted_at FROM files WHERE device_id = $1 AND path = $2 FOR UPDATE",
        None,
    )
}
impl LockFileRowStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
    ) -> LockFileRowQuery<'c, 'a, 's, C, LockFileRow, 2> {
        LockFileRowQuery {
            client,
            params: [device_id, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<LockFileRow, postgres::Error> {
                Ok(LockFileRow {
                    id: row.try_get(0)?,
                    current_version_id: row.try_get(1)?,
                    deleted_at: row.try_get(2)?,
                })
            },
            mapper: |it| LockFileRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, LockFileRowParams<T1>, LockFileRowQuery<'c, 'a, 's, C, LockFileRow, 2>, C>
    for LockFileRowStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a LockFileRowParams<T1>,
    ) -> LockFileRowQuery<'c, 'a, 's, C, LockFileRow, 2> {
        self.bind(client, &params.device_id, &params.path)
    }
}
pub struct VersionHasContentStmt(&'static str, Option<postgres::Statement>);
pub fn version_has_content() -> VersionHasContentStmt {
    VersionHasContentStmt("SELECT root_hash = $1 AS same FROM versions WHERE id = $2", None)
}
impl VersionHasContentStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s self,
        client: &'c mut C,
        root: &'a T1,
        version_id: &'a i64,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        BoolQuery {
            client,
            params: [root, version_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>
    crate::client::sync::Params<'c, 'a, 's, VersionHasContentParams<T1>, BoolQuery<'c, 'a, 's, C, bool, 2>, C>
    for VersionHasContentStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a VersionHasContentParams<T1>,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        self.bind(client, &params.root, &params.version_id)
    }
}
pub struct EnsureFileStmt(&'static str, Option<postgres::Statement>);
pub fn ensure_file() -> EnsureFileStmt {
    EnsureFileStmt(
        "INSERT INTO files (device_id, path) VALUES ($1, $2) ON CONFLICT (device_id, path) DO UPDATE SET path = excluded.path RETURNING id",
        None,
    )
}
impl EnsureFileStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
    ) -> I64Query<'c, 'a, 's, C, i64, 2> {
        I64Query {
            client,
            params: [device_id, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, EnsureFileParams<T1>, I64Query<'c, 'a, 's, C, i64, 2>, C>
    for EnsureFileStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a EnsureFileParams<T1>) -> I64Query<'c, 'a, 's, C, i64, 2> {
        self.bind(client, &params.device_id, &params.path)
    }
}
pub struct InsertVersionStmt(&'static str, Option<postgres::Statement>);
pub fn insert_version() -> InsertVersionStmt {
    InsertVersionStmt(
        "INSERT INTO versions (file_id, root_hash, size, mtime, mode, created_at) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        None,
    )
}
impl InsertVersionStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s self,
        client: &'c mut C,
        file_id: &'a i64,
        root: &'a T1,
        size: &'a i64,
        mtime: &'a i64,
        mode: &'a i64,
        created_at: &'a i64,
    ) -> I64Query<'c, 'a, 's, C, i64, 6> {
        I64Query {
            client,
            params: [file_id, root, size, mtime, mode, created_at],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>
    crate::client::sync::Params<'c, 'a, 's, InsertVersionParams<T1>, I64Query<'c, 'a, 's, C, i64, 6>, C>
    for InsertVersionStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a InsertVersionParams<T1>) -> I64Query<'c, 'a, 's, C, i64, 6> {
        self.bind(client, &params.file_id, &params.root, &params.size, &params.mtime, &params.mode, &params.created_at)
    }
}
pub struct MakeCurrentStmt(&'static str, Option<postgres::Statement>);
pub fn make_current() -> MakeCurrentStmt {
    MakeCurrentStmt("UPDATE files SET current_version_id = $1, deleted_at = NULL WHERE id = $2", None)
}
impl MakeCurrentStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        version_id: &'a i64,
        file_id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[version_id, file_id])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, MakeCurrentParams, Result<u64, postgres::Error>, C> for MakeCurrentStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a MakeCurrentParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.version_id, &params.file_id)
    }
}
pub struct SetVersionMtimeStmt(&'static str, Option<postgres::Statement>);
pub fn set_version_mtime() -> SetVersionMtimeStmt {
    SetVersionMtimeStmt("UPDATE versions SET mtime = $1 WHERE id = $2", None)
}
impl SetVersionMtimeStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        mtime: &'a i64,
        version_id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[mtime, version_id])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, SetVersionMtimeParams, Result<u64, postgres::Error>, C>
    for SetVersionMtimeStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetVersionMtimeParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.mtime, &params.version_id)
    }
}
pub struct ContentKnownStmt(&'static str, Option<postgres::Statement>);
pub fn content_known() -> ContentKnownStmt {
    ContentKnownStmt("SELECT EXISTS (SELECT 1 FROM file_meta WHERE root_hash = $1) AS known", None)
}
impl ContentKnownStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql>(
        &'s self,
        client: &'c mut C,
        root: &'a T1,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 1> {
        BoolQuery {
            client,
            params: [root],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct LivePathWithRootStmt(&'static str, Option<postgres::Statement>);
pub fn live_path_with_root() -> LivePathWithRootStmt {
    LivePathWithRootStmt(
        "SELECT f.path FROM files f JOIN versions v ON v.id = f.current_version_id WHERE f.device_id = $1 AND f.deleted_at IS NULL AND v.root_hash = $2 ORDER BY f.path = $3 DESC, f.id LIMIT 1",
        None,
    )
}
impl LivePathWithRootStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        root: &'a T1,
        prefer: &'a T2,
    ) -> StringQuery<'c, 'a, 's, C, String, 3> {
        StringQuery {
            client,
            params: [device_id, root, prefer],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::BytesSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, LivePathWithRootParams<T1, T2>, StringQuery<'c, 'a, 's, C, String, 3>, C>
    for LivePathWithRootStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a LivePathWithRootParams<T1, T2>,
    ) -> StringQuery<'c, 'a, 's, C, String, 3> {
        self.bind(client, &params.device_id, &params.root, &params.prefer)
    }
}
pub struct DeletedInWebAppStmt(&'static str, Option<postgres::Statement>);
pub fn deleted_in_web_app() -> DeletedInWebAppStmt {
    DeletedInWebAppStmt(
        "SELECT 1 FROM files WHERE device_id = $1 AND path = $2 AND deleted_at IS NOT NULL AND deleted_by = 'web'",
        None,
    )
}
impl DeletedInWebAppStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
    ) -> I32Query<'c, 'a, 's, C, i32, 2> {
        I32Query {
            client,
            params: [device_id, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, DeletedInWebAppParams<T1>, I32Query<'c, 'a, 's, C, i32, 2>, C>
    for DeletedInWebAppStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a DeletedInWebAppParams<T1>) -> I32Query<'c, 'a, 's, C, i32, 2> {
        self.bind(client, &params.device_id, &params.path)
    }
}
pub struct LiveContentStmt(&'static str, Option<postgres::Statement>);
pub fn live_content() -> LiveContentStmt {
    LiveContentStmt(
        "SELECT v.root_hash, v.size FROM files f JOIN versions v ON v.id = f.current_version_id WHERE f.device_id = $1 AND f.path = $2 AND f.deleted_at IS NULL",
        None,
    )
}
impl LiveContentStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
    ) -> LiveContentQuery<'c, 'a, 's, C, LiveContent, 2> {
        LiveContentQuery {
            client,
            params: [device_id, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<LiveContentBorrowed, postgres::Error> {
                Ok(LiveContentBorrowed { root_hash: row.try_get(0)?, size: row.try_get(1)? })
            },
            mapper: |it| LiveContent::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, LiveContentParams<T1>, LiveContentQuery<'c, 'a, 's, C, LiveContent, 2>, C>
    for LiveContentStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a LiveContentParams<T1>,
    ) -> LiveContentQuery<'c, 'a, 's, C, LiveContent, 2> {
        self.bind(client, &params.device_id, &params.path)
    }
}
pub struct DeletePathStmt(&'static str, Option<postgres::Statement>);
pub fn delete_path() -> DeletePathStmt {
    DeletePathStmt(
        "UPDATE files SET deleted_at = $1, deleted_by = $2 WHERE device_id = $3 AND path = $4 AND deleted_at IS NULL RETURNING current_version_id",
        None,
    )
}
impl DeletePathStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        ts: &'a i64,
        deleted_by: &'a Option<T1>,
        device_id: &'a i64,
        path: &'a T2,
    ) -> Optioni64Query<'c, 'a, 's, C, Option<i64>, 4> {
        Optioni64Query {
            client,
            params: [ts, deleted_by, device_id, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, DeletePathParams<T1, T2>, Optioni64Query<'c, 'a, 's, C, Option<i64>, 4>, C>
    for DeletePathStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a DeletePathParams<T1, T2>,
    ) -> Optioni64Query<'c, 'a, 's, C, Option<i64>, 4> {
        self.bind(client, &params.ts, &params.deleted_by, &params.device_id, &params.path)
    }
}
pub struct KeepSourceItemDeletedStmt(&'static str, Option<postgres::Statement>);
pub fn keep_source_item_deleted() -> KeepSourceItemDeletedStmt {
    KeepSourceItemDeletedStmt(
        "UPDATE source_items SET removed_in_web_app = TRUE WHERE path = $1 AND source_id IN (SELECT id FROM sources WHERE device_id = $2)",
        None,
    )
}
impl KeepSourceItemDeletedStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        path: &'a T1,
        device_id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[path, device_id])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, KeepSourceItemDeletedParams<T1>, Result<u64, postgres::Error>, C>
    for KeepSourceItemDeletedStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a KeepSourceItemDeletedParams<T1>,
    ) -> Result<u64, postgres::Error> {
        self.bind(client, &params.path, &params.device_id)
    }
}
pub struct TouchDeviceStmt(&'static str, Option<postgres::Statement>);
pub fn touch_device() -> TouchDeviceStmt {
    TouchDeviceStmt("UPDATE devices SET last_seen = $1, totals_stale = TRUE WHERE id = $2", None)
}
impl TouchDeviceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now, id])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, TouchDeviceParams, Result<u64, postgres::Error>, C> for TouchDeviceStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a TouchDeviceParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.id)
    }
}
pub struct TouchCollectedStmt(&'static str, Option<postgres::Statement>);
pub fn touch_collected() -> TouchCollectedStmt {
    TouchCollectedStmt(
        "UPDATE devices SET last_seen = $1, last_collected = $1, totals_stale = TRUE WHERE id = $2",
        None,
    )
}
impl TouchCollectedStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now, id])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, TouchCollectedParams, Result<u64, postgres::Error>, C>
    for TouchCollectedStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a TouchCollectedParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.id)
    }
}
pub struct MarkStaleStmt(&'static str, Option<postgres::Statement>);
pub fn mark_stale() -> MarkStaleStmt {
    MarkStaleStmt("UPDATE devices SET totals_stale = TRUE WHERE id = $1", None)
}
impl MarkStaleStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[id])
    }
}
pub struct FileInfoStmt(&'static str, Option<postgres::Statement>);
pub fn file_info() -> FileInfoStmt {
    FileInfoStmt(
        "SELECT f.id, f.current_version_id, f.deleted_at, v.size, v.mtime, v.created_at, v.root_hash, m.mime, m.width, m.height, m.taken_at, m.exif FROM files f JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash WHERE f.device_id = $1 AND f.path = $2",
        None,
    )
}
impl FileInfoStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
    ) -> FileInfoQuery<'c, 'a, 's, C, FileInfo, 2> {
        FileInfoQuery {
            client,
            params: [device_id, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<FileInfoBorrowed, postgres::Error> {
                Ok(FileInfoBorrowed {
                    id: row.try_get(0)?,
                    current_version_id: row.try_get(1)?,
                    deleted_at: row.try_get(2)?,
                    size: row.try_get(3)?,
                    mtime: row.try_get(4)?,
                    created_at: row.try_get(5)?,
                    root_hash: row.try_get(6)?,
                    mime: row.try_get(7)?,
                    width: row.try_get(8)?,
                    height: row.try_get(9)?,
                    taken_at: row.try_get(10)?,
                    exif: row.try_get(11)?,
                })
            },
            mapper: |it| FileInfo::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, FileInfoParams<T1>, FileInfoQuery<'c, 'a, 's, C, FileInfo, 2>, C>
    for FileInfoStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a FileInfoParams<T1>,
    ) -> FileInfoQuery<'c, 'a, 's, C, FileInfo, 2> {
        self.bind(client, &params.device_id, &params.path)
    }
}
pub struct FileVersionsStmt(&'static str, Option<postgres::Statement>);
pub fn file_versions() -> FileVersionsStmt {
    FileVersionsStmt(
        "SELECT id, created_at, size, mtime, root_hash, pruned_at FROM versions WHERE file_id = $1 ORDER BY id DESC",
        None,
    )
}
impl FileVersionsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        file_id: &'a i64,
    ) -> FileVersionsQuery<'c, 'a, 's, C, FileVersions, 1> {
        FileVersionsQuery {
            client,
            params: [file_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<FileVersionsBorrowed, postgres::Error> {
                Ok(FileVersionsBorrowed {
                    id: row.try_get(0)?,
                    created_at: row.try_get(1)?,
                    size: row.try_get(2)?,
                    mtime: row.try_get(3)?,
                    root_hash: row.try_get(4)?,
                    pruned_at: row.try_get(5)?,
                })
            },
            mapper: |it| FileVersions::from(it),
        }
    }
}
pub struct ExpandPathsStmt(&'static str, Option<postgres::Statement>);
pub fn expand_paths() -> ExpandPathsStmt {
    ExpandPathsStmt(
        "SELECT path FROM files WHERE device_id = $1 AND ($2 = '' OR path = $2 OR path LIKE $3) AND current_version_id IS NOT NULL AND ($4::boolean OR deleted_at IS NULL) ORDER BY path",
        None,
    )
}
impl ExpandPathsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
        below: &'a T2,
        include_deleted: &'a bool,
    ) -> StringQuery<'c, 'a, 's, C, String, 4> {
        StringQuery {
            client,
            params: [device_id, path, below, include_deleted],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, ExpandPathsParams<T1, T2>, StringQuery<'c, 'a, 's, C, String, 4>, C>
    for ExpandPathsStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a ExpandPathsParams<T1, T2>,
    ) -> StringQuery<'c, 'a, 's, C, String, 4> {
        self.bind(client, &params.device_id, &params.path, &params.below, &params.include_deleted)
    }
}
pub struct VersionToRestoreStmt(&'static str, Option<postgres::Statement>);
pub fn version_to_restore() -> VersionToRestoreStmt {
    VersionToRestoreStmt(
        "SELECT f.current_version_id, f.deleted_at, v.root_hash, v.size, v.mtime, v.mode FROM files f JOIN versions v ON v.file_id = f.id WHERE f.device_id = $1 AND f.path = $2 AND v.id = $3",
        None,
    )
}
impl VersionToRestoreStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
        version_id: &'a i64,
    ) -> VersionToRestoreQuery<'c, 'a, 's, C, VersionToRestore, 3> {
        VersionToRestoreQuery {
            client,
            params: [device_id, path, version_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<VersionToRestoreBorrowed, postgres::Error> {
                Ok(VersionToRestoreBorrowed {
                    current_version_id: row.try_get(0)?,
                    deleted_at: row.try_get(1)?,
                    root_hash: row.try_get(2)?,
                    size: row.try_get(3)?,
                    mtime: row.try_get(4)?,
                    mode: row.try_get(5)?,
                })
            },
            mapper: |it| VersionToRestore::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        VersionToRestoreParams<T1>,
        VersionToRestoreQuery<'c, 'a, 's, C, VersionToRestore, 3>,
        C,
    > for VersionToRestoreStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a VersionToRestoreParams<T1>,
    ) -> VersionToRestoreQuery<'c, 'a, 's, C, VersionToRestore, 3> {
        self.bind(client, &params.device_id, &params.path, &params.version_id)
    }
}
pub struct FoldersBelowStmt(&'static str, Option<postgres::Statement>);
pub fn folders_below() -> FoldersBelowStmt {
    FoldersBelowStmt(
        "SELECT split_part(substr(f.path, $1), '/', 1) AS name, COUNT(f.id) AS files, COALESCE(SUM(v.size), 0)::BIGINT AS bytes, MAX(v.created_at) AS changed, CASE WHEN bool_and(f.deleted_at IS NOT NULL) THEN MAX(f.deleted_at) END AS deleted_at FROM files f LEFT JOIN versions v ON v.id = f.current_version_id WHERE f.device_id = $2 AND f.path LIKE $3 AND ($4::boolean OR f.deleted_at IS NULL) AND strpos(substr(f.path, $1), '/') > 0 GROUP BY 1 ORDER BY lower(split_part(substr(f.path, $1), '/', 1))",
        None,
    )
}
impl FoldersBelowStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        from: &'a i32,
        device_id: &'a i64,
        below: &'a T1,
        include_deleted: &'a bool,
    ) -> FoldersBelowQuery<'c, 'a, 's, C, FoldersBelow, 4> {
        FoldersBelowQuery {
            client,
            params: [from, device_id, below, include_deleted],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<FoldersBelowBorrowed, postgres::Error> {
                Ok(FoldersBelowBorrowed {
                    name: row.try_get(0)?,
                    files: row.try_get(1)?,
                    bytes: row.try_get(2)?,
                    changed: row.try_get(3)?,
                    deleted_at: row.try_get(4)?,
                })
            },
            mapper: |it| FoldersBelow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        FoldersBelowParams<T1>,
        FoldersBelowQuery<'c, 'a, 's, C, FoldersBelow, 4>,
        C,
    > for FoldersBelowStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a FoldersBelowParams<T1>,
    ) -> FoldersBelowQuery<'c, 'a, 's, C, FoldersBelow, 4> {
        self.bind(client, &params.from, &params.device_id, &params.below, &params.include_deleted)
    }
}
pub struct FileRowStmt(&'static str, Option<postgres::Statement>);
pub fn file_row() -> FileRowStmt {
    FileRowStmt("SELECT id, current_version_id, deleted_at FROM files WHERE device_id = $1 AND path = $2", None)
}
impl FileRowStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
    ) -> FileRowQuery<'c, 'a, 's, C, FileRow, 2> {
        FileRowQuery {
            client,
            params: [device_id, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<FileRow, postgres::Error> {
                Ok(FileRow { id: row.try_get(0)?, current_version_id: row.try_get(1)?, deleted_at: row.try_get(2)? })
            },
            mapper: |it| FileRow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, FileRowParams<T1>, FileRowQuery<'c, 'a, 's, C, FileRow, 2>, C>
    for FileRowStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a FileRowParams<T1>) -> FileRowQuery<'c, 'a, 's, C, FileRow, 2> {
        self.bind(client, &params.device_id, &params.path)
    }
}
pub struct VersionOfFileStmt(&'static str, Option<postgres::Statement>);
pub fn version_of_file() -> VersionOfFileStmt {
    VersionOfFileStmt("SELECT root_hash, size FROM versions WHERE id = $1 AND file_id = $2", None)
}
impl VersionOfFileStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        version_id: &'a i64,
        file_id: &'a i64,
    ) -> VersionOfFileQuery<'c, 'a, 's, C, VersionOfFile, 2> {
        VersionOfFileQuery {
            client,
            params: [version_id, file_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<VersionOfFileBorrowed, postgres::Error> {
                Ok(VersionOfFileBorrowed { root_hash: row.try_get(0)?, size: row.try_get(1)? })
            },
            mapper: |it| VersionOfFile::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, VersionOfFileParams, VersionOfFileQuery<'c, 'a, 's, C, VersionOfFile, 2>, C>
    for VersionOfFileStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a VersionOfFileParams,
    ) -> VersionOfFileQuery<'c, 'a, 's, C, VersionOfFile, 2> {
        self.bind(client, &params.version_id, &params.file_id)
    }
}
pub struct FileByIdStmt(&'static str, Option<postgres::Statement>);
pub fn file_by_id() -> FileByIdStmt {
    FileByIdStmt(
        "SELECT d.folder, f.path, f.current_version_id, f.deleted_at, v.root_hash, v.size, m.codec FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash WHERE f.id = $1",
        None,
    )
}
impl FileByIdStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        id: &'a i64,
    ) -> FileByIdQuery<'c, 'a, 's, C, FileById, 1> {
        FileByIdQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<FileByIdBorrowed, postgres::Error> {
                Ok(FileByIdBorrowed {
                    folder: row.try_get(0)?,
                    path: row.try_get(1)?,
                    current_version_id: row.try_get(2)?,
                    deleted_at: row.try_get(3)?,
                    root_hash: row.try_get(4)?,
                    size: row.try_get(5)?,
                    codec: row.try_get(6)?,
                })
            },
            mapper: |it| FileById::from(it),
        }
    }
}
pub struct CurrentMtimeStmt(&'static str, Option<postgres::Statement>);
pub fn current_mtime() -> CurrentMtimeStmt {
    CurrentMtimeStmt(
        "SELECT v.mtime FROM files f JOIN versions v ON v.id = f.current_version_id WHERE f.device_id = $1 AND f.path = $2",
        None,
    )
}
impl CurrentMtimeStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
    ) -> I64Query<'c, 'a, 's, C, i64, 2> {
        I64Query {
            client,
            params: [device_id, path],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, CurrentMtimeParams<T1>, I64Query<'c, 'a, 's, C, i64, 2>, C>
    for CurrentMtimeStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a CurrentMtimeParams<T1>) -> I64Query<'c, 'a, 's, C, i64, 2> {
        self.bind(client, &params.device_id, &params.path)
    }
}
pub struct LiveFilesBelowStmt(&'static str, Option<postgres::Statement>);
pub fn live_files_below() -> LiveFilesBelowStmt {
    LiveFilesBelowStmt(
        "SELECT f.path, v.size, v.mtime FROM files f JOIN versions v ON v.id = f.current_version_id WHERE f.device_id = $1 AND f.deleted_at IS NULL AND ($2 = '' OR f.path = $2 OR f.path LIKE $3)",
        None,
    )
}
impl LiveFilesBelowStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
        below: &'a T2,
    ) -> LiveFilesBelowQuery<'c, 'a, 's, C, LiveFilesBelow, 3> {
        LiveFilesBelowQuery {
            client,
            params: [device_id, path, below],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<LiveFilesBelowBorrowed, postgres::Error> {
                Ok(LiveFilesBelowBorrowed { path: row.try_get(0)?, size: row.try_get(1)?, mtime: row.try_get(2)? })
            },
            mapper: |it| LiveFilesBelow::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        LiveFilesBelowParams<T1, T2>,
        LiveFilesBelowQuery<'c, 'a, 's, C, LiveFilesBelow, 3>,
        C,
    > for LiveFilesBelowStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a LiveFilesBelowParams<T1, T2>,
    ) -> LiveFilesBelowQuery<'c, 'a, 's, C, LiveFilesBelow, 3> {
        self.bind(client, &params.device_id, &params.path, &params.below)
    }
}
