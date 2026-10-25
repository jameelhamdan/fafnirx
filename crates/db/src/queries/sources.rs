// This file was generated with `cornucopia`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct ListSourcesParams {
    pub all: bool,
    pub user: i64,
}
#[derive(Debug)]
pub struct SourceByOwnerParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub owner: T1,
    pub name: T2,
}
#[derive(Debug)]
pub struct InsertSourceDeviceParams<T1: crate::StringSql> {
    pub name: T1,
    pub now: i64,
    pub user_id: i64,
}
#[derive(Debug)]
pub struct InsertSourceParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub name: T1,
    pub config: T2,
    pub interval_secs: i64,
    pub device_id: i64,
    pub now: i64,
}
#[derive(Debug)]
pub struct FinishSourceParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub status: T1,
    pub message: T2,
    pub ok_at: Option<i64>,
    pub id: i64,
}
#[derive(Debug)]
pub struct UpsertSourceItemParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub source_id: i64,
    pub remote_id: T1,
    pub path: T2,
    pub remote_version: T3,
}
#[derive(Clone, Copy, Debug)]
pub struct RemovedInWebAppParams {
    pub device_id: i64,
    pub source_id: i64,
}
#[derive(Debug)]
pub struct DeleteSourceItemParams<T1: crate::StringSql> {
    pub source_id: i64,
    pub remote_id: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListSources {
    pub name: String,
    pub owner: String,
    pub config: String,
    pub interval_secs: i64,
    pub enabled: i64,
    pub last_run_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last_status: Option<String>,
    pub last_message: Option<String>,
    pub bytes: i64,
    pub files: i64,
    pub device_id: i64,
    pub folder: String,
}
pub struct ListSourcesBorrowed<'a> {
    pub name: &'a str,
    pub owner: &'a str,
    pub config: &'a str,
    pub interval_secs: i64,
    pub enabled: i64,
    pub last_run_at: Option<i64>,
    pub last_ok_at: Option<i64>,
    pub last_status: Option<&'a str>,
    pub last_message: Option<&'a str>,
    pub bytes: i64,
    pub files: i64,
    pub device_id: i64,
    pub folder: &'a str,
}
impl<'a> From<ListSourcesBorrowed<'a>> for ListSources {
    fn from(
        ListSourcesBorrowed {
            name,
            owner,
            config,
            interval_secs,
            enabled,
            last_run_at,
            last_ok_at,
            last_status,
            last_message,
            bytes,
            files,
            device_id,
            folder,
        }: ListSourcesBorrowed<'a>,
    ) -> Self {
        Self {
            name: name.into(),
            owner: owner.into(),
            config: config.into(),
            interval_secs,
            enabled,
            last_run_at,
            last_ok_at,
            last_status: last_status.map(|v| v.into()),
            last_message: last_message.map(|v| v.into()),
            bytes,
            files,
            device_id,
            folder: folder.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceByOwner {
    pub id: i64,
    pub folder: String,
}
pub struct SourceByOwnerBorrowed<'a> {
    pub id: i64,
    pub folder: &'a str,
}
impl<'a> From<SourceByOwnerBorrowed<'a>> for SourceByOwner {
    fn from(SourceByOwnerBorrowed { id, folder }: SourceByOwnerBorrowed<'a>) -> Self {
        Self { id, folder: folder.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct InsertSourceDevice {
    pub id: i64,
    pub folder: String,
}
pub struct InsertSourceDeviceBorrowed<'a> {
    pub id: i64,
    pub folder: &'a str,
}
impl<'a> From<InsertSourceDeviceBorrowed<'a>> for InsertSourceDevice {
    fn from(InsertSourceDeviceBorrowed { id, folder }: InsertSourceDeviceBorrowed<'a>) -> Self {
        Self { id, folder: folder.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceDevice {
    pub device_id: i64,
    pub folder: String,
}
pub struct SourceDeviceBorrowed<'a> {
    pub device_id: i64,
    pub folder: &'a str,
}
impl<'a> From<SourceDeviceBorrowed<'a>> for SourceDevice {
    fn from(SourceDeviceBorrowed { device_id, folder }: SourceDeviceBorrowed<'a>) -> Self {
        Self { device_id, folder: folder.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimSource {
    pub id: i64,
    pub kind: String,
    pub config: String,
    pub device_id: i64,
    pub folder: String,
}
pub struct ClaimSourceBorrowed<'a> {
    pub id: i64,
    pub kind: &'a str,
    pub config: &'a str,
    pub device_id: i64,
    pub folder: &'a str,
}
impl<'a> From<ClaimSourceBorrowed<'a>> for ClaimSource {
    fn from(ClaimSourceBorrowed { id, kind, config, device_id, folder }: ClaimSourceBorrowed<'a>) -> Self {
        Self { id, kind: kind.into(), config: config.into(), device_id, folder: folder.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct SourceItems {
    pub remote_id: String,
    pub path: String,
    pub remote_version: String,
}
pub struct SourceItemsBorrowed<'a> {
    pub remote_id: &'a str,
    pub path: &'a str,
    pub remote_version: &'a str,
}
impl<'a> From<SourceItemsBorrowed<'a>> for SourceItems {
    fn from(SourceItemsBorrowed { remote_id, path, remote_version }: SourceItemsBorrowed<'a>) -> Self {
        Self { remote_id: remote_id.into(), path: path.into(), remote_version: remote_version.into() }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct ListSourcesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ListSourcesBorrowed, postgres::Error>,
    mapper: fn(ListSourcesBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListSourcesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ListSourcesBorrowed) -> R) -> ListSourcesQuery<'c, 'a, 's, C, R, N> {
        ListSourcesQuery {
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
pub struct SourceByOwnerQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SourceByOwnerBorrowed, postgres::Error>,
    mapper: fn(SourceByOwnerBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SourceByOwnerQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SourceByOwnerBorrowed) -> R) -> SourceByOwnerQuery<'c, 'a, 's, C, R, N> {
        SourceByOwnerQuery {
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
pub struct InsertSourceDeviceQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<InsertSourceDeviceBorrowed, postgres::Error>,
    mapper: fn(InsertSourceDeviceBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> InsertSourceDeviceQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(InsertSourceDeviceBorrowed) -> R) -> InsertSourceDeviceQuery<'c, 'a, 's, C, R, N> {
        InsertSourceDeviceQuery {
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
pub struct SourceDeviceQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SourceDeviceBorrowed, postgres::Error>,
    mapper: fn(SourceDeviceBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SourceDeviceQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SourceDeviceBorrowed) -> R) -> SourceDeviceQuery<'c, 'a, 's, C, R, N> {
        SourceDeviceQuery {
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
pub struct ClaimSourceQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ClaimSourceBorrowed, postgres::Error>,
    mapper: fn(ClaimSourceBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ClaimSourceQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ClaimSourceBorrowed) -> R) -> ClaimSourceQuery<'c, 'a, 's, C, R, N> {
        ClaimSourceQuery {
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
pub struct SourceItemsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SourceItemsBorrowed, postgres::Error>,
    mapper: fn(SourceItemsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SourceItemsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SourceItemsBorrowed) -> R) -> SourceItemsQuery<'c, 'a, 's, C, R, N> {
        SourceItemsQuery {
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
pub struct ListSourcesStmt(&'static str, Option<postgres::Statement>);
pub fn list_sources() -> ListSourcesStmt {
    ListSourcesStmt(
        "SELECT s.name, u.name AS owner, s.config, s.interval_secs, s.enabled, s.last_run_at, s.last_ok_at, s.last_status, s.last_message, d.total_bytes AS bytes, d.file_count AS files, s.device_id, d.folder FROM sources s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = d.user_id WHERE $1::boolean OR d.user_id = $2::bigint ORDER BY u.name, s.name",
        None,
    )
}
impl ListSourcesStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        all: &'a bool,
        user: &'a i64,
    ) -> ListSourcesQuery<'c, 'a, 's, C, ListSources, 2> {
        ListSourcesQuery {
            client,
            params: [all, user],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ListSourcesBorrowed, postgres::Error> {
                Ok(ListSourcesBorrowed {
                    name: row.try_get(0)?,
                    owner: row.try_get(1)?,
                    config: row.try_get(2)?,
                    interval_secs: row.try_get(3)?,
                    enabled: row.try_get(4)?,
                    last_run_at: row.try_get(5)?,
                    last_ok_at: row.try_get(6)?,
                    last_status: row.try_get(7)?,
                    last_message: row.try_get(8)?,
                    bytes: row.try_get(9)?,
                    files: row.try_get(10)?,
                    device_id: row.try_get(11)?,
                    folder: row.try_get(12)?,
                })
            },
            mapper: |it| ListSources::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, ListSourcesParams, ListSourcesQuery<'c, 'a, 's, C, ListSources, 2>, C>
    for ListSourcesStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a ListSourcesParams,
    ) -> ListSourcesQuery<'c, 'a, 's, C, ListSources, 2> {
        self.bind(client, &params.all, &params.user)
    }
}
pub struct SourceByOwnerStmt(&'static str, Option<postgres::Statement>);
pub fn source_by_owner() -> SourceByOwnerStmt {
    SourceByOwnerStmt(
        "SELECT s.id, d.folder FROM sources s JOIN devices d ON d.id = s.device_id JOIN users u ON u.id = d.user_id WHERE u.name = $1 AND s.name = $2",
        None,
    )
}
impl SourceByOwnerStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        owner: &'a T1,
        name: &'a T2,
    ) -> SourceByOwnerQuery<'c, 'a, 's, C, SourceByOwner, 2> {
        SourceByOwnerQuery {
            client,
            params: [owner, name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SourceByOwnerBorrowed, postgres::Error> {
                Ok(SourceByOwnerBorrowed { id: row.try_get(0)?, folder: row.try_get(1)? })
            },
            mapper: |it| SourceByOwner::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        SourceByOwnerParams<T1, T2>,
        SourceByOwnerQuery<'c, 'a, 's, C, SourceByOwner, 2>,
        C,
    > for SourceByOwnerStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a SourceByOwnerParams<T1, T2>,
    ) -> SourceByOwnerQuery<'c, 'a, 's, C, SourceByOwner, 2> {
        self.bind(client, &params.owner, &params.name)
    }
}
pub struct InsertSourceDeviceStmt(&'static str, Option<postgres::Statement>);
pub fn insert_source_device() -> InsertSourceDeviceStmt {
    InsertSourceDeviceStmt(
        "INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES ($1, 'source', '!', $2, $3) ON CONFLICT (user_id, name) DO NOTHING RETURNING id, folder",
        None,
    )
}
impl InsertSourceDeviceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
        now: &'a i64,
        user_id: &'a i64,
    ) -> InsertSourceDeviceQuery<'c, 'a, 's, C, InsertSourceDevice, 3> {
        InsertSourceDeviceQuery {
            client,
            params: [name, now, user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<InsertSourceDeviceBorrowed, postgres::Error> {
                Ok(InsertSourceDeviceBorrowed { id: row.try_get(0)?, folder: row.try_get(1)? })
            },
            mapper: |it| InsertSourceDevice::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        InsertSourceDeviceParams<T1>,
        InsertSourceDeviceQuery<'c, 'a, 's, C, InsertSourceDevice, 3>,
        C,
    > for InsertSourceDeviceStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a InsertSourceDeviceParams<T1>,
    ) -> InsertSourceDeviceQuery<'c, 'a, 's, C, InsertSourceDevice, 3> {
        self.bind(client, &params.name, &params.now, &params.user_id)
    }
}
pub struct InsertSourceStmt(&'static str, Option<postgres::Statement>);
pub fn insert_source() -> InsertSourceStmt {
    InsertSourceStmt(
        "INSERT INTO sources (name, kind, config, interval_secs, device_id, created_at) VALUES ($1, 'gdrive', $2, $3, $4, $5)",
        None,
    )
}
impl InsertSourceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
        config: &'a T2,
        interval_secs: &'a i64,
        device_id: &'a i64,
        now: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[name, config, interval_secs, device_id, now])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, InsertSourceParams<T1, T2>, Result<u64, postgres::Error>, C>
    for InsertSourceStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a InsertSourceParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.name, &params.config, &params.interval_secs, &params.device_id, &params.now)
    }
}
pub struct SourceDeviceStmt(&'static str, Option<postgres::Statement>);
pub fn source_device() -> SourceDeviceStmt {
    SourceDeviceStmt(
        "SELECT s.device_id, d.folder FROM sources s JOIN devices d ON d.id = s.device_id WHERE s.id = $1",
        None,
    )
}
impl SourceDeviceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        id: &'a i64,
    ) -> SourceDeviceQuery<'c, 'a, 's, C, SourceDevice, 1> {
        SourceDeviceQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SourceDeviceBorrowed, postgres::Error> {
                Ok(SourceDeviceBorrowed { device_id: row.try_get(0)?, folder: row.try_get(1)? })
            },
            mapper: |it| SourceDevice::from(it),
        }
    }
}
pub struct DeleteSourceItemsStmt(&'static str, Option<postgres::Statement>);
pub fn delete_source_items() -> DeleteSourceItemsStmt {
    DeleteSourceItemsStmt("DELETE FROM source_items WHERE source_id = $1", None)
}
impl DeleteSourceItemsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        source_id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[source_id])
    }
}
pub struct DeleteSourceStmt(&'static str, Option<postgres::Statement>);
pub fn delete_source() -> DeleteSourceStmt {
    DeleteSourceStmt("DELETE FROM sources WHERE id = $1", None)
}
impl DeleteSourceStmt {
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
pub struct RequestSourceRunStmt(&'static str, Option<postgres::Statement>);
pub fn request_source_run() -> RequestSourceRunStmt {
    RequestSourceRunStmt("UPDATE sources SET run_requested = 1 WHERE id = $1", None)
}
impl RequestSourceRunStmt {
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
pub struct ToggleSourceStmt(&'static str, Option<postgres::Statement>);
pub fn toggle_source() -> ToggleSourceStmt {
    ToggleSourceStmt("UPDATE sources SET enabled = 1 - enabled WHERE id = $1", None)
}
impl ToggleSourceStmt {
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
pub struct ClaimSourceStmt(&'static str, Option<postgres::Statement>);
pub fn claim_source() -> ClaimSourceStmt {
    ClaimSourceStmt(
        "UPDATE sources SET last_status = 'running', last_run_at = $1, run_requested = 0, last_message = '' WHERE id = (SELECT id FROM sources WHERE (run_requested = 1 OR (enabled = 1 AND (last_run_at IS NULL OR last_run_at + interval_secs <= $1))) AND last_status IS DISTINCT FROM 'running' ORDER BY id LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id, kind, config, device_id, (SELECT d.folder FROM devices d WHERE d.id = device_id) AS folder",
        None,
    )
}
impl ClaimSourceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
    ) -> ClaimSourceQuery<'c, 'a, 's, C, ClaimSource, 1> {
        ClaimSourceQuery {
            client,
            params: [now],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ClaimSourceBorrowed, postgres::Error> {
                Ok(ClaimSourceBorrowed {
                    id: row.try_get(0)?,
                    kind: row.try_get(1)?,
                    config: row.try_get(2)?,
                    device_id: row.try_get(3)?,
                    folder: row.try_get(4)?,
                })
            },
            mapper: |it| ClaimSource::from(it),
        }
    }
}
pub struct FinishSourceStmt(&'static str, Option<postgres::Statement>);
pub fn finish_source() -> FinishSourceStmt {
    FinishSourceStmt(
        "UPDATE sources SET last_status = $1, last_message = $2, last_ok_at = COALESCE($3, last_ok_at) WHERE id = $4",
        None,
    )
}
impl FinishSourceStmt {
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
    crate::client::sync::Params<'c, 'a, 's, FinishSourceParams<T1, T2>, Result<u64, postgres::Error>, C>
    for FinishSourceStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a FinishSourceParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.status, &params.message, &params.ok_at, &params.id)
    }
}
pub struct UpsertSourceItemStmt(&'static str, Option<postgres::Statement>);
pub fn upsert_source_item() -> UpsertSourceItemStmt {
    UpsertSourceItemStmt(
        "INSERT INTO source_items (source_id, remote_id, path, remote_version) VALUES ($1, $2, $3, $4) ON CONFLICT (source_id, remote_id) DO UPDATE SET path = excluded.path, remote_version = excluded.remote_version, removed_in_web_app = FALSE",
        None,
    )
}
impl UpsertSourceItemStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        source_id: &'a i64,
        remote_id: &'a T1,
        path: &'a T2,
        remote_version: &'a T3,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[source_id, remote_id, path, remote_version])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, UpsertSourceItemParams<T1, T2, T3>, Result<u64, postgres::Error>, C>
    for UpsertSourceItemStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a UpsertSourceItemParams<T1, T2, T3>,
    ) -> Result<u64, postgres::Error> {
        self.bind(client, &params.source_id, &params.remote_id, &params.path, &params.remote_version)
    }
}
pub struct SourceItemsStmt(&'static str, Option<postgres::Statement>);
pub fn source_items() -> SourceItemsStmt {
    SourceItemsStmt("SELECT remote_id, path, remote_version FROM source_items WHERE source_id = $1", None)
}
impl SourceItemsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        source_id: &'a i64,
    ) -> SourceItemsQuery<'c, 'a, 's, C, SourceItems, 1> {
        SourceItemsQuery {
            client,
            params: [source_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SourceItemsBorrowed, postgres::Error> {
                Ok(SourceItemsBorrowed {
                    remote_id: row.try_get(0)?,
                    path: row.try_get(1)?,
                    remote_version: row.try_get(2)?,
                })
            },
            mapper: |it| SourceItems::from(it),
        }
    }
}
pub struct LivePathsStmt(&'static str, Option<postgres::Statement>);
pub fn live_paths() -> LivePathsStmt {
    LivePathsStmt("SELECT path FROM files WHERE device_id = $1 AND deleted_at IS NULL", None)
}
impl LivePathsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
    ) -> StringQuery<'c, 'a, 's, C, String, 1> {
        StringQuery {
            client,
            params: [device_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct RemovedInWebAppStmt(&'static str, Option<postgres::Statement>);
pub fn removed_in_web_app() -> RemovedInWebAppStmt {
    RemovedInWebAppStmt(
        "SELECT path FROM files WHERE device_id = $1 AND deleted_at IS NOT NULL AND deleted_by = 'web' UNION SELECT path FROM source_items WHERE source_id = $2 AND removed_in_web_app",
        None,
    )
}
impl RemovedInWebAppStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        source_id: &'a i64,
    ) -> StringQuery<'c, 'a, 's, C, String, 2> {
        StringQuery {
            client,
            params: [device_id, source_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, RemovedInWebAppParams, StringQuery<'c, 'a, 's, C, String, 2>, C>
    for RemovedInWebAppStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a RemovedInWebAppParams) -> StringQuery<'c, 'a, 's, C, String, 2> {
        self.bind(client, &params.device_id, &params.source_id)
    }
}
pub struct DeleteSourceItemStmt(&'static str, Option<postgres::Statement>);
pub fn delete_source_item() -> DeleteSourceItemStmt {
    DeleteSourceItemStmt("DELETE FROM source_items WHERE source_id = $1 AND remote_id = $2", None)
}
impl DeleteSourceItemStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        source_id: &'a i64,
        remote_id: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[source_id, remote_id])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, DeleteSourceItemParams<T1>, Result<u64, postgres::Error>, C>
    for DeleteSourceItemStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a DeleteSourceItemParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.source_id, &params.remote_id)
    }
}
