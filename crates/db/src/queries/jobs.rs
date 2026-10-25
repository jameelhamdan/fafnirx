// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct EnqueueJobParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub kind: T1,
    pub payload: T2,
    pub now: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct ClaimJobParams {
    pub now: i64,
    pub with_proxies: bool,
}
#[derive(Debug)]
pub struct RetryJobParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub status: T1,
    pub run_after: i64,
    pub error: T2,
    pub id: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimJob {
    pub id: i64,
    pub kind: String,
    pub payload: String,
    pub attempts: i64,
}
pub struct ClaimJobBorrowed<'a> {
    pub id: i64,
    pub kind: &'a str,
    pub payload: &'a str,
    pub attempts: i64,
}
impl<'a> From<ClaimJobBorrowed<'a>> for ClaimJob {
    fn from(ClaimJobBorrowed { id, kind, payload, attempts }: ClaimJobBorrowed<'a>) -> Self {
        Self { id, kind: kind.into(), payload: payload.into(), attempts }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct ClaimJobQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ClaimJobBorrowed, postgres::Error>,
    mapper: fn(ClaimJobBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ClaimJobQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ClaimJobBorrowed) -> R) -> ClaimJobQuery<'c, 'a, 's, C, R, N> {
        ClaimJobQuery {
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
pub struct RequeueRunningJobsStmt(&'static str, Option<postgres::Statement>);
pub fn requeue_running_jobs() -> RequeueRunningJobsStmt {
    RequeueRunningJobsStmt("UPDATE jobs SET status = 'queued' WHERE status = 'running'", None)
}
impl RequeueRunningJobsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(&'s self, client: &'c mut C) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[])
    }
}
pub struct ClaimJobStmt(&'static str, Option<postgres::Statement>);
pub fn claim_job() -> ClaimJobStmt {
    ClaimJobStmt(
        "UPDATE jobs SET status = 'running', attempts = attempts + 1 WHERE id = (SELECT id FROM jobs WHERE status = 'queued' AND run_after <= $1 AND ($2::boolean OR kind <> 'proxy') ORDER BY (kind = 'proxy'), id LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING id, kind, payload, attempts",
        None,
    )
}
impl ClaimJobStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        with_proxies: &'a bool,
    ) -> ClaimJobQuery<'c, 'a, 's, C, ClaimJob, 2> {
        ClaimJobQuery {
            client,
            params: [now, with_proxies],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ClaimJobBorrowed, postgres::Error> {
                Ok(ClaimJobBorrowed {
                    id: row.try_get(0)?,
                    kind: row.try_get(1)?,
                    payload: row.try_get(2)?,
                    attempts: row.try_get(3)?,
                })
            },
            mapper: |it| ClaimJob::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, ClaimJobParams, ClaimJobQuery<'c, 'a, 's, C, ClaimJob, 2>, C>
    for ClaimJobStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a ClaimJobParams) -> ClaimJobQuery<'c, 'a, 's, C, ClaimJob, 2> {
        self.bind(client, &params.now, &params.with_proxies)
    }
}
pub struct CompleteJobStmt(&'static str, Option<postgres::Statement>);
pub fn complete_job() -> CompleteJobStmt {
    CompleteJobStmt("DELETE FROM jobs WHERE id = $1", None)
}
impl CompleteJobStmt {
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
pub struct RetryJobStmt(&'static str, Option<postgres::Statement>);
pub fn retry_job() -> RetryJobStmt {
    RetryJobStmt("UPDATE jobs SET status = $1, run_after = $2, error = $3 WHERE id = $4", None)
}
impl RetryJobStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        status: &'a T1,
        run_after: &'a i64,
        error: &'a T2,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[status, run_after, error, id])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, RetryJobParams<T1, T2>, Result<u64, postgres::Error>, C> for RetryJobStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a RetryJobParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.status, &params.run_after, &params.error, &params.id)
    }
}
