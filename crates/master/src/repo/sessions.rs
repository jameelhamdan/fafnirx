//! Login sessions. The database keeps only the hash of the cookie's token, so a copy of it cannot be used to log in.

use crate::platform::pg::Pg;
use fafnirx_db::queries::sessions as q;
use postgres::{Error, GenericClient};

/// The user a session belongs to: (id, name, role), when it has not expired and the user is not disabled.
pub fn user_of(c: &mut impl GenericClient, token_hash: &str, now: i64) -> Result<Option<(i64, String, String)>, Error> {
    Ok(q::session_user().bind(&mut Pg(c), &token_hash, &now).opt()?.map(|u| (u.id, u.name, u.role)))
}

/// Forgets the sessions that have expired, then starts a new one.
pub fn start(c: &mut impl GenericClient, token_hash: &str, user_id: i64, now: i64, lifetime: i64) -> Result<(), Error> {
    q::delete_expired_sessions().bind(&mut Pg(&mut *c), &now)?;
    q::insert_session().bind(&mut Pg(c), &token_hash, &user_id, &now, &(now + lifetime))?;
    Ok(())
}

pub fn end(c: &mut impl GenericClient, token_hash: &str) -> Result<(), Error> {
    q::delete_session().bind(&mut Pg(c), &token_hash)?;
    Ok(())
}

/// Ends every session of a user.
pub fn end_all(c: &mut impl GenericClient, user_id: i64) -> Result<(), Error> {
    q::delete_user_sessions().bind(&mut Pg(c), &user_id)?;
    Ok(())
}

/// Ends every session of a user except the one with this token hash (the one that asked for the change).
pub fn end_others(c: &mut impl GenericClient, user_id: i64, keep: &str) -> Result<(), Error> {
    q::delete_user_sessions_except().bind(&mut Pg(c), &user_id, &keep)?;
    Ok(())
}
