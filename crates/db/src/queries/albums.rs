// This file was generated with `cornucopia`. Do not modify.

#[derive(Clone, Copy, Debug)]
pub struct FindAlbumsParams {
    pub user_id: i64,
    pub id: Option<i64>,
}
#[derive(Debug)]
pub struct AlbumOfFolderParams<T1: crate::StringSql> {
    pub user_id: i64,
    pub device_id: i64,
    pub prefix: T1,
}
#[derive(Debug)]
pub struct InsertAlbumParams<T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql> {
    pub name: T1,
    pub description: T2,
    pub now: i64,
    pub user_id: i64,
    pub folder_device_id: Option<i64>,
    pub folder_prefix: Option<T3>,
}
#[derive(Debug)]
pub struct AddFilesToAlbumParams<T1: crate::ArraySql<Item = i64>> {
    pub album_id: i64,
    pub now: i64,
    pub ids: T1,
    pub all: bool,
    pub scope_user: i64,
}
#[derive(Debug)]
pub struct AddPlaceToAlbumParams<T1: crate::StringSql, T2: crate::StringSql> {
    pub album_id: i64,
    pub now: i64,
    pub device: T1,
    pub path: T2,
    pub all: bool,
    pub scope_user: i64,
}
#[derive(Debug)]
pub struct RenameAlbumParams<T1: crate::StringSql> {
    pub name: T1,
    pub id: i64,
}
#[derive(Debug)]
pub struct SetAlbumDescriptionParams<T1: crate::StringSql> {
    pub description: T1,
    pub id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct FileIsInAlbumParams {
    pub album_id: i64,
    pub file_id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct SetAlbumCoverParams {
    pub cover: Option<i64>,
    pub id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct TouchAlbumParams {
    pub now: i64,
    pub id: i64,
}
#[derive(Clone, Copy, Debug)]
pub struct DeleteAlbumParams {
    pub id: i64,
    pub user_id: i64,
}
#[derive(Debug)]
pub struct RemoveFilesFromAlbumParams<T1: crate::ArraySql<Item = i64>> {
    pub album_id: i64,
    pub ids: T1,
}
#[derive(Debug)]
pub struct ClearCoverAmongParams<T1: crate::ArraySql<Item = i64>> {
    pub album_id: i64,
    pub ids: T1,
}
#[derive(Clone, Copy, Debug)]
pub struct AlbumsOfFileParams {
    pub file_id: i64,
    pub uid: i64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FindAlbums {
    pub id: i64,
    pub name: String,
    pub description: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub files: i64,
    pub bytes: i64,
    pub cover: Option<i64>,
    pub folder_device: Option<String>,
    pub folder_prefix: Option<String>,
}
pub struct FindAlbumsBorrowed<'a> {
    pub id: i64,
    pub name: &'a str,
    pub description: &'a str,
    pub created_at: i64,
    pub updated_at: i64,
    pub files: i64,
    pub bytes: i64,
    pub cover: Option<i64>,
    pub folder_device: Option<&'a str>,
    pub folder_prefix: Option<&'a str>,
}
impl<'a> From<FindAlbumsBorrowed<'a>> for FindAlbums {
    fn from(
        FindAlbumsBorrowed {
            id,
            name,
            description,
            created_at,
            updated_at,
            files,
            bytes,
            cover,
            folder_device,
            folder_prefix,
        }: FindAlbumsBorrowed<'a>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            description: description.into(),
            created_at,
            updated_at,
            files,
            bytes,
            cover,
            folder_device: folder_device.map(|v| v.into()),
            folder_prefix: folder_prefix.map(|v| v.into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct AlbumsOfFile {
    pub id: i64,
    pub name: String,
}
pub struct AlbumsOfFileBorrowed<'a> {
    pub id: i64,
    pub name: &'a str,
}
impl<'a> From<AlbumsOfFileBorrowed<'a>> for AlbumsOfFile {
    fn from(AlbumsOfFileBorrowed { id, name }: AlbumsOfFileBorrowed<'a>) -> Self {
        Self { id, name: name.into() }
    }
}
use crate::client::sync::GenericClient;
use postgres::fallible_iterator::FallibleIterator;
pub struct FindAlbumsQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<FindAlbumsBorrowed, postgres::Error>,
    mapper: fn(FindAlbumsBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> FindAlbumsQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(FindAlbumsBorrowed) -> R) -> FindAlbumsQuery<'c, 'a, 's, C, R, N> {
        FindAlbumsQuery {
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
pub struct AlbumsOfFileQuery<'c, 'a, 's, C: GenericClient, T, const N: usize> {
    client: &'c mut C,
    params: [&'a (dyn postgres_types::ToSql + Sync); N],
    query: &'static str,
    cached: Option<&'s postgres::Statement>,
    extractor: fn(&postgres::Row) -> Result<AlbumsOfFileBorrowed, postgres::Error>,
    mapper: fn(AlbumsOfFileBorrowed) -> T,
}
impl<'c, 'a, 's, C, T: 'c, const N: usize> AlbumsOfFileQuery<'c, 'a, 's, C, T, N>
where
    C: GenericClient,
{
    pub fn map<R>(self, mapper: fn(AlbumsOfFileBorrowed) -> R) -> AlbumsOfFileQuery<'c, 'a, 's, C, R, N> {
        AlbumsOfFileQuery {
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
pub struct FindAlbumsStmt(&'static str, Option<postgres::Statement>);
pub fn find_albums() -> FindAlbumsStmt {
    FindAlbumsStmt(
        "SELECT a.id, a.name, a.description, a.created_at, a.updated_at, (SELECT COUNT(*) FROM files f WHERE (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id) OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix))) AND f.deleted_at IS NULL AND f.media IS NOT NULL) AS files, (SELECT COALESCE(SUM(v.size), 0)::BIGINT FROM files f JOIN versions v ON v.id = f.current_version_id WHERE (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id) OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix))) AND f.deleted_at IS NULL AND f.media IS NOT NULL) AS bytes, COALESCE((SELECT f.id FROM files f WHERE f.id = a.cover_file_id AND f.deleted_at IS NULL), (SELECT f.id FROM files f JOIN versions v ON v.id = f.current_version_id LEFT JOIN file_meta m ON m.root_hash = v.root_hash LEFT JOIN file_state s ON s.file_id = f.id AND s.user_id = a.user_id WHERE (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id) OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix))) AND f.deleted_at IS NULL AND f.media IS NOT NULL ORDER BY COALESCE(s.taken_at, m.taken_at, v.mtime / 1000000000) DESC, f.id DESC LIMIT 1)) AS cover, (SELECT d.folder FROM devices d WHERE d.id = a.folder_device_id) AS folder_device, a.folder_prefix FROM albums a WHERE a.user_id = $1 AND ($2::bigint IS NULL OR a.id = $2) ORDER BY a.updated_at DESC, a.id DESC",
        None,
    )
}
impl FindAlbumsStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        user_id: &'a i64,
        id: &'a Option<i64>,
    ) -> FindAlbumsQuery<'c, 'a, 's, C, FindAlbums, 2> {
        FindAlbumsQuery {
            client,
            params: [user_id, id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<FindAlbumsBorrowed, postgres::Error> {
                Ok(FindAlbumsBorrowed {
                    id: row.try_get(0)?,
                    name: row.try_get(1)?,
                    description: row.try_get(2)?,
                    created_at: row.try_get(3)?,
                    updated_at: row.try_get(4)?,
                    files: row.try_get(5)?,
                    bytes: row.try_get(6)?,
                    cover: row.try_get(7)?,
                    folder_device: row.try_get(8)?,
                    folder_prefix: row.try_get(9)?,
                })
            },
            mapper: |it| FindAlbums::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, FindAlbumsParams, FindAlbumsQuery<'c, 'a, 's, C, FindAlbums, 2>, C>
    for FindAlbumsStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a FindAlbumsParams,
    ) -> FindAlbumsQuery<'c, 'a, 's, C, FindAlbums, 2> {
        self.bind(client, &params.user_id, &params.id)
    }
}
pub struct AlbumOfFolderStmt(&'static str, Option<postgres::Statement>);
pub fn album_of_folder() -> AlbumOfFolderStmt {
    AlbumOfFolderStmt("SELECT id FROM albums WHERE user_id = $1 AND folder_device_id = $2 AND folder_prefix = $3", None)
}
impl AlbumOfFolderStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        user_id: &'a i64,
        device_id: &'a i64,
        prefix: &'a T1,
    ) -> I64Query<'c, 'a, 's, C, i64, 3> {
        I64Query {
            client,
            params: [user_id, device_id, prefix],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, AlbumOfFolderParams<T1>, I64Query<'c, 'a, 's, C, i64, 3>, C>
    for AlbumOfFolderStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a AlbumOfFolderParams<T1>) -> I64Query<'c, 'a, 's, C, i64, 3> {
        self.bind(client, &params.user_id, &params.device_id, &params.prefix)
    }
}
pub struct InsertAlbumStmt(&'static str, Option<postgres::Statement>);
pub fn insert_album() -> InsertAlbumStmt {
    InsertAlbumStmt(
        "INSERT INTO albums (name, description, created_at, updated_at, user_id, folder_device_id, folder_prefix) VALUES ($1, $2, $3, $3, $4, $5, $6) RETURNING id",
        None,
    )
}
impl InsertAlbumStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
        description: &'a T2,
        now: &'a i64,
        user_id: &'a i64,
        folder_device_id: &'a Option<i64>,
        folder_prefix: &'a Option<T3>,
    ) -> I64Query<'c, 'a, 's, C, i64, 6> {
        I64Query {
            client,
            params: [name, description, now, user_id, folder_device_id, folder_prefix],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql, T3: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, InsertAlbumParams<T1, T2, T3>, I64Query<'c, 'a, 's, C, i64, 6>, C>
    for InsertAlbumStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a InsertAlbumParams<T1, T2, T3>,
    ) -> I64Query<'c, 'a, 's, C, i64, 6> {
        self.bind(
            client,
            &params.name,
            &params.description,
            &params.now,
            &params.user_id,
            &params.folder_device_id,
            &params.folder_prefix,
        )
    }
}
pub struct AddFilesToAlbumStmt(&'static str, Option<postgres::Statement>);
pub fn add_files_to_album() -> AddFilesToAlbumStmt {
    AddFilesToAlbumStmt(
        "INSERT INTO album_files (album_id, file_id, added_at) SELECT $1, f.id, $2 FROM files f JOIN devices d ON d.id = f.device_id WHERE f.id = ANY($3) AND f.deleted_at IS NULL AND ($4::boolean OR d.kind = 'master' OR d.user_id = $5::bigint) ON CONFLICT DO NOTHING",
        None,
    )
}
impl AddFilesToAlbumStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        album_id: &'a i64,
        now: &'a i64,
        ids: &'a T1,
        all: &'a bool,
        scope_user: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[album_id, now, ids, all, scope_user])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, AddFilesToAlbumParams<T1>, Result<u64, postgres::Error>, C>
    for AddFilesToAlbumStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a AddFilesToAlbumParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.album_id, &params.now, &params.ids, &params.all, &params.scope_user)
    }
}
pub struct AddPlaceToAlbumStmt(&'static str, Option<postgres::Statement>);
pub fn add_place_to_album() -> AddPlaceToAlbumStmt {
    AddPlaceToAlbumStmt(
        "INSERT INTO album_files (album_id, file_id, added_at) SELECT $1, f.id, $2 FROM files f JOIN devices d ON d.id = f.device_id WHERE d.folder = $3 AND f.deleted_at IS NULL AND f.media IS NOT NULL AND ($4 = '' OR f.path = $4 OR starts_with(f.path, $4 || '/')) AND ($5::boolean OR d.kind = 'master' OR d.user_id = $6::bigint) ON CONFLICT DO NOTHING",
        None,
    )
}
impl AddPlaceToAlbumStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        album_id: &'a i64,
        now: &'a i64,
        device: &'a T1,
        path: &'a T2,
        all: &'a bool,
        scope_user: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[album_id, now, device, path, all, scope_user])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql, T2: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, AddPlaceToAlbumParams<T1, T2>, Result<u64, postgres::Error>, C>
    for AddPlaceToAlbumStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a AddPlaceToAlbumParams<T1, T2>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.album_id, &params.now, &params.device, &params.path, &params.all, &params.scope_user)
    }
}
pub struct RenameAlbumStmt(&'static str, Option<postgres::Statement>);
pub fn rename_album() -> RenameAlbumStmt {
    RenameAlbumStmt("UPDATE albums SET name = $1 WHERE id = $2", None)
}
impl RenameAlbumStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        name: &'a T1,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[name, id])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, RenameAlbumParams<T1>, Result<u64, postgres::Error>, C>
    for RenameAlbumStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a RenameAlbumParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.name, &params.id)
    }
}
pub struct SetAlbumDescriptionStmt(&'static str, Option<postgres::Statement>);
pub fn set_album_description() -> SetAlbumDescriptionStmt {
    SetAlbumDescriptionStmt("UPDATE albums SET description = $1 WHERE id = $2", None)
}
impl SetAlbumDescriptionStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>(
        &'s self,
        client: &'c mut C,
        description: &'a T1,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[description, id])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::StringSql>
    crate::client::sync::Params<'c, 'a, 's, SetAlbumDescriptionParams<T1>, Result<u64, postgres::Error>, C>
    for SetAlbumDescriptionStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetAlbumDescriptionParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.description, &params.id)
    }
}
pub struct FileIsInAlbumStmt(&'static str, Option<postgres::Statement>);
pub fn file_is_in_album() -> FileIsInAlbumStmt {
    FileIsInAlbumStmt(
        "SELECT EXISTS (SELECT 1 FROM albums a, files f WHERE a.id = $1 AND f.id = $2 AND (EXISTS (SELECT 1 FROM album_files af WHERE af.album_id = a.id AND af.file_id = f.id) OR (a.folder_device_id = f.device_id AND starts_with(f.path, a.folder_prefix)))) AS member",
        None,
    )
}
impl FileIsInAlbumStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        album_id: &'a i64,
        file_id: &'a i64,
    ) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        BoolQuery {
            client,
            params: [album_id, file_id],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row| Ok(row.try_get(0)?),
            mapper: |it| it,
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, FileIsInAlbumParams, BoolQuery<'c, 'a, 's, C, bool, 2>, C>
    for FileIsInAlbumStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a FileIsInAlbumParams) -> BoolQuery<'c, 'a, 's, C, bool, 2> {
        self.bind(client, &params.album_id, &params.file_id)
    }
}
pub struct SetAlbumCoverStmt(&'static str, Option<postgres::Statement>);
pub fn set_album_cover() -> SetAlbumCoverStmt {
    SetAlbumCoverStmt("UPDATE albums SET cover_file_id = $1 WHERE id = $2", None)
}
impl SetAlbumCoverStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        cover: &'a Option<i64>,
        id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[cover, id])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, SetAlbumCoverParams, Result<u64, postgres::Error>, C>
    for SetAlbumCoverStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a SetAlbumCoverParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.cover, &params.id)
    }
}
pub struct TouchAlbumStmt(&'static str, Option<postgres::Statement>);
pub fn touch_album() -> TouchAlbumStmt {
    TouchAlbumStmt("UPDATE albums SET updated_at = $1 WHERE id = $2", None)
}
impl TouchAlbumStmt {
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
    crate::client::sync::Params<'c, 'a, 's, TouchAlbumParams, Result<u64, postgres::Error>, C> for TouchAlbumStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a TouchAlbumParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.now, &params.id)
    }
}
pub struct DeleteAlbumStmt(&'static str, Option<postgres::Statement>);
pub fn delete_album() -> DeleteAlbumStmt {
    DeleteAlbumStmt("DELETE FROM albums WHERE id = $1 AND user_id = $2", None)
}
impl DeleteAlbumStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        id: &'a i64,
        user_id: &'a i64,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[id, user_id])
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, DeleteAlbumParams, Result<u64, postgres::Error>, C> for DeleteAlbumStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a DeleteAlbumParams) -> Result<u64, postgres::Error> {
        self.bind(client, &params.id, &params.user_id)
    }
}
pub struct RemoveFilesFromAlbumStmt(&'static str, Option<postgres::Statement>);
pub fn remove_files_from_album() -> RemoveFilesFromAlbumStmt {
    RemoveFilesFromAlbumStmt("DELETE FROM album_files WHERE album_id = $1 AND file_id = ANY($2)", None)
}
impl RemoveFilesFromAlbumStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        album_id: &'a i64,
        ids: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[album_id, ids])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, RemoveFilesFromAlbumParams<T1>, Result<u64, postgres::Error>, C>
    for RemoveFilesFromAlbumStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a RemoveFilesFromAlbumParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.album_id, &params.ids)
    }
}
pub struct ClearCoverAmongStmt(&'static str, Option<postgres::Statement>);
pub fn clear_cover_among() -> ClearCoverAmongStmt {
    ClearCoverAmongStmt("UPDATE albums SET cover_file_id = NULL WHERE id = $1 AND cover_file_id = ANY($2)", None)
}
impl ClearCoverAmongStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>(
        &'s self,
        client: &'c mut C,
        album_id: &'a i64,
        ids: &'a T1,
    ) -> Result<u64, postgres::Error> {
        client.execute(self.0, &[album_id, ids])
    }
}
impl<'c, 'a, 's, C: GenericClient, T1: crate::ArraySql<Item = i64>>
    crate::client::sync::Params<'c, 'a, 's, ClearCoverAmongParams<T1>, Result<u64, postgres::Error>, C>
    for ClearCoverAmongStmt
{
    fn params(&'s self, client: &'c mut C, params: &'a ClearCoverAmongParams<T1>) -> Result<u64, postgres::Error> {
        self.bind(client, &params.album_id, &params.ids)
    }
}
pub struct AlbumsOfFileStmt(&'static str, Option<postgres::Statement>);
pub fn albums_of_file() -> AlbumsOfFileStmt {
    AlbumsOfFileStmt(
        "SELECT a.id, a.name FROM album_files af JOIN albums a ON a.id = af.album_id WHERE af.file_id = $1 AND a.user_id = $2 ORDER BY lower(a.name)",
        None,
    )
}
impl AlbumsOfFileStmt {
    pub fn prepare<'a, C: GenericClient>(mut self, client: &'a mut C) -> Result<Self, postgres::Error> {
        self.1 = Some(client.prepare(self.0)?);
        Ok(self)
    }
    pub fn bind<'c, 'a, 's, C: GenericClient>(
        &'s self,
        client: &'c mut C,
        file_id: &'a i64,
        uid: &'a i64,
    ) -> AlbumsOfFileQuery<'c, 'a, 's, C, AlbumsOfFile, 2> {
        AlbumsOfFileQuery {
            client,
            params: [file_id, uid],
            query: self.0,
            cached: self.1.as_ref(),
            extractor: |row: &postgres::Row| -> Result<AlbumsOfFileBorrowed, postgres::Error> {
                Ok(AlbumsOfFileBorrowed { id: row.try_get(0)?, name: row.try_get(1)? })
            },
            mapper: |it| AlbumsOfFile::from(it),
        }
    }
}
impl<'c, 'a, 's, C: GenericClient>
    crate::client::sync::Params<'c, 'a, 's, AlbumsOfFileParams, AlbumsOfFileQuery<'c, 'a, 's, C, AlbumsOfFile, 2>, C>
    for AlbumsOfFileStmt
{
    fn params(
        &'s self,
        client: &'c mut C,
        params: &'a AlbumsOfFileParams,
    ) -> AlbumsOfFileQuery<'c, 'a, 's, C, AlbumsOfFile, 2> {
        self.bind(client, &params.file_id, &params.uid)
    }
}
