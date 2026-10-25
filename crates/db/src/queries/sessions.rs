// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct SessionUserParams<T1: crate::StringSql> {
    pub token_hash: T1,
    pub now: i64,
}
#[derive(Debug)]
pub struct InsertSessionParams<T1: crate::StringSql> {
    pub token_hash: T1,
    pub user_id: i64,
    pub created_at: i64,
    pub expires_at: i64,
}
#[derive(Debug)]
pub struct DeleteUserSessionsExceptParams<T1: crate::StringSql> {
    pub user_id: i64,
    pub keep: T1,
}
#[derive(Debug, Clone, PartialEq)]
pub struct SessionUser {
    pub id: i64,
    pub name: String,
    pub role: String,
}
pub struct SessionUserBorrowed<'a> {
    pub id: i64,
    pub name: &'a str,
    pub role: &'a str,
}
impl<'a> From<SessionUserBorrowed<'a>> for SessionUser {
    fn from(SessionUserBorrowed { id, name, role }: SessionUserBorrowed<'a>) -> Self {
        Self { id, name: name.into(), role: role.into() }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct SessionUserQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<SessionUserBorrowed, postgres::Error>,
    mapper: fn(SessionUserBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> SessionUserQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(SessionUserBorrowed) -> R) -> SessionUserQuery<'c, 'a, 's, C, R, N> {
        SessionUserQuery {
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
pub struct SessionUserStmt(&'static str, Option<postgres::Statement>);
pub fn session_user() -> SessionUserStmt {
    SessionUserStmt(
        "SELECT u.id, u.name, u.role FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_hash = $1 AND s.expires_at > $2 AND u.disabled_at IS NULL",
        None,
    )
}
impl SessionUserStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        token_hash: &'a T1,
        now: &'a i64,
    ) -> SessionUserQuery<'c, 'a, 's, C, SessionUser, 2> {
        SessionUserQuery {
            client,
            params: [token_hash, now],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<SessionUserBorrowed, postgres::Error> {
                Ok(SessionUserBorrowed { id: row.try_get(0)?, name: row.try_get(1)?, role: row.try_get(2)? })
            },
            mapper: |it| SessionUser::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, SessionUserParams<T1>, SessionUserQuery<'c, 'a, 's, C, SessionUser, 2>, C>
    for SessionUserStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a SessionUserParams<T1>,
    ) -> SessionUserQuery<'c, 'a, 's, C, SessionUser, 2> {
        self.bind(client, &params.token_hash, &params.now)
    }
}
pub struct DeleteExpiredSessionsStmt(&'static str, Option<postgres::Statement>);
pub fn delete_expired_sessions() -> DeleteExpiredSessionsStmt {
    DeleteExpiredSessionsStmt("DELETE FROM sessions WHERE expires_at <= $1", None)
}
impl DeleteExpiredSessionsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now])
    }
}
pub struct InsertSessionStmt(&'static str, Option<postgres::Statement>);
pub fn insert_session() -> InsertSessionStmt {
    InsertSessionStmt(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES ($1, $2, $3, $4)",
        None,
    )
}
impl InsertSessionStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        token_hash: &'a T1,
        user_id: &'a i64,
        created_at: &'a i64,
        expires_at: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[token_hash, user_id, created_at, expires_at])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, InsertSessionParams<T1>, Result<u64, postgres::Error>, C>
    for InsertSessionStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a InsertSessionParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.token_hash, &params.user_id, &params.created_at, &params.expires_at)
    }
}
pub struct DeleteSessionStmt(&'static str, Option<postgres::Statement>);
pub fn delete_session() -> DeleteSessionStmt {
    DeleteSessionStmt("DELETE FROM sessions WHERE token_hash = $1", None)
}
impl DeleteSessionStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        token_hash: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[token_hash])
    }
}
pub struct DeleteUserSessionsStmt(&'static str, Option<postgres::Statement>);
pub fn delete_user_sessions() -> DeleteUserSessionsStmt {
    DeleteUserSessionsStmt("DELETE FROM sessions WHERE user_id = $1", None)
}
impl DeleteUserSessionsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        user_id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[user_id])
    }
}
pub struct DeleteUserSessionsExceptStmt(&'static str, Option<postgres::Statement>);
pub fn delete_user_sessions_except() -> DeleteUserSessionsExceptStmt {
    DeleteUserSessionsExceptStmt("DELETE FROM sessions WHERE user_id = $1 AND token_hash <> $2", None)
}
impl DeleteUserSessionsExceptStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        user_id: &'a i64,
        keep: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[user_id, keep])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, DeleteUserSessionsExceptParams<T1>, Result<u64, postgres::Error>, C>
    for DeleteUserSessionsExceptStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a DeleteUserSessionsExceptParams<T1>,
    ) -> Result<u64, postgres::Error> {
        self.bind(client, &params.user_id, &params.keep)
    }
}
