// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct EnqueueJobParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub kind: T1,
    pub payload: T2,
    pub now: i64,
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct EnqueueJobStmt(&'static str, Option<postgres::Statement>);
pub fn enqueue_job() -> EnqueueJobStmt {
    EnqueueJobStmt("INSERT INTO jobs (kind, payload, created_at) VALUES ($1, $2, $3)", None)
}
impl EnqueueJobStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        kind: &'a T1,
        payload: &'a T2,
        now: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[kind, payload, now])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, EnqueueJobParams<T1, T2>, Result<u64, postgres::Error>, C>
    for EnqueueJobStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a EnqueueJobParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.kind, &params.payload, &params.now)
    }
}
