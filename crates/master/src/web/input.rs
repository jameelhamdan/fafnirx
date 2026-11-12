//! The rules for the names and paths that come in a request, in one place: a handler asks for the value it needs and gets it
//! cleaned or a 400 that says what is wrong.

use crate::web::ApiError;
use fafnirx_common::{clean_remote_path, valid_name};

pub const NAME_RULE: &str = "Name: 1-63 characters, lowercase letters, digits, '.', '_' or '-'; not a reserved Windows name";

/// The name of a device, folder, source or user from a request: trimmed and in lower case, and one that can be a name.
pub fn name(raw: &str) -> Result<String, ApiError> {
    let name = raw.trim().to_lowercase();
    if valid_name(&name) {
        Ok(name)
    } else {
        Err(ApiError::bad(NAME_RULE))
    }
}

/// A path below a folder from a request, cleaned and without slashes at the ends; empty for the whole folder.
pub fn folder_path(raw: &str) -> Result<String, ApiError> {
    let path = raw.trim().trim_matches('/');
    if path.is_empty() {
        return Ok(String::new());
    }
    clean_remote_path(path).map_err(|e| ApiError::bad(e.to_string()))
}

/// [`folder_path`] as the start of the paths below it: `a/b/` for `a/b`, empty for the whole folder.
pub fn folder_prefix(raw: &str) -> Result<String, ApiError> {
    let path = folder_path(raw)?;
    Ok(if path.is_empty() { path } else { path + "/" })
}

/// A path that has to name something: a file or a folder, not the whole folder.
pub fn path_inside(raw: &str) -> Result<String, ApiError> {
    match folder_path(raw)? {
        path if path.is_empty() => Err(ApiError::bad("Give a path")),
        path => Ok(path),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/web/input.rs"]
mod tests;
