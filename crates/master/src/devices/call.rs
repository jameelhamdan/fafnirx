//! How a call of the device-facing API fails, and what the client is told. A handler's closure returns `Result<T, CallError>`:
//! a `Status` it makes itself, a refusal the web app would also give (`ApiError`), or anything else, which is logged and
//! reported as `internal` without its text.

use crate::web::ApiError;
use axum::http::StatusCode;
use tonic::Status;

#[derive(Debug)]
pub enum CallError {
    /// Told to the client as it is.
    Status(Status),
    /// A refusal the web app gives too (a shared folder cannot hold that path): the client is told which.
    Refused(ApiError),
    /// Logged here, `internal` to the client.
    Internal(anyhow::Error),
}

impl From<Status> for CallError {
    fn from(s: Status) -> Self {
        CallError::Status(s)
    }
}

impl From<ApiError> for CallError {
    fn from(e: ApiError) -> Self {
        CallError::Refused(e)
    }
}

impl From<postgres::Error> for CallError {
    fn from(e: postgres::Error) -> Self {
        CallError::Internal(e.into())
    }
}

impl From<std::io::Error> for CallError {
    fn from(e: std::io::Error) -> Self {
        CallError::Internal(e.into())
    }
}

/// The code shared with the web app (`files::ingest`, `files::livetree`) fails with `anyhow`, and puts an `ApiError` inside when the
/// refusal is the caller's to hear about: it is taken out here, once, as the web app does (`ApiError::from`).
impl From<anyhow::Error> for CallError {
    fn from(e: anyhow::Error) -> Self {
        match e.downcast::<ApiError>() {
            Ok(refused) => CallError::Refused(refused),
            Err(e) => match e.downcast::<Status>() {
                Ok(status) => CallError::Status(status),
                Err(e) => CallError::Internal(e),
            },
        }
    }
}

impl From<CallError> for Status {
    fn from(e: CallError) -> Self {
        match e {
            CallError::Status(s) => s,
            CallError::Refused(ApiError(code, message)) => match code {
                StatusCode::CONFLICT => Status::already_exists(message),
                StatusCode::BAD_REQUEST => Status::invalid_argument(message),
                _ => internal(message),
            },
            CallError::Internal(e) => internal(format!("{e:#}")),
        }
    }
}

pub fn internal<E: std::fmt::Display>(e: E) -> Status {
    tracing::error!("internal error: {e}");
    Status::internal("internal error")
}

#[cfg(test)]
#[path = "../../tests/unit/devices/call.rs"]
mod tests;
