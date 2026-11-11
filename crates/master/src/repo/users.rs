//! Users: who may log in and in what role. Deleting one (`web::users::delete`) takes what is theirs with them.

use fafnirx_db::client::GenericClient;
use fafnirx_db::queries::users as q;
use postgres::Error;

pub type Listed = q::ListUsers;

/// A user as the rules about administrators see them.
pub struct Account {
    pub id: i64,
    pub role: String,
    pub disabled: bool,
}

pub fn list(c: &mut impl GenericClient) -> Result<Vec<Listed>, Error> {
    q::list_users().bind(c).all()
}

pub fn names(c: &mut impl GenericClient) -> Result<Vec<String>, Error> {
    q::user_names().bind(c).all()
}

pub fn exists(c: &mut impl GenericClient, name: &str) -> Result<bool, Error> {
    Ok(q::user_exists().bind(c, &name).opt()?.is_some())
}

/// Serializes changes to the administrators until the transaction ends, so that two requests cannot each leave the
/// other as the last one.
pub fn lock(c: &mut impl GenericClient) -> Result<(), Error> {
    q::lock_users().bind(c).map(drop)
}

pub fn by_name(c: &mut impl GenericClient, name: &str) -> Result<Option<Account>, Error> {
    Ok(q::user_by_name()
        .bind(c, &name)
        .opt()?
        .map(|u| Account { id: u.id, role: u.role, disabled: u.disabled }))
}

pub fn enable(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::enable_user().bind(c, &id).map(drop)
}

/// The id and password hash of the administrator the master starts with.
pub fn admin_credentials(c: &mut impl GenericClient) -> Result<(i64, String), Error> {
    let a = q::admin_credentials().bind(c).one()?;
    Ok((a.id, a.password_hash))
}

/// The id and password hash of an enabled user, for logging in.
pub fn login_credentials(c: &mut impl GenericClient, name: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::login_credentials().bind(c, &name).opt()?.map(|u| (u.id, u.password_hash)))
}

/// The id of an enabled user who can have devices of their own (not a read-only one).
pub fn who_can_have_devices(c: &mut impl GenericClient, name: &str) -> Result<Option<i64>, Error> {
    q::user_who_can_have_devices().bind(c, &name).opt()
}

/// A computer or folder of a user.
pub type Owned = q::DevicesOfUser;

pub fn devices_of(c: &mut impl GenericClient, id: i64) -> Result<Vec<Owned>, Error> {
    q::devices_of_user().bind(c, &id).all()
}

pub fn sources_of(c: &mut impl GenericClient, id: i64) -> Result<Vec<String>, Error> {
    q::sources_of_user().bind(c, &id).all()
}

/// A mirror or another user's source that uses a folder of the user: (its name, the folder).
pub fn link_on_folders_of(c: &mut impl GenericClient, id: i64) -> Result<Option<(String, String)>, Error> {
    Ok(q::links_on_folders_of_user().bind(c, &id).opt()?.map(|l| (l.name, l.folder)))
}

/// Deletes the user and what is only theirs that has no device of its own: memberships of others' folders, favorites and
/// ratings, tags, albums and sessions. Their alerts are closed. Their devices must be gone already.
pub fn delete(c: &mut impl GenericClient, id: i64, name: &str, now: i64) -> Result<(), Error> {
    q::delete_memberships_of_user().bind(c, &id)?;
    q::delete_file_state_of_user().bind(c, &id)?;
    q::delete_tags_of_user().bind(c, &id)?;
    q::delete_albums_of_user().bind(c, &id)?;
    q::resolve_alerts_of_user().bind(c, &now, &format!("{name}/"))?;
    q::delete_user().bind(c, &id).map(drop)
}
