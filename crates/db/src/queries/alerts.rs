// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct UpsertAlertParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub kind: T1,
    pub key: T2,
    pub message: T3,
    pub now: i64,
}
#[derive(Debug)]
pub struct ResolveAlertParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub now: i64,
    pub kind: T1,
    pub key: T2,
}
#[derive(Debug)]
pub struct ResolveStaleAlertsParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub now: i64,
    pub kind: T1,
    pub key: T2,
    pub before: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct ExpireUploadErrorsParams {
    pub now: i64,
    pub before: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct DismissAlertParams {
    pub now: i64,
    pub id: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct OpenAlerts {
    pub id: i64,
    pub kind: String,
    pub key: String,
    pub message: String,
    pub created_at: i64,
    pub updated_at: Option<i64>,
}
pub struct OpenAlertsBorrowed<'a> {
    pub id: i64,
    pub kind: &'a str,
    pub key: &'a str,
    pub message: &'a str,
    pub created_at: i64,
    pub updated_at: Option<i64>,
}
impl<'a> From<OpenAlertsBorrowed<'a>> for OpenAlerts {
    fn from(OpenAlertsBorrowed { id, kind, key, message, created_at, updated_at }: OpenAlertsBorrowed<'a>) -> Self {
        Self { id, kind: kind.into(), key: key.into(), message: message.into(), created_at, updated_at }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct OpenAlertsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<OpenAlertsBorrowed, postgres::Error>,
    mapper: fn(OpenAlertsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> OpenAlertsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(OpenAlertsBorrowed) -> R) -> OpenAlertsQuery<'c, 'a, 's, C, R, N> {
        OpenAlertsQuery {
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
pub struct UpsertAlertStmt(&'static str, Option<postgres::Statement>);
pub fn upsert_alert() -> UpsertAlertStmt {
    UpsertAlertStmt(
        "INSERT INTO alerts (kind, key, message, created_at, updated_at) VALUES ($1, $2, $3, $4, $4) ON CONFLICT (kind, key) WHERE resolved_at IS NULL DO UPDATE SET message = excluded.message, updated_at = excluded.updated_at",
        None,
    )
}
impl UpsertAlertStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        kind: &'a T1,
        key: &'a T2,
        message: &'a T3,
        now: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[kind, key, message, now])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, UpsertAlertParams<T1, T2, T3>, Result<u64, postgres::Error>, C>
    for UpsertAlertStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a UpsertAlertParams<T1, T2, T3>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.kind, &params.key, &params.message, &params.now)
    }
}
pub struct ResolveAlertStmt(&'static str, Option<postgres::Statement>);
pub fn resolve_alert() -> ResolveAlertStmt {
    ResolveAlertStmt("UPDATE alerts SET resolved_at = $1 WHERE kind = $2 AND key = $3 AND resolved_at IS NULL", None)
}
impl ResolveAlertStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        kind: &'a T1,
        key: &'a T2,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now, kind, key])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, ResolveAlertParams<T1, T2>, Result<u64, postgres::Error>, C>
    for ResolveAlertStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a ResolveAlertParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.kind, &params.key)
    }
}
pub struct ResolveStaleAlertsStmt(&'static str, Option<postgres::Statement>);
pub fn resolve_stale_alerts() -> ResolveStaleAlertsStmt {
    ResolveStaleAlertsStmt(
        "UPDATE alerts SET resolved_at = $1 WHERE kind = $2 AND key = $3 AND resolved_at IS NULL AND updated_at < $4",
        None,
    )
}
impl ResolveStaleAlertsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        kind: &'a T1,
        key: &'a T2,
        before: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now, kind, key, before])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, ResolveStaleAlertsParams<T1, T2>, Result<u64, postgres::Error>, C>
    for ResolveStaleAlertsStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a ResolveStaleAlertsParams<T1, T2>,
    ) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.kind, &params.key, &params.before)
    }
}
pub struct ExpireUploadErrorsStmt(&'static str, Option<postgres::Statement>);
pub fn expire_upload_errors() -> ExpireUploadErrorsStmt {
    ExpireUploadErrorsStmt(
        "UPDATE alerts SET resolved_at = $1 WHERE kind = 'upload_error' AND resolved_at IS NULL AND COALESCE(updated_at, created_at) < $2",
        None,
    )
}
impl ExpireUploadErrorsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        before: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now, before])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, ExpireUploadErrorsParams, Result<u64, postgres::Error>, C>
    for ExpireUploadErrorsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a ExpireUploadErrorsParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.before)
    }
}
pub struct OpenAlertsStmt(&'static str, Option<postgres::Statement>);
pub fn open_alerts() -> OpenAlertsStmt {
    OpenAlertsStmt(
        "SELECT id, kind, key, message, created_at, updated_at FROM alerts WHERE resolved_at IS NULL ORDER BY id DESC",
        None,
    )
}
impl OpenAlertsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> OpenAlertsQuery<'c, 'a, 's, C, OpenAlerts, 0> {
        OpenAlertsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<OpenAlertsBorrowed, postgres::Error> {
                Ok(OpenAlertsBorrowed {
                    id: row.try_get(0)?,
                    kind: row.try_get(1)?,
                    key: row.try_get(2)?,
                    message: row.try_get(3)?,
                    created_at: row.try_get(4)?,
                    updated_at: row.try_get(5)?,
                })
            },
            mapper: |it| OpenAlerts::from(it),
        }
    }
}
pub struct AlertKeyStmt(&'static str, Option<postgres::Statement>);
pub fn alert_key() -> AlertKeyStmt {
    AlertKeyStmt("SELECT key FROM alerts WHERE id = $1", None)
}
impl AlertKeyStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        id: &'a i64,
    ) -> StringQuery<'c, 'a, 's, C, String, 1> {
        StringQuery {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct DismissAlertStmt(&'static str, Option<postgres::Statement>);
pub fn dismiss_alert() -> DismissAlertStmt {
    DismissAlertStmt("UPDATE alerts SET resolved_at = $1 WHERE id = $2 AND resolved_at IS NULL", None)
}
impl DismissAlertStmt {
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
    crate::client::sync::Params<'c, 'a, 's, DismissAlertParams, Result<u64, postgres::Error>, C> for DismissAlertStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a DismissAlertParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.id)
    }
}
pub struct PurgeResolvedAlertsStmt(&'static str, Option<postgres::Statement>);
pub fn purge_resolved_alerts() -> PurgeResolvedAlertsStmt {
    PurgeResolvedAlertsStmt("DELETE FROM alerts WHERE resolved_at IS NOT NULL AND resolved_at < $1", None)
}
impl PurgeResolvedAlertsStmt {
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
