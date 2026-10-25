// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct InsertMirrorParams<
    T1: crate::StringSql,
    T2: crate::StringSql,
    T3: crate::StringSql,
    T4: crate::StringSql,
    T5: crate::StringSql,
> {
    pub name: T1,
    pub remote: T2,
    pub remote_type: T3,
    pub options: T4,
    pub remote_path: T5,
    pub interval_secs: i64,
    pub keep_runs: i64,
    pub now: i64,
}
#[derive(Debug)]
pub struct FinishMirrorParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub status: T1,
    pub message: T2,
    pub ok_at: Option<i64>,
    pub id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct SetMirrorTotalsParams {
    pub files: i64,
    pub bytes: i64,
    pub id: i64,
}
#[derive(Debug)]
pub struct UpsertMirrorItemParams<T1: crate::StringSql, T2: crate::BytesSql> {
    pub mirror_id: i64,
    pub device_id: i64,
    pub path: T1,
    pub root_hash: T2,
}
#[derive(Debug)]
pub struct DeleteMirrorItemParams<T1: crate::StringSql> {
    pub mirror_id: i64,
    pub device_id: i64,
    pub path: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListMirrors {
    pub name: String,
    pub remote: String,
    pub remote_type: String,
    pub options: String,
    pub remote_path: String,
    pub interval_secs: i64,
    pub keep_runs: i64,
    pub enabled: i64,
    pub last_run_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last_status: Option<String>,
    pub last_message: Option<String>,
    pub last_files: Option<i64>,
    pub last_bytes: Option<i64>,
}
pub struct ListMirrorsBorrowed<'a> {
    pub name: &'a str,
    pub remote: &'a str,
    pub remote_type: &'a str,
    pub options: &'a str,
    pub remote_path: &'a str,
    pub interval_secs: i64,
    pub keep_runs: i64,
    pub enabled: i64,
    pub last_run_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last_status: Option<&'a str>,
    pub last_message: Option<&'a str>,
    pub last_files: Option<i64>,
    pub last_bytes: Option<i64>,
}
impl<'a> From<ListMirrorsBorrowed<'a>> for ListMirrors {
    fn from(
        ListMirrorsBorrowed {
            name,
            remote,
            remote_type,
            options,
            remote_path,
            interval_secs,
            keep_runs,
            enabled,
            last_run_at,
            last_ok_at,
            last_status,
            last_message,
            last_files,
            last_bytes,
        }: ListMirrorsBorrowed<'a>,
    ) -> Self {
        Self {
            name: name.into(),
            remote: remote.into(),
            remote_type: remote_type.into(),
            options: options.into(),
            remote_path: remote_path.into(),
            interval_secs,
            keep_runs,
            enabled,
            last_run_at,
            last_ok_at,
            last_status: last_status.map(|v| v.into()),
            last_message: last_message.map(|v| v.into()),
            last_files,
            last_bytes,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimMirror {
    pub id: i64,
    pub name: String,
    pub remote_type: String,
    pub options: String,
    pub remote_path: String,
    pub keep_runs: i64,
    pub remote: String,
}
pub struct ClaimMirrorBorrowed<'a> {
    pub id: i64,
    pub name: &'a str,
    pub remote_type: &'a str,
    pub options: &'a str,
    pub remote_path: &'a str,
    pub keep_runs: i64,
    pub remote: &'a str,
}
impl<'a> From<ClaimMirrorBorrowed<'a>> for ClaimMirror {
    fn from(
        ClaimMirrorBorrowed { id, name, remote_type, options, remote_path, keep_runs, remote }: ClaimMirrorBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            remote_type: remote_type.into(),
            options: options.into(),
            remote_path: remote_path.into(),
            keep_runs,
            remote: remote.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct LiveFilesForMirror {
    pub device_id: i64,
    pub folder: String,
    pub path: String,
    pub root_hash: Vec<u8>,
    pub size: i64,
    pub mtime: i64,
}
pub struct LiveFilesForMirrorBorrowed<'a> {
    pub device_id: i64,
    pub folder: &'a str,
    pub path: &'a str,
    pub root_hash: &'a [u8],
    pub size: i64,
    pub mtime: i64,
}
impl<'a> From<LiveFilesForMirrorBorrowed<'a>> for LiveFilesForMirror {
    fn from(
        LiveFilesForMirrorBorrowed { device_id, folder, path, root_hash, size, mtime }: LiveFilesForMirrorBorrowed<'a>,
    ) -> Self {
        Self { device_id, folder: folder.into(), path: path.into(), root_hash: root_hash.into(), size, mtime }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct MirrorItems {
    pub device_id: i64,
    pub path: String,
    pub root_hash: Vec<u8>,
}
pub struct MirrorItemsBorrowed<'a> {
    pub device_id: i64,
    pub path: &'a str,
    pub root_hash: &'a [u8],
}
impl<'a> From<MirrorItemsBorrowed<'a>> for MirrorItems {
    fn from(MirrorItemsBorrowed { device_id, path, root_hash }: MirrorItemsBorrowed<'a>) -> Self {
        Self { device_id, path: path.into(), root_hash: root_hash.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceFolders {
    pub id: i64,
    pub folder: String,
}
pub struct DeviceFoldersBorrowed<'a> {
    pub id: i64,
    pub folder: &'a str,
}
impl<'a> From<DeviceFoldersBorrowed<'a>> for DeviceFolders {
    fn from(DeviceFoldersBorrowed { id, folder }: DeviceFoldersBorrowed<'a>) -> Self {
        Self { id, folder: folder.into() }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct ListMirrorsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ListMirrorsBorrowed, postgres::Error>,
    mapper: fn(ListMirrorsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListMirrorsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ListMirrorsBorrowed) -> R) -> ListMirrorsQuery<'c, 'a, 's, C, R, N> {
        ListMirrorsQuery {
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
pub struct ClaimMirrorQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ClaimMirrorBorrowed, postgres::Error>,
    mapper: fn(ClaimMirrorBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ClaimMirrorQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ClaimMirrorBorrowed) -> R) -> ClaimMirrorQuery<'c, 'a, 's, C, R, N> {
        ClaimMirrorQuery {
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
pub struct LiveFilesForMirrorQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<LiveFilesForMirrorBorrowed, postgres::Error>,
    mapper: fn(LiveFilesForMirrorBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> LiveFilesForMirrorQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(LiveFilesForMirrorBorrowed) -> R) -> LiveFilesForMirrorQuery<'c, 'a, 's, C, R, N> {
        LiveFilesForMirrorQuery {
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
pub struct MirrorItemsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<MirrorItemsBorrowed, postgres::Error>,
    mapper: fn(MirrorItemsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> MirrorItemsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(MirrorItemsBorrowed) -> R) -> MirrorItemsQuery<'c, 'a, 's, C, R, N> {
        MirrorItemsQuery {
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
pub struct DeviceFoldersQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<DeviceFoldersBorrowed, postgres::Error>,
    mapper: fn(DeviceFoldersBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> DeviceFoldersQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(DeviceFoldersBorrowed) -> R) -> DeviceFoldersQuery<'c, 'a, 's, C, R, N> {
        DeviceFoldersQuery {
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
pub struct ListMirrorsStmt(&'static str, Option<postgres::Statement>);
pub fn list_mirrors() -> ListMirrorsStmt {
    ListMirrorsStmt(
        "SELECT name, remote, remote_type, options, remote_path, interval_secs, keep_runs, enabled, last_run_at, last_ok_at, last_status, last_message, last_files, last_bytes FROM mirrors ORDER BY name",
        None,
    )
}
impl ListMirrorsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> ListMirrorsQuery<'c, 'a, 's, C, ListMirrors, 0> {
        ListMirrorsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ListMirrorsBorrowed, postgres::Error> {
                Ok(ListMirrorsBorrowed {
                    name: row.try_get(0)?,
                    remote: row.try_get(1)?,
                    remote_type: row.try_get(2)?,
                    options: row.try_get(3)?,
                    remote_path: row.try_get(4)?,
                    interval_secs: row.try_get(5)?,
                    keep_runs: row.try_get(6)?,
                    enabled: row.try_get(7)?,
                    last_run_at: row.try_get(8)?,
                    last_ok_at: row.try_get(9)?,
                    last_status: row.try_get(10)?,
                    last_message: row.try_get(11)?,
                    last_files: row.try_get(12)?,
                    last_bytes: row.try_get(13)?,
                })
            },
            mapper: |it| ListMirrors::from(it),
        }
    }
}
pub struct MirrorIdStmt(&'static str, Option<postgres::Statement>);
pub fn mirror_id() -> MirrorIdStmt {
    MirrorIdStmt("SELECT id FROM mirrors WHERE name = $1", None)
}
impl MirrorIdStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct InsertMirrorStmt(&'static str, Option<postgres::Statement>);
pub fn insert_mirror() -> InsertMirrorStmt {
    InsertMirrorStmt(
        "INSERT INTO mirrors (name, remote, remote_type, options, remote_path, interval_secs, keep_runs, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT (name) DO NOTHING",
        None,
    )
}
impl InsertMirrorStmt {
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
        T5: crate::StringSql,
    >(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
        remote: &'a T2,
        remote_type: &'a T3,
        options: &'a T4,
        remote_path: &'a T5,
        interval_secs: &'a i64,
        keep_runs: &'a i64,
        now: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[name, remote, remote_type, options, remote_path, interval_secs, keep_runs, now])
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
    T5: crate::StringSql,
> crate::client::sync::Params<'c, 'a, 's, InsertMirrorParams<T1, T2, T3, T4, T5>, Result<u64, postgres::Error>, C>
    for InsertMirrorStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a InsertMirrorParams<T1, T2, T3, T4, T5>,
    ) -> Result<u64, postgres::Error> {
        self.bind(
            client,
            &params.name,
            &params.remote,
            &params.remote_type,
            &params.options,
            &params.remote_path,
            &params.interval_secs,
            &params.keep_runs,
            &params.now,
        )
    }
}
pub struct DeleteMirrorItemsStmt(&'static str, Option<postgres::Statement>);
pub fn delete_mirror_items() -> DeleteMirrorItemsStmt {
    DeleteMirrorItemsStmt("DELETE FROM mirror_items WHERE mirror_id = $1", None)
}
impl DeleteMirrorItemsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        mirror_id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[mirror_id])
    }
}
pub struct DeleteMirrorStmt(&'static str, Option<postgres::Statement>);
pub fn delete_mirror() -> DeleteMirrorStmt {
    DeleteMirrorStmt("DELETE FROM mirrors WHERE id = $1", None)
}
impl DeleteMirrorStmt {
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
pub struct RequestMirrorRunStmt(&'static str, Option<postgres::Statement>);
pub fn request_mirror_run() -> RequestMirrorRunStmt {
    RequestMirrorRunStmt("UPDATE mirrors SET run_requested = 1 WHERE id = $1", None)
}
impl RequestMirrorRunStmt {
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
pub struct ToggleMirrorStmt(&'static str, Option<postgres::Statement>);
pub fn toggle_mirror() -> ToggleMirrorStmt {
    ToggleMirrorStmt("UPDATE mirrors SET enabled = 1 - enabled WHERE id = $1", None)
}
impl ToggleMirrorStmt {
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
pub struct ClaimMirrorStmt(&'static str, Option<postgres::Statement>);
pub fn claim_mirror() -> ClaimMirrorStmt {
    ClaimMirrorStmt(
        "UPDATE mirrors SET last_status = 'running', last_run_at = $1, run_requested = 0, last_message = '' WHERE id = (SELECT id FROM mirrors WHERE (run_requested = 1 OR (enabled = 1 AND (last_run_at IS NULL OR last_run_at + interval_secs <= $1))) AND last_status IS DISTINCT FROM 'running' ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id, name, remote_type, options, remote_path, keep_runs, remote",
        None,
    )
}
impl ClaimMirrorStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
    ) -> ClaimMirrorQuery<'c, 'a, 's, C, ClaimMirror, 1> {
        ClaimMirrorQuery {
            client,
            params: [now],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ClaimMirrorBorrowed, postgres::Error> {
                Ok(ClaimMirrorBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    remote_type: row.try_get(2)?,
                    options: row.try_get(3)?,
                    remote_path: row.try_get(4)?,
                    keep_runs: row.try_get(5)?,
                    remote: row.try_get(6)?,
                })
            },
            mapper: |it| ClaimMirror::from(it),
        }
    }
}
pub struct FinishMirrorStmt(&'static str, Option<postgres::Statement>);
pub fn finish_mirror() -> FinishMirrorStmt {
    FinishMirrorStmt(
        "UPDATE mirrors SET last_status = $1, last_message = $2, last_ok_at = COALESCE($3, last_ok_at) WHERE id = $4",
        None,
    )
}
impl FinishMirrorStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        status: &'a T1,
        message: &'a T2,
        ok_at: &'a Option<i64>,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[status, message, ok_at, id])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, FinishMirrorParams<T1, T2>, Result<u64, postgres::Error>, C>
    for FinishMirrorStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a FinishMirrorParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.status, &params.message, &params.ok_at, &params.id)
    }
}
pub struct SetMirrorTotalsStmt(&'static str, Option<postgres::Statement>);
pub fn set_mirror_totals() -> SetMirrorTotalsStmt {
    SetMirrorTotalsStmt("UPDATE mirrors SET last_files = $1, last_bytes = $2 WHERE id = $3", None)
}
impl SetMirrorTotalsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        files: &'a i64,
        bytes: &'a i64,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[files, bytes, id])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, SetMirrorTotalsParams, Result<u64, postgres::Error>, C>
    for SetMirrorTotalsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetMirrorTotalsParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.files, &params.bytes, &params.id)
    }
}
pub struct LiveFilesForMirrorStmt(&'static str, Option<postgres::Statement>);
pub fn live_files_for_mirror() -> LiveFilesForMirrorStmt {
    LiveFilesForMirrorStmt(
        "SELECT f.device_id, d.folder, f.path, v.root_hash, v.size, v.mtime FROM files f JOIN devices d ON d.id = f.device_id JOIN versions v ON v.id = f.current_version_id WHERE f.deleted_at IS NULL ORDER BY d.folder, f.path",
        None,
    )
}
impl LiveFilesForMirrorStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> LiveFilesForMirrorQuery<'c, 'a, 's, C, LiveFilesForMirror, 0> {
        LiveFilesForMirrorQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<LiveFilesForMirrorBorrowed, postgres::Error> {
                Ok(LiveFilesForMirrorBorrowed {
                    device_id: row.try_get(0)?,
                    folder: row.try_get(1)?,
                    path: row.try_get(2)?,
                    root_hash: row.try_get(3)?,
                    size: row.try_get(4)?,
                    mtime: row.try_get(5)?,
                })
            },
            mapper: |it| LiveFilesForMirror::from(it),
        }
    }
}
pub struct MirrorItemsStmt(&'static str, Option<postgres::Statement>);
pub fn mirror_items() -> MirrorItemsStmt {
    MirrorItemsStmt("SELECT device_id, path, root_hash FROM mirror_items WHERE mirror_id = $1", None)
}
impl MirrorItemsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        mirror_id: &'a i64,
    ) -> MirrorItemsQuery<'c, 'a, 's, C, MirrorItems, 1> {
        MirrorItemsQuery {
            client,
            params: [mirror_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<MirrorItemsBorrowed, postgres::Error> {
                Ok(MirrorItemsBorrowed {
                    device_id: row.try_get(0)?,
                    path: row.try_get(1)?,
                    root_hash: row.try_get(2)?,
                })
            },
            mapper: |it| MirrorItems::from(it),
        }
    }
}
pub struct UpsertMirrorItemStmt(&'static str, Option<postgres::Statement>);
pub fn upsert_mirror_item() -> UpsertMirrorItemStmt {
    UpsertMirrorItemStmt(
        "INSERT INTO mirror_items (mirror_id, device_id, path, root_hash) VALUES ($1, $2, $3, $4) ON CONFLICT (mirror_id, device_id, path) DO UPDATE SET root_hash = excluded.root_hash",
        None,
    )
}
impl UpsertMirrorItemStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::BytesSql>(
        &'s self,
        client: &'c mut C,
        mirror_id: &'a i64,
        device_id: &'a i64,
        path: &'a T1,
        root_hash: &'a T2,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[mirror_id, device_id, path, root_hash])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::BytesSql>
    crate::client::sync::Params<'c, 'a, 's, UpsertMirrorItemParams<T1, T2>, Result<u64, postgres::Error>, C>
    for UpsertMirrorItemStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a UpsertMirrorItemParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.mirror_id, &params.device_id, &params.path, &params.root_hash)
    }
}
pub struct DeleteMirrorItemStmt(&'static str, Option<postgres::Statement>);
pub fn delete_mirror_item() -> DeleteMirrorItemStmt {
    DeleteMirrorItemStmt("DELETE FROM mirror_items WHERE mirror_id = $1 AND device_id = $2 AND path = $3", None)
}
impl DeleteMirrorItemStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        mirror_id: &'a i64,
        device_id: &'a i64,
        path: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[mirror_id, device_id, path])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, DeleteMirrorItemParams<T1>, Result<u64, postgres::Error>, C>
    for DeleteMirrorItemStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a DeleteMirrorItemParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.mirror_id, &params.device_id, &params.path)
    }
}
pub struct DeviceFoldersStmt(&'static str, Option<postgres::Statement>);
pub fn device_folders() -> DeviceFoldersStmt {
    DeviceFoldersStmt("SELECT id, folder FROM devices", None)
}
impl DeviceFoldersStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> DeviceFoldersQuery<'c, 'a, 's, C, DeviceFolders, 0> {
        DeviceFoldersQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<DeviceFoldersBorrowed, postgres::Error> {
                Ok(DeviceFoldersBorrowed { id: row.try_get(0)?, folder: row.try_get(1)? })
            },
            mapper: |it| DeviceFolders::from(it),
        }
    }
}
