// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct InsertUserParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub name: T1,
    pub role: T2,
    pub password_hash: T3,
    pub now: i64,
}
#[derive(Debug)]
pub struct SetUserRoleParams<T1: crate::StringSql> {
    pub role: T1,
    pub id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct DisableUserParams {
    pub now: i64,
    pub id: i64,
}
#[derive(Debug)]
pub struct SetPasswordHashParams<T1: crate::StringSql> {
    pub password_hash: T1,
    pub id: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ListUsers {
    pub name: String,
    pub role: String,
    pub disabled: bool,
    pub created_at: i64,
    pub devices: i64,
}
pub struct ListUsersBorrowed<'a> {
    pub name: &'a str,
    pub role: &'a str,
    pub disabled: bool,
    pub created_at: i64,
    pub devices: i64,
}
impl<'a> From<ListUsersBorrowed<'a>> for ListUsers {
    fn from(ListUsersBorrowed { name, role, disabled, created_at, devices }: ListUsersBorrowed<'a>) -> Self {
        Self { name: name.into(), role: role.into(), disabled, created_at, devices }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct UserByName {
    pub id: i64,
    pub role: String,
    pub disabled: bool,
}
pub struct UserByNameBorrowed<'a> {
    pub id: i64,
    pub role: &'a str,
    pub disabled: bool,
}
impl<'a> From<UserByNameBorrowed<'a>> for UserByName {
    fn from(UserByNameBorrowed { id, role, disabled }: UserByNameBorrowed<'a>) -> Self {
        Self { id, role: role.into(), disabled }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct AdminCredentials {
    pub id: i64,
    pub password_hash: String,
}
pub struct AdminCredentialsBorrowed<'a> {
    pub id: i64,
    pub password_hash: &'a str,
}
impl<'a> From<AdminCredentialsBorrowed<'a>> for AdminCredentials {
    fn from(AdminCredentialsBorrowed { id, password_hash }: AdminCredentialsBorrowed<'a>) -> Self {
        Self { id, password_hash: password_hash.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct LoginCredentials {
    pub id: i64,
    pub password_hash: String,
}
pub struct LoginCredentialsBorrowed<'a> {
    pub id: i64,
    pub password_hash: &'a str,
}
impl<'a> From<LoginCredentialsBorrowed<'a>> for LoginCredentials {
    fn from(LoginCredentialsBorrowed { id, password_hash }: LoginCredentialsBorrowed<'a>) -> Self {
        Self { id, password_hash: password_hash.into() }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct ListUsersQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ListUsersBorrowed, postgres::Error>,
    mapper: fn(ListUsersBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ListUsersQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ListUsersBorrowed) -> R) -> ListUsersQuery<'c, 'a, 's, C, R, N> {
        ListUsersQuery {
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
pub struct UserByNameQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<UserByNameBorrowed, postgres::Error>,
    mapper: fn(UserByNameBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> UserByNameQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(UserByNameBorrowed) -> R) -> UserByNameQuery<'c, 'a, 's, C, R, N> {
        UserByNameQuery {
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
pub struct AdminCredentialsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<AdminCredentialsBorrowed, postgres::Error>,
    mapper: fn(AdminCredentialsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> AdminCredentialsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(AdminCredentialsBorrowed) -> R) -> AdminCredentialsQuery<'c, 'a, 's, C, R, N> {
        AdminCredentialsQuery {
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
pub struct LoginCredentialsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<LoginCredentialsBorrowed, postgres::Error>,
    mapper: fn(LoginCredentialsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> LoginCredentialsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(LoginCredentialsBorrowed) -> R) -> LoginCredentialsQuery<'c, 'a, 's, C, R, N> {
        LoginCredentialsQuery {
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
pub struct ListUsersStmt(&'static str, Option<postgres::Statement>);
pub fn list_users() -> ListUsersStmt {
    ListUsersStmt(
        "SELECT u.name, u.role, u.disabled_at IS NOT NULL AS disabled, u.created_at, (SELECT COUNT(*) FROM devices d WHERE d.user_id = u.id AND d.kind IN ('device', 'local')) AS devices FROM users u ORDER BY u.name",
        None,
    )
}
impl ListUsersStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> ListUsersQuery<'c, 'a, 's, C, ListUsers, 0> {
        ListUsersQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ListUsersBorrowed, postgres::Error> {
                Ok(ListUsersBorrowed {
                    name: row.try_get(0)?,
                    role: row.try_get(1)?,
                    disabled: row.try_get(2)?,
                    created_at: row.try_get(3)?,
                    devices: row.try_get(4)?,
                })
            },
            mapper: |it| ListUsers::from(it),
        }
    }
}
pub struct UserNamesStmt(&'static str, Option<postgres::Statement>);
pub fn user_names() -> UserNamesStmt {
    UserNamesStmt("SELECT name FROM users", None)
}
impl UserNamesStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(&'s self, client: &'c mut C) -> StringQuery<'c, 'a, 's, C, String, 0> {
        StringQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct UserExistsStmt(&'static str, Option<postgres::Statement>);
pub fn user_exists() -> UserExistsStmt {
    UserExistsStmt("SELECT 1 FROM users WHERE name = $1", None)
}
impl UserExistsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
    ) -> I32Query<'c, 'a, 's, C, i32, 1> {
        I32Query {
            client,
            params: [name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct MasterPathTakenStmt(&'static str, Option<postgres::Statement>);
pub fn master_path_taken() -> MasterPathTakenStmt {
    MasterPathTakenStmt(
        "SELECT EXISTS (SELECT 1 FROM files WHERE device_id = 0 AND lower(split_part(path, '/', 1)) = $1) AS taken",
        None,
    )
}
impl MasterPathTakenStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 1> {
        BoolQuery {
            client,
            params: [name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct InsertUserStmt(&'static str, Option<postgres::Statement>);
pub fn insert_user() -> InsertUserStmt {
    InsertUserStmt("INSERT INTO users (name, role, password_hash, created_at) VALUES ($1, $2, $3, $4)", None)
}
impl InsertUserStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
        role: &'a T2,
        password_hash: &'a T3,
        now: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[name, role, password_hash, now])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, InsertUserParams<T1, T2, T3>, Result<u64, postgres::Error>, C>
    for InsertUserStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a InsertUserParams<T1, T2, T3>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.name, &params.role, &params.password_hash, &params.now)
    }
}
pub struct LockUsersStmt(&'static str, Option<postgres::Statement>);
pub fn lock_users() -> LockUsersStmt {
    LockUsersStmt("LOCK TABLE users IN SHARE ROW EXCLUSIVE MODE", None)
}
impl LockUsersStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(&'s self, client: &'c mut C) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[])
    }
}
pub struct UserByNameStmt(&'static str, Option<postgres::Statement>);
pub fn user_by_name() -> UserByNameStmt {
    UserByNameStmt("SELECT id, role, disabled_at IS NOT NULL AS disabled FROM users WHERE name = $1", None)
}
impl UserByNameStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
    ) -> UserByNameQuery<'c, 'a, 's, C, UserByName, 1> {
        UserByNameQuery {
            client,
            params: [name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<UserByNameBorrowed, postgres::Error> {
                Ok(UserByNameBorrowed { id: row.try_get(0)?, role: row.try_get(1)?, disabled: row.try_get(2)? })
            },
            mapper: |it| UserByName::from(it),
        }
    }
}
pub struct OtherActiveAdminsStmt(&'static str, Option<postgres::Statement>);
pub fn other_active_admins() -> OtherActiveAdminsStmt {
    OtherActiveAdminsStmt(
        "SELECT COUNT(*) AS others FROM users WHERE role = 'admin' AND disabled_at IS NULL AND id <> $1",
        None,
    )
}
impl OtherActiveAdminsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        id: &'a i64,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct SetUserRoleStmt(&'static str, Option<postgres::Statement>);
pub fn set_user_role() -> SetUserRoleStmt {
    SetUserRoleStmt("UPDATE users SET role = $1 WHERE id = $2", None)
}
impl SetUserRoleStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        role: &'a T1,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[role, id])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, SetUserRoleParams<T1>, Result<u64, postgres::Error>, C>
    for SetUserRoleStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetUserRoleParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.role, &params.id)
    }
}
pub struct DisableUserStmt(&'static str, Option<postgres::Statement>);
pub fn disable_user() -> DisableUserStmt {
    DisableUserStmt("UPDATE users SET disabled_at = COALESCE(disabled_at, $1) WHERE id = $2", None)
}
impl DisableUserStmt {
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
    crate::client::sync::Params<'c, 'a, 's, DisableUserParams, Result<u64, postgres::Error>, C> for DisableUserStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a DisableUserParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.id)
    }
}
pub struct EnableUserStmt(&'static str, Option<postgres::Statement>);
pub fn enable_user() -> EnableUserStmt {
    EnableUserStmt("UPDATE users SET disabled_at = NULL WHERE id = $1", None)
}
impl EnableUserStmt {
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
pub struct SetPasswordHashStmt(&'static str, Option<postgres::Statement>);
pub fn set_password_hash() -> SetPasswordHashStmt {
    SetPasswordHashStmt("UPDATE users SET password_hash = $1 WHERE id = $2", None)
}
impl SetPasswordHashStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        password_hash: &'a T1,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[password_hash, id])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, SetPasswordHashParams<T1>, Result<u64, postgres::Error>, C>
    for SetPasswordHashStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetPasswordHashParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.password_hash, &params.id)
    }
}
pub struct AdminCredentialsStmt(&'static str, Option<postgres::Statement>);
pub fn admin_credentials() -> AdminCredentialsStmt {
    AdminCredentialsStmt("SELECT id, password_hash FROM users WHERE name = 'admin'", None)
}
impl AdminCredentialsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> AdminCredentialsQuery<'c, 'a, 's, C, AdminCredentials, 0> {
        AdminCredentialsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<AdminCredentialsBorrowed, postgres::Error> {
                Ok(AdminCredentialsBorrowed { id: row.try_get(0)?, password_hash: row.try_get(1)? })
            },
            mapper: |it| AdminCredentials::from(it),
        }
    }
}
pub struct LoginCredentialsStmt(&'static str, Option<postgres::Statement>);
pub fn login_credentials() -> LoginCredentialsStmt {
    LoginCredentialsStmt("SELECT id, password_hash FROM users WHERE name = $1 AND disabled_at IS NULL", None)
}
impl LoginCredentialsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
    ) -> LoginCredentialsQuery<'c, 'a, 's, C, LoginCredentials, 1> {
        LoginCredentialsQuery {
            client,
            params: [name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<LoginCredentialsBorrowed, postgres::Error> {
                Ok(LoginCredentialsBorrowed { id: row.try_get(0)?, password_hash: row.try_get(1)? })
            },
            mapper: |it| LoginCredentials::from(it),
        }
    }
}
pub struct PasswordHashOfStmt(&'static str, Option<postgres::Statement>);
pub fn password_hash_of() -> PasswordHashOfStmt {
    PasswordHashOfStmt("SELECT password_hash FROM users WHERE id = $1", None)
}
impl PasswordHashOfStmt {
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
pub struct UserWhoCanHaveDevicesStmt(&'static str, Option<postgres::Statement>);
pub fn user_who_can_have_devices() -> UserWhoCanHaveDevicesStmt {
    UserWhoCanHaveDevicesStmt(
        "SELECT id FROM users WHERE name = $1 AND disabled_at IS NULL AND role <> 'readonly'",
        None,
    )
}
impl UserWhoCanHaveDevicesStmt {
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
