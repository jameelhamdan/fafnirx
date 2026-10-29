//! Users: who may log in and in what role. A user is never deleted, only disabled, because the name is their folder.

use crate::platform::pg::Pg;
use fafnirx_db::queries::users as q;
use postgres::{Error, GenericClient};

pub type Listed = q::ListUsers;

/// A user as the rules about administrators see them.
pub struct Account {
    pub id: i64,
    pub role: String,
    pub disabled: bool,
}

pub fn list(c: &mut impl GenericClient) -> Result<Vec<Listed>, Error> {
    q::list_users().bind(&mut Pg(c)).all()
}

pub fn names(c: &mut impl GenericClient) -> Result<Vec<String>, Error> {
    q::user_names().bind(&mut Pg(c)).all()
}

/// The users who can log in, with their roles.
pub fn enabled(c: &mut impl GenericClient) -> Result<Vec<q::EnabledUsers>, Error> {
    q::enabled_users().bind(&mut Pg(c)).all()
}

pub fn exists(c: &mut impl GenericClient, name: &str) -> Result<bool, Error> {
    Ok(q::user_exists().bind(&mut Pg(c), &name).opt()?.is_some())
}

pub fn insert(c: &mut impl GenericClient, name: &str, role: &str, password_hash: &str, now: i64) -> Result<(), Error> {
    q::insert_user().bind(&mut Pg(c), &name, &role, &password_hash, &now)?;
    Ok(())
}

/// Serializes changes to the administrators until the transaction ends, so that two requests cannot each leave the
/// other as the last one.
pub fn lock(c: &mut impl GenericClient) -> Result<(), Error> {
    q::lock_users().bind(&mut Pg(c))?;
    Ok(())
}

pub fn by_name(c: &mut impl GenericClient, name: &str) -> Result<Option<Account>, Error> {
    Ok(q::user_by_name().bind(&mut Pg(c), &name).opt()?.map(|u| Account {
        id: u.id,
        role: u.role,
        disabled: u.disabled,
    }))
}

/// How many administrators other than `id` can log in.
pub fn other_active_admins(c: &mut impl GenericClient, id: i64) -> Result<i64, Error> {
    q::other_active_admins().bind(&mut Pg(c), &id).one()
}

pub fn set_role(c: &mut impl GenericClient, id: i64, role: &str) -> Result<(), Error> {
    q::set_user_role().bind(&mut Pg(c), &role, &id)?;
    Ok(())
}

pub fn disable(c: &mut impl GenericClient, id: i64, now: i64) -> Result<(), Error> {
    q::disable_user().bind(&mut Pg(c), &now, &id)?;
    Ok(())
}

pub fn enable(c: &mut impl GenericClient, id: i64) -> Result<(), Error> {
    q::enable_user().bind(&mut Pg(c), &id)?;
    Ok(())
}

pub fn set_password_hash(c: &mut impl GenericClient, id: i64, hash: &str) -> Result<(), Error> {
    q::set_password_hash().bind(&mut Pg(c), &hash, &id)?;
    Ok(())
}

/// The id and password hash of the administrator the master starts with.
pub fn admin_credentials(c: &mut impl GenericClient) -> Result<(i64, String), Error> {
    let a = q::admin_credentials().bind(&mut Pg(c)).one()?;
    Ok((a.id, a.password_hash))
}

/// The id and password hash of an enabled user, for logging in.
pub fn login_credentials(c: &mut impl GenericClient, name: &str) -> Result<Option<(i64, String)>, Error> {
    Ok(q::login_credentials().bind(&mut Pg(c), &name).opt()?.map(|u| (u.id, u.password_hash)))
}

pub fn password_hash_of(c: &mut impl GenericClient, id: i64) -> Result<String, Error> {
    q::password_hash_of().bind(&mut Pg(c), &id).one()
}

/// The id of an enabled user who can have devices of their own (not a read-only one).
pub fn who_can_have_devices(c: &mut impl GenericClient, name: &str) -> Result<Option<i64>, Error> {
    q::user_who_can_have_devices().bind(&mut Pg(c), &name).opt()
}
