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
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
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
