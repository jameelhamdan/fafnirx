// This file was generated with `cornucopia`. Do not modify.

#[derive(Debug)]
pub struct TakeOverDeviceParams<T1: crate::StringSql> {
    pub instance: T1,
    pub now: i64,
    pub id: i64,
    pub lease_start: i64,
}
#[derive(Debug)]
pub struct RecordHeartbeatParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql, T4: crate::StringSql>
{
    pub hostname: T1,
    pub os: T2,
    pub ip: Option<T3>,
    pub client_version: T4,
    pub disk_total: i64,
    pub disk_free: i64,
    pub now: i64,
    pub id: i64,
}
#[derive(Debug)]
pub struct SaveActivityParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub device_id: i64,
    pub now: i64,
    pub phase: T1,
    pub current_file: T2,
    pub files_done: i64,
    pub files_total: i64,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub speed_bps: i64,
}
#[derive(Debug)]
pub struct DeviceSummariesParams<T1: crate::StringSql> {
    pub folder: Option<T1>,
    pub all: bool,
    pub user: i64,
}
#[derive(Debug)]
pub struct InsertDeviceParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub name: T1,
    pub kind: T2,
    pub token_hash: T3,
    pub now: i64,
    pub user_id: i64,
}
#[derive(Debug)]
pub struct SetDeviceTokenParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub token_hash: T1,
    pub folder: T2,
}
#[derive(Debug)]
pub struct RevokeDeviceParams<T1: crate::StringSql> {
    pub now: i64,
    pub folder: T1,
}
#[derive(Clone, Copy, Debug)]
pub struct UpdateTotalsParams {
    pub now: i64,
    pub id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct VisibleTotalsParams {
    pub all: bool,
    pub user: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceByToken {
    pub id: i64,
    pub folder: String,
    pub instance_id: Option<String>,
    pub hostname: Option<String>,
    pub ip: Option<String>,
}
pub struct DeviceByTokenBorrowed<'a> {
    pub id: i64,
    pub folder: &'a str,
    pub instance_id: Option<&'a str>,
    pub hostname: Option<&'a str>,
    pub ip: Option<&'a str>,
}
impl<'a> From<DeviceByTokenBorrowed<'a>> for DeviceByToken {
    fn from(DeviceByTokenBorrowed { id, folder, instance_id, hostname, ip }: DeviceByTokenBorrowed<'a>) -> Self {
        Self {
            id,
            folder: folder.into(),
            instance_id: instance_id.map(|v| v.into()),
            hostname: hostname.map(|v| v.into()),
            ip: ip.map(|v| v.into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceSummaries {
    pub name: String,
    pub kind: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub hostname: Option<String>,
    pub os: Option<String>,
    pub ip: Option<String>,
    pub client_version: Option<String>,
    pub disk_total: Option<i64>,
    pub disk_free: Option<i64>,
    pub last_seen: Option<i64>,
    pub last_collected: Option<i64>,
    pub total_bytes: i64,
    pub file_count: i64,
    pub activity_at: Option<i64>,
    pub phase: Option<String>,
    pub current_file: Option<String>,
    pub files_done: Option<i64>,
    pub files_total: Option<i64>,
    pub bytes_done: Option<i64>,
    pub bytes_total: Option<i64>,
    pub speed_bps: Option<i64>,
    pub user_name: String,
    pub folder: String,
}
pub struct DeviceSummariesBorrowed<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub hostname: Option<&'a str>,
    pub os: Option<&'a str>,
    pub ip: Option<&'a str>,
    pub client_version: Option<&'a str>,
    pub disk_total: Option<i64>,
    pub disk_free: Option<i64>,
    pub last_seen: Option<i64>,
    pub last_collected: Option<i64>,
    pub total_bytes: i64,
    pub file_count: i64,
    pub activity_at: Option<i64>,
    pub phase: Option<&'a str>,
    pub current_file: Option<&'a str>,
    pub files_done: Option<i64>,
    pub files_total: Option<i64>,
    pub bytes_done: Option<i64>,
    pub bytes_total: Option<i64>,
    pub speed_bps: Option<i64>,
    pub user_name: &'a str,
    pub folder: &'a str,
}
impl<'a> From<DeviceSummariesBorrowed<'a>> for DeviceSummaries {
    fn from(
        DeviceSummariesBorrowed {
            name,
            kind,
            created_at,
            revoked_at,
            hostname,
            os,
            ip,
            client_version,
            disk_total,
            disk_free,
            last_seen,
            last_collected,
            total_bytes,
            file_count,
            activity_at,
            phase,
            current_file,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
            speed_bps,
            user_name,
            folder,
        }: DeviceSummariesBorrowed<'a>,
    ) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            created_at,
            revoked_at,
            hostname: hostname.map(|v| v.into()),
            os: os.map(|v| v.into()),
            ip: ip.map(|v| v.into()),
            client_version: client_version.map(|v| v.into()),
            disk_total,
            disk_free,
            last_seen,
            last_collected,
            total_bytes,
            file_count,
            activity_at,
            phase: phase.map(|v| v.into()),
            current_file: current_file.map(|v| v.into()),
            files_done,
            files_total,
            bytes_done,
            bytes_total,
            speed_bps,
            user_name: user_name.into(),
            folder: folder.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct UserForDevice {
    pub id: i64,
    pub role: String,
}
pub struct UserForDeviceBorrowed<'a> {
    pub id: i64,
    pub role: &'a str,
}
impl<'a> From<UserForDeviceBorrowed<'a>> for UserForDevice {
    fn from(UserForDeviceBorrowed { id, role }: UserForDeviceBorrowed<'a>) -> Self {
        Self { id, role: role.into() }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ClientsLastSeen {
    pub folder: String,
    pub last_seen: i64,
}
pub struct ClientsLastSeenBorrowed<'a> {
    pub folder: &'a str,
    pub last_seen: i64,
}
impl<'a> From<ClientsLastSeenBorrowed<'a>> for ClientsLastSeen {
    fn from(ClientsLastSeenBorrowed { folder, last_seen }: ClientsLastSeenBorrowed<'a>) -> Self {
        Self { folder: folder.into(), last_seen }
    }
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct VisibleTotals {
    pub files: i64,
    pub bytes: i64,
    pub deleted: i64,
}
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct MasterTotals {
    pub file_count: i64,
    pub total_bytes: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceByFolder {
    pub id: i64,
    pub folder: String,
}
pub struct DeviceByFolderBorrowed<'a> {
    pub id: i64,
    pub folder: &'a str,
}
impl<'a> From<DeviceByFolderBorrowed<'a>> for DeviceByFolder {
    fn from(DeviceByFolderBorrowed { id, folder }: DeviceByFolderBorrowed<'a>) -> Self {
        Self { id, folder: folder.into() }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct DeviceByTokenQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<DeviceByTokenBorrowed, postgres::Error>,
    mapper: fn(DeviceByTokenBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> DeviceByTokenQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(DeviceByTokenBorrowed) -> R) -> DeviceByTokenQuery<'c, 'a, 's, C, R, N> {
        DeviceByTokenQuery {
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
pub struct DeviceSummariesQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<DeviceSummariesBorrowed, postgres::Error>,
    mapper: fn(DeviceSummariesBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> DeviceSummariesQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(DeviceSummariesBorrowed) -> R) -> DeviceSummariesQuery<'c, 'a, 's, C, R, N> {
        DeviceSummariesQuery {
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
pub struct UserForDeviceQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<UserForDeviceBorrowed, postgres::Error>,
    mapper: fn(UserForDeviceBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> UserForDeviceQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(UserForDeviceBorrowed) -> R) -> UserForDeviceQuery<'c, 'a, 's, C, R, N> {
        UserForDeviceQuery {
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
pub struct ClientsLastSeenQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<ClientsLastSeenBorrowed, postgres::Error>,
    mapper: fn(ClientsLastSeenBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> ClientsLastSeenQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(ClientsLastSeenBorrowed) -> R) -> ClientsLastSeenQuery<'c, 'a, 's, C, R, N> {
        ClientsLastSeenQuery {
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
pub struct VisibleTotalsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<VisibleTotals, postgres::Error>,
    mapper: fn(VisibleTotals) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> VisibleTotalsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(VisibleTotals) -> R) -> VisibleTotalsQuery<'c, 'a, 's, C, R, N> {
        VisibleTotalsQuery {
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
pub struct MasterTotalsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<MasterTotals, postgres::Error>,
    mapper: fn(MasterTotals) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> MasterTotalsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(MasterTotals) -> R) -> MasterTotalsQuery<'c, 'a, 's, C, R, N> {
        MasterTotalsQuery {
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
pub struct DeviceByFolderQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<DeviceByFolderBorrowed, postgres::Error>,
    mapper: fn(DeviceByFolderBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> DeviceByFolderQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(DeviceByFolderBorrowed) -> R) -> DeviceByFolderQuery<'c, 'a, 's, C, R, N> {
        DeviceByFolderQuery {
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
pub struct DeviceByTokenStmt(&'static str, Option<postgres::Statement>);
pub fn device_by_token() -> DeviceByTokenStmt {
    DeviceByTokenStmt(
        "SELECT d.id, d.folder, d.instance_id, d.hostname, d.ip FROM devices d JOIN users u ON u.id = d.user_id WHERE d.token_hash = $1 AND d.revoked_at IS NULL AND d.kind = 'device' AND u.disabled_at IS NULL",
        None,
    )
}
impl DeviceByTokenStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        token_hash: &'a T1,
    ) -> DeviceByTokenQuery<'c, 'a, 's, C, DeviceByToken, 1> {
        DeviceByTokenQuery {
            client,
            params: [token_hash],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<DeviceByTokenBorrowed, postgres::Error> {
                Ok(DeviceByTokenBorrowed {
                    id: row.try_get(0)?,
                    folder: row.try_get(1)?,
                    instance_id: row.try_get(2)?,
                    hostname: row.try_get(3)?,
                    ip: row.try_get(4)?,
                })
            },
            mapper: |it| DeviceByToken::from(it),
        }
    }
}
pub struct TakeOverDeviceStmt(&'static str, Option<postgres::Statement>);
pub fn take_over_device() -> TakeOverDeviceStmt {
    TakeOverDeviceStmt(
        "UPDATE devices SET instance_id = $1, last_seen = $2 WHERE id = $3 AND (instance_id IS NULL OR instance_id = $1 OR last_seen IS NULL OR last_seen < $4)",
        None,
    )
}
impl TakeOverDeviceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        instance: &'a T1,
        now: &'a i64,
        id: &'a i64,
        lease_start: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[instance, now, id, lease_start])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, TakeOverDeviceParams<T1>, Result<u64, postgres::Error>, C>
    for TakeOverDeviceStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a TakeOverDeviceParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.instance, &params.now, &params.id, &params.lease_start)
    }
}
pub struct RecordHeartbeatStmt(&'static str, Option<postgres::Statement>);
pub fn record_heartbeat() -> RecordHeartbeatStmt {
    RecordHeartbeatStmt(
        "UPDATE devices SET hostname = $1, os = $2, ip = $3, client_version = $4, disk_total = $5, disk_free = $6, last_seen = $7 WHERE id = $8",
        None,
    )
}
impl RecordHeartbeatStmt {
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
        hostname: &'a T1,
        os: &'a T2,
        ip: &'a Option<T3>,
        client_version: &'a T4,
        disk_total: &'a i64,
        disk_free: &'a i64,
        now: &'a i64,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[hostname, os, ip, client_version, disk_total, disk_free, now, id])
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
> crate::client::sync::Params<'c, 'a, 's, RecordHeartbeatParams<T1, T2, T3, T4>, Result<u64, postgres::Error>, C>
    for RecordHeartbeatStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a RecordHeartbeatParams<T1, T2, T3, T4>,
    ) -> Result<u64, postgres::Error> {
        self.bind(
            client,
            &params.hostname,
            &params.os,
            &params.ip,
            &params.client_version,
            &params.disk_total,
            &params.disk_free,
            &params.now,
            &params.id,
        )
    }
}
pub struct ClearActivityStmt(&'static str, Option<postgres::Statement>);
pub fn clear_activity() -> ClearActivityStmt {
    ClearActivityStmt("DELETE FROM device_activity WHERE device_id = $1", None)
}
impl ClearActivityStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[device_id])
    }
}
pub struct SaveActivityStmt(&'static str, Option<postgres::Statement>);
pub fn save_activity() -> SaveActivityStmt {
    SaveActivityStmt(
        "INSERT INTO device_activity (device_id, updated_at, phase, current_file, files_done, files_total, bytes_done, bytes_total, speed_bps) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT (device_id) DO UPDATE SET updated_at = EXCLUDED.updated_at, phase = EXCLUDED.phase, current_file = EXCLUDED.current_file, files_done = EXCLUDED.files_done, files_total = EXCLUDED.files_total, bytes_done = EXCLUDED.bytes_done, bytes_total = EXCLUDED.bytes_total, speed_bps = EXCLUDED.speed_bps",
        None,
    )
}
impl SaveActivityStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        device_id: &'a i64,
        now: &'a i64,
        phase: &'a T1,
        current_file: &'a T2,
        files_done: &'a i64,
        files_total: &'a i64,
        bytes_done: &'a i64,
        bytes_total: &'a i64,
        speed_bps: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(
            self.0,
            &[device_id, now, phase, current_file, files_done, files_total, bytes_done, bytes_total, speed_bps],
        )
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, SaveActivityParams<T1, T2>, Result<u64, postgres::Error>, C>
    for SaveActivityStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SaveActivityParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(
            client,
            &params.device_id,
            &params.now,
            &params.phase,
            &params.current_file,
            &params.files_done,
            &params.files_total,
            &params.bytes_done,
            &params.bytes_total,
            &params.speed_bps,
        )
    }
}
pub struct DeviceSummariesStmt(&'static str, Option<postgres::Statement>);
pub fn device_summaries() -> DeviceSummariesStmt {
    DeviceSummariesStmt(
        "SELECT d.name, d.kind, d.created_at, d.revoked_at, d.hostname, d.os, d.ip, d.client_version, d.disk_total, d.disk_free, d.last_seen, d.last_collected, d.total_bytes, d.file_count, a.updated_at AS activity_at, a.phase, a.current_file, a.files_done, a.files_total, a.bytes_done, a.bytes_total, a.speed_bps, u.name AS user_name, d.folder FROM devices d JOIN users u ON u.id = d.user_id LEFT JOIN device_activity a ON a.device_id = d.id WHERE ($1::text IS NULL OR d.folder = $1) AND ($2::boolean OR d.kind = 'master' OR d.user_id = $3::bigint) ORDER BY u.name, d.name",
        None,
    )
}
impl DeviceSummariesStmt {
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
    ) -> DeviceSummariesQuery<'c, 'a, 's, C, DeviceSummaries, 3> {
        DeviceSummariesQuery {
            client,
            params: [folder, all, user],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<DeviceSummariesBorrowed, postgres::Error> {
                Ok(DeviceSummariesBorrowed {
                    name: row.try_get(0)?,
                    kind: row.try_get(1)?,
                    created_at: row.try_get(2)?,
                    revoked_at: row.try_get(3)?,
                    hostname: row.try_get(4)?,
                    os: row.try_get(5)?,
                    ip: row.try_get(6)?,
                    client_version: row.try_get(7)?,
                    disk_total: row.try_get(8)?,
                    disk_free: row.try_get(9)?,
                    last_seen: row.try_get(10)?,
                    last_collected: row.try_get(11)?,
                    total_bytes: row.try_get(12)?,
                    file_count: row.try_get(13)?,
                    activity_at: row.try_get(14)?,
                    phase: row.try_get(15)?,
                    current_file: row.try_get(16)?,
                    files_done: row.try_get(17)?,
                    files_total: row.try_get(18)?,
                    bytes_done: row.try_get(19)?,
                    bytes_total: row.try_get(20)?,
                    speed_bps: row.try_get(21)?,
                    user_name: row.try_get(22)?,
                    folder: row.try_get(23)?,
                })
            },
            mapper: |it| DeviceSummaries::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<
        'c,
        'a,
        's,
        DeviceSummariesParams<T1>,
        DeviceSummariesQuery<'c, 'a, 's, C, DeviceSummaries, 3>,
        C,
    > for DeviceSummariesStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a DeviceSummariesParams<T1>,
    ) -> DeviceSummariesQuery<'c, 'a, 's, C, DeviceSummaries, 3> {
        self.bind(client, &params.folder, &params.all, &params.user)
    }
}
pub struct UserForDeviceStmt(&'static str, Option<postgres::Statement>);
pub fn user_for_device() -> UserForDeviceStmt {
    UserForDeviceStmt("SELECT id, role FROM users WHERE name = $1 AND disabled_at IS NULL", None)
}
impl UserForDeviceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
    ) -> UserForDeviceQuery<'c, 'a, 's, C, UserForDevice, 1> {
        UserForDeviceQuery {
            client,
            params: [name],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<UserForDeviceBorrowed, postgres::Error> {
                Ok(UserForDeviceBorrowed { id: row.try_get(0)?, role: row.try_get(1)? })
            },
            mapper: |it| UserForDevice::from(it),
        }
    }
}
pub struct InsertDeviceStmt(&'static str, Option<postgres::Statement>);
pub fn insert_device() -> InsertDeviceStmt {
    InsertDeviceStmt(
        "INSERT INTO devices (name, kind, token_hash, created_at, user_id) VALUES ($1, $2, $3, $4, $5) ON CONFLICT (user_id, name) DO NOTHING RETURNING folder",
        None,
    )
}
impl InsertDeviceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
        kind: &'a T2,
        token_hash: &'a T3,
        now: &'a i64,
        user_id: &'a i64,
    ) -> StringQuery<'c, 'a, 's, C, String, 5> {
        StringQuery {
            client,
            params: [name, kind, token_hash, now, user_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it.into(),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, InsertDeviceParams<T1, T2, T3>, StringQuery<'c, 'a, 's, C, String, 5>, C>
    for InsertDeviceStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a InsertDeviceParams<T1, T2, T3>,
    ) -> StringQuery<'c, 'a, 's, C, String, 5> {
        self.bind(client, &params.name, &params.kind, &params.token_hash, &params.now, &params.user_id)
    }
}
pub struct SetDeviceTokenStmt(&'static str, Option<postgres::Statement>);
pub fn set_device_token() -> SetDeviceTokenStmt {
    SetDeviceTokenStmt(
        "UPDATE devices SET token_hash = $1, revoked_at = NULL, instance_id = NULL WHERE folder = $2 AND kind = 'device'",
        None,
    )
}
impl SetDeviceTokenStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        token_hash: &'a T1,
        folder: &'a T2,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[token_hash, folder])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, SetDeviceTokenParams<T1, T2>, Result<u64, postgres::Error>, C>
    for SetDeviceTokenStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetDeviceTokenParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.token_hash, &params.folder)
    }
}
pub struct ReleaseDeviceStmt(&'static str, Option<postgres::Statement>);
pub fn release_device() -> ReleaseDeviceStmt {
    ReleaseDeviceStmt("UPDATE devices SET instance_id = NULL WHERE folder = $1 AND kind = 'device'", None)
}
impl ReleaseDeviceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        folder: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[folder])
    }
}
pub struct RevokeDeviceStmt(&'static str, Option<postgres::Statement>);
pub fn revoke_device() -> RevokeDeviceStmt {
    RevokeDeviceStmt(
        "UPDATE devices SET revoked_at = COALESCE(revoked_at, $1) WHERE folder = $2 AND kind = 'device'",
        None,
    )
}
impl RevokeDeviceStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        now: &'a i64,
        folder: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[now, folder])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, RevokeDeviceParams<T1>, Result<u64, postgres::Error>, C>
    for RevokeDeviceStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a RevokeDeviceParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.folder)
    }
}
pub struct DeviceOrLocalExistsStmt(&'static str, Option<postgres::Statement>);
pub fn device_or_local_exists() -> DeviceOrLocalExistsStmt {
    DeviceOrLocalExistsStmt("SELECT 1 FROM devices WHERE folder = $1 AND kind IN ('device', 'local')", None)
}
impl DeviceOrLocalExistsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        folder: &'a T1,
    ) -> I32Query<'c, 'a, 's, C, i32, 1> {
        I32Query {
            client,
            params: [folder],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct LockDeviceFolderStmt(&'static str, Option<postgres::Statement>);
pub fn lock_device_folder() -> LockDeviceFolderStmt {
    LockDeviceFolderStmt("SELECT folder FROM devices WHERE id = $1 FOR UPDATE", None)
}
impl LockDeviceFolderStmt {
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
pub struct UpdateTotalsStmt(&'static str, Option<postgres::Statement>);
pub fn update_totals() -> UpdateTotalsStmt {
    UpdateTotalsStmt(
        "UPDATE devices d SET file_count = t.live, total_bytes = t.bytes, deleted_files = t.gone, totals_stale = FALSE, totals_at = $1 FROM (SELECT COUNT(*) FILTER (WHERE f.deleted_at IS NULL) AS live, COALESCE(SUM(v.size) FILTER (WHERE f.deleted_at IS NULL), 0)::BIGINT AS bytes, COUNT(*) FILTER (WHERE f.deleted_at IS NOT NULL) AS gone FROM files f LEFT JOIN versions v ON v.id = f.current_version_id WHERE f.device_id = $2) t WHERE d.id = $2",
        None,
    )
}
impl UpdateTotalsStmt {
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
    crate::client::sync::Params<'c, 'a, 's, UpdateTotalsParams, Result<u64, postgres::Error>, C> for UpdateTotalsStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a UpdateTotalsParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.id)
    }
}
pub struct StaleDeviceIdsStmt(&'static str, Option<postgres::Statement>);
pub fn stale_device_ids() -> StaleDeviceIdsStmt {
    StaleDeviceIdsStmt(
        "SELECT d.id FROM devices d WHERE d.totals_stale AND NOT EXISTS (SELECT 1 FROM device_activity a WHERE a.device_id = d.id AND a.updated_at > $1)",
        None,
    )
}
impl StaleDeviceIdsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        syncing_since: &'a i64,
    ) -> I64Query<'c, 'a, 's, C, i64, 1> {
        I64Query {
            client,
            params: [syncing_since],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
pub struct ClientsLastSeenStmt(&'static str, Option<postgres::Statement>);
pub fn clients_last_seen() -> ClientsLastSeenStmt {
    ClientsLastSeenStmt(
        "SELECT folder, last_seen FROM devices WHERE kind = 'device' AND revoked_at IS NULL AND last_seen IS NOT NULL",
        None,
    )
}
impl ClientsLastSeenStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> ClientsLastSeenQuery<'c, 'a, 's, C, ClientsLastSeen, 0> {
        ClientsLastSeenQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<ClientsLastSeenBorrowed, postgres::Error> {
                Ok(ClientsLastSeenBorrowed { folder: row.try_get(0)?, last_seen: row.try_get(1)? })
            },
            mapper: |it| ClientsLastSeen::from(it),
        }
    }
}
pub struct VisibleTotalsStmt(&'static str, Option<postgres::Statement>);
pub fn visible_totals() -> VisibleTotalsStmt {
    VisibleTotalsStmt(
        "SELECT COALESCE(SUM(file_count), 0)::BIGINT AS files, COALESCE(SUM(total_bytes), 0)::BIGINT AS bytes, COALESCE(SUM(deleted_files), 0)::BIGINT AS deleted FROM devices d WHERE ($1::boolean OR d.kind = 'master' OR d.user_id = $2::bigint)",
        None,
    )
}
impl VisibleTotalsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        all: &'a bool,
        user: &'a i64,
    ) -> VisibleTotalsQuery<'c, 'a, 's, C, VisibleTotals, 2> {
        VisibleTotalsQuery {
            client,
            params: [all, user],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<VisibleTotals, postgres::Error> {
                Ok(VisibleTotals { files: row.try_get(0)?, bytes: row.try_get(1)?, deleted: row.try_get(2)? })
            },
            mapper: |it| VisibleTotals::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, VisibleTotalsParams, VisibleTotalsQuery<'c, 'a, 's, C, VisibleTotals, 2>, C>
    for VisibleTotalsStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a VisibleTotalsParams,
    ) -> VisibleTotalsQuery<'c, 'a, 's, C, VisibleTotals, 2> {
        self.bind(client, &params.all, &params.user)
    }
}
pub struct MarkTotalsStaleStmt(&'static str, Option<postgres::Statement>);
pub fn mark_totals_stale() -> MarkTotalsStaleStmt {
    MarkTotalsStaleStmt("UPDATE devices SET totals_stale = TRUE WHERE id = ANY($1)", None)
}
impl MarkTotalsStaleStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        ids: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[ids])
    }
}
pub struct MasterTotalsStmt(&'static str, Option<postgres::Statement>);
pub fn master_totals() -> MasterTotalsStmt {
    MasterTotalsStmt("SELECT file_count, total_bytes FROM devices WHERE kind = 'master'", None)
}
impl MasterTotalsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
    ) -> MasterTotalsQuery<'c, 'a, 's, C, MasterTotals, 0> {
        MasterTotalsQuery {
            client,
            params: [],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<MasterTotals, postgres::Error> {
                Ok(MasterTotals { file_count: row.try_get(0)?, total_bytes: row.try_get(1)? })
            },
            mapper: |it| MasterTotals::from(it),
        }
    }
}
pub struct DeviceKindStmt(&'static str, Option<postgres::Statement>);
pub fn device_kind() -> DeviceKindStmt {
    DeviceKindStmt("SELECT kind FROM devices WHERE id = $1", None)
}
impl DeviceKindStmt {
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
pub struct DeviceByFolderStmt(&'static str, Option<postgres::Statement>);
pub fn device_by_folder() -> DeviceByFolderStmt {
    DeviceByFolderStmt("SELECT id, folder FROM devices WHERE folder = $1", None)
}
impl DeviceByFolderStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        folder: &'a T1,
    ) -> DeviceByFolderQuery<'c, 'a, 's, C, DeviceByFolder, 1> {
        DeviceByFolderQuery {
            client,
            params: [folder],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<DeviceByFolderBorrowed, postgres::Error> {
                Ok(DeviceByFolderBorrowed { id: row.try_get(0)?, folder: row.try_get(1)? })
            },
            mapper: |it| DeviceByFolder::from(it),
        }
    }
}
