// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct SetSettingParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub key: T1,
    pub value: T2,
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
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
pub struct GetSettingStmt(&'static str, Option<postgres::Statement>);
pub fn get_setting() -> GetSettingStmt {
    GetSettingStmt("SELECT value FROM settings WHERE key = $1", None)
}
impl GetSettingStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        key: &'a T1,
    ) -> StringQuery<'c, 'a, 's, C, String, 1> {
        StringQuery {
            client,
            params: [key],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
pub struct SetSettingStmt(&'static str, Option<postgres::Statement>);
pub fn set_setting() -> SetSettingStmt {
    SetSettingStmt(
        "INSERT INTO settings (key, value) VALUES ($1, $2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        None,
    )
}
impl SetSettingStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        key: &'a T1,
        value: &'a T2,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[key, value])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, SetSettingParams<T1, T2>, Result<u64, postgres::Error>, C>
    for SetSettingStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetSettingParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.key, &params.value)
    }
}
