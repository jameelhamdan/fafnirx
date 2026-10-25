// This file was generated with `cornucopia`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct ActivityByDayParams {
    pub since: i64,
    pub all: bool,
    pub user: i64,
}
#[derive(Debug)]
pub struct RecentEventsParams<T1: crate::StringSql> {
    pub folder: Option<T1>,
    pub all: bool,
    pub user: i64,
    pub limit: i64,
}
#[derive(Debug)]
pub struct InsertEventParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql, T4: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
    pub action: T2,
    pub ts: i64,
    pub hash: Option<T3>,
    pub detail: Option<T4>,
}
#[derive(Debug)]
pub struct FileEventsParams<T1: crate::StringSql> {
    pub device_id: i64,
    pub path: T1,
    pub limit: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityByDay {
    pub day: i64,
    pub action: String,
    pub n: i64,
}
pub struct ActivityByDayBorrowed<'a> {
    pub day: i64,
    pub action: &'a str,
    pub n: i64,
}
impl<'a> From<ActivityByDayBorrowed<'a>> for ActivityByDay {
    fn from(ActivityByDayBorrowed { day, action, n }: ActivityByDayBorrowed<'a>) -> Self {
        Self { day, action: action.into(), n }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct RecentEvents {
    pub id: i64,
    pub ts: i64,
    pub action: String,
    pub path: String,
    pub detail: Option<String>,
    pub device: String,
}
pub struct RecentEventsBorrowed<'a> {
    pub id: i64,
    pub ts: i64,
    pub action: &'a str,
    pub path: &'a str,
    pub detail: Option<&'a str>,
    pub device: &'a str,
}
impl<'a> From<RecentEventsBorrowed<'a>> for RecentEvents {
    fn from(RecentEventsBorrowed { id, ts, action, path, detail, device }: RecentEventsBorrowed<'a>) -> Self {
        Self {
            id,
            ts,
            action: action.into(),
            path: path.into(),
            detail: detail.map(|v| v.into()),
            device: device.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct FileEvents {
    pub id: i64,
    pub ts: i64,
    pub action: String,
    pub path: String,
    pub detail: Option<String>,
    pub device: String,
}
pub struct FileEventsBorrowed<'a> {
    pub id: i64,
    pub ts: i64,
    pub action: &'a str,
    pub path: &'a str,
    pub detail: Option<&'a str>,
    pub device: &'a str,
}
impl<'a> From<FileEventsBorrowed<'a>> for FileEvents {
    fn from(FileEventsBorrowed { id, ts, action, path, detail, device }: FileEventsBorrowed<'a>) -> Self {
        Self {
            id,
            ts,
            action: action.into(),
            path: path.into(),
            detail: detail.map(|v| v.into()),
            device: device.into(),
        }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct ActivityByDayQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ActivityByDayBorrowed, postgres::Error>,
    mapper: fn(ActivityByDayBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ActivityByDayQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ActivityByDayBorrowed) -> R) -> ActivityByDayQuery<'c, 'a, 's, C, R, N> {
        ActivityByDayQuery {
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
pub struct RecentEventsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<RecentEventsBorrowed, postgres::Error>,
    mapper: fn(RecentEventsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> RecentEventsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(RecentEventsBorrowed) -> R) -> RecentEventsQuery<'c, 'a, 's, C, R, N> {
        RecentEventsQuery {
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
pub struct FileEventsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<FileEventsBorrowed, postgres::Error>,
    mapper: fn(FileEventsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FileEventsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(FileEventsBorrowed) -> R) -> FileEventsQuery<'c, 'a, 's, C, R, N> {
        FileEventsQuery {
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
pub struct ActivityByDayStmt(&'static str, Option<postgres::Statement>);
pub fn activity_by_day() -> ActivityByDayStmt {
    ActivityByDayStmt(
        "SELECT e.ts / 86400 AS day, e.action, COUNT(*) AS n FROM events e JOIN devices d ON d.id = e.device_id WHERE e.ts >= $1 AND ($2::boolean OR d.kind = 'master' OR d.user_id = $3::bigint) GROUP BY 1, 2",
        None,
    )
}
impl ActivityByDayStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        since: &'a i64,
        all: &'a bool,
        user: &'a i64,
    ) -> ActivityByDayQuery<'c, 'a, 's, C, ActivityByDay, 3> {
        ActivityByDayQuery {
            client,
            params: [since, all, user],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ActivityByDayBorrowed, postgres::Error> {
                Ok(ActivityByDayBorrowed { day: row.try_get(0)?, action: row.try_get(1)?, n: row.try_get(2)? })
            },
            mapper: |it| ActivityByDay::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, ActivityByDayParams, ActivityByDayQuery<'c, 'a, 's, C, ActivityByDay, 3>, C>
    for ActivityByDayStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a ActivityByDayParams,
    ) -> ActivityByDayQuery<'c, 'a, 's, C, ActivityByDay, 3> {
        self.bind(client, &params.since, &params.all, &params.user)
    }
}
pub struct RecentEventsStmt(&'static str, Option<postgres::Statement>);
pub fn recent_events() -> RecentEventsStmt {
    RecentEventsStmt(
        "SELECT e.id, e.ts, e.action, e.path, e.detail, d.folder AS device FROM events e JOIN devices d ON d.id = e.device_id WHERE ($1::text IS NULL OR d.folder = $1) AND ($2::boolean OR d.kind = 'master' OR d.user_id = $3::bigint) ORDER BY e.id DESC LIMIT $4",
        None,
    )
}
impl RecentEventsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        folder: &'a Option<T1>,
        all: &'a bool,
        user: &'a i64,
        limit: &'a i64,
    ) -> RecentEventsQuery<'c, 'a, 's, C, RecentEvents, 4> {
        RecentEventsQuery {
            client,
            params: [folder, all, user, limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<RecentEventsBorrowed, postgres::Error> {
                Ok(RecentEventsBorrowed {
                    id: row.try_get(0)?,
                    ts: row.try_get(1)?,
                    action: row.try_get(2)?,
                    path: row.try_get(3)?,
                    detail: row.try_get(4)?,
                    device: row.try_get(5)?,
                })
            },
            mapper: |it| RecentEvents::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        RecentEventsParams<T1>,
        RecentEventsQuery<'c, 'a, 's, C, RecentEvents, 4>,
        C,
    > for RecentEventsStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a RecentEventsParams<T1>,
    ) -> RecentEventsQuery<'c, 'a, 's, C, RecentEvents, 4> {
        self.bind(client, &params.folder, &params.all, &params.user, &params.limit)
    }
}
pub struct PurgeEventsStmt(&'static str, Option<postgres::Statement>);
pub fn purge_events() -> PurgeEventsStmt {
    PurgeEventsStmt("DELETE FROM events WHERE ts < $1", None)
}
impl PurgeEventsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        before: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[before])
    }
}
pub struct InsertEventStmt(&'static str, Option<postgres::Statement>);
pub fn insert_event() -> InsertEventStmt {
    InsertEventStmt(
        "INSERT INTO events (device_id, path, action, ts, hash, detail) VALUES ($1, $2, $3, $4, $5, $6)",
        None,
    )
}
impl InsertEventStmt {
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
        device_id: &'a i64,
        path: &'a T1,
        action: &'a T2,
        ts: &'a i64,
        hash: &'a Option<T3>,
        detail: &'a Option<T4>,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[device_id, path, action, ts, hash, detail])
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
> crate::client::sync::Params<'c, 'a, 's, InsertEventParams<T1, T2, T3, T4>, Result<u64, postgres::Error>, C>
    for InsertEventStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a InsertEventParams<T1, T2, T3, T4>,
    ) -> Result<u64, postgres::Error> {
        self.bind(client, &params.device_id, &params.path, &params.action, &params.ts, &params.hash, &params.detail)
    }
}
pub struct FileEventsStmt(&'static str, Option<postgres::Statement>);
pub fn file_events() -> FileEventsStmt {
    FileEventsStmt(
        "SELECT e.id, e.ts, e.action, e.path, e.detail, d.folder AS device FROM events e JOIN devices d ON d.id = e.device_id WHERE e.device_id = $1 AND e.path = $2 ORDER BY e.id DESC LIMIT $3",
        None,
    )
}
impl FileEventsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        path: &'a T1,
        limit: &'a i64,
    ) -> FileEventsQuery<'c, 'a, 's, C, FileEvents, 3> {
        FileEventsQuery {
            client,
            params: [device_id, path, limit],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<FileEventsBorrowed, postgres::Error> {
                Ok(FileEventsBorrowed {
                    id: row.try_get(0)?,
                    ts: row.try_get(1)?,
                    action: row.try_get(2)?,
                    path: row.try_get(3)?,
                    detail: row.try_get(4)?,
                    device: row.try_get(5)?,
                })
            },
            mapper: |it| FileEvents::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, FileEventsParams<T1>, FileEventsQuery<'c, 'a, 's, C, FileEvents, 3>, C>
    for FileEventsStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a FileEventsParams<T1>,
    ) -> FileEventsQuery<'c, 'a, 's, C, FileEvents, 3> {
        self.bind(client, &params.device_id, &params.path, &params.limit)
    }
}
