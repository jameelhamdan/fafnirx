//! Mirrors: everything or one folder copied to a storage, for the administrators. Added, run now, switched, restored and removed.

use super::backend_options;
use super::hours_to_secs;
use super::mirror_id;
use super::rclone_config;
use super::touch;
use super::touch_link;
use super::Touch;
use crate::integrations::spec::MirrorSpec;
use crate::platform::db;
use crate::repo::alerts;
use crate::repo::devices;
use crate::repo::links;
use crate::repo::links::Dir;
use crate::repo::links::MirrorKind;
use crate::repo::mirrors;
use crate::web::access::Access;
use crate::web::act;
use crate::web::input;
use crate::web::run;
use crate::web::ApiError;
use crate::web::Done;
use crate::web::Res;
use crate::web::Shared;
use axum::extract::Path;
use axum::extract::State;
use axum::Extension;
use axum::Json;
use fafnirx_common::unix_now;
use serde::Deserialize;
use serde_json::json;
use serde_json::Value;

pub async fn run_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    touch_link(&state, who, None, name, Touch::RunNow).await
}

pub async fn toggle_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    touch_link(&state, who, None, name, Touch::ToggleEnabled).await
}

#[derive(Deserialize)]
pub struct NewMirror {
    name: String,
    /// `replica` (the files as they are) or `backup` (encrypted with the passwords below).
    #[serde(default)]
    kind: MirrorKind,
    #[serde(default)]
    crypt_password: String,
    #[serde(default)]
    crypt_salt: String,
    /// A remote of the rclone config file (`name:path`); when set, the backend, options and path are not used.
    #[serde(default)]
    remote: String,
    #[serde(default)]
    remote_type: String,
    #[serde(default)]
    options: String,
    /// Passwords that rclone keeps in a scrambled form (`rclone obscure`), given plain: key -> password.
    #[serde(default)]
    secrets: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    remote_path: String,
    /// The folder to hold, like `ana/laptop` or `master:Photos`; everything when empty.
    #[serde(default)]
    folder: String,
    #[serde(default = "three_days")]
    interval_hours: f64,
    #[serde(default = "three")]
    keep_runs: i64,
}

fn three_days() -> f64 {
    72.0
}
fn three() -> i64 {
    3
}

pub async fn create_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Json(b): Json<NewMirror>) -> Res<Done> {
    who.require_admin()?;
    let options = backend_options(&b.options, &b.secrets).await?;
    let mut mirror = MirrorSpec {
        name: input::device_name(&b.name)?,
        kind: b.kind,
        crypt_password: b.crypt_password,
        crypt_salt: b.crypt_salt,
        remote: b.remote,
        remote_type: b.remote_type,
        remote_path: b.remote_path,
        options: options.into_iter().collect(),
        folder: b.folder,
        interval_hours: b.interval_hours,
        keep_runs: b.keep_runs,
    };
    mirror.normalize().map_err(|e| ApiError::bad(format!("{e:#}")))?;
    let secs = hours_to_secs(mirror.interval_hours)?;
    let mut config = rclone_config(mirror.remote, mirror.remote_type, mirror.remote_path, mirror.options);
    config["crypt_password"] = mirror.crypt_password.into();
    config["crypt_salt"] = mirror.crypt_salt.into();
    config["keep_runs"] = mirror.keep_runs.into();
    act(&state, move |c| {
        let folder_id = match mirror.folder.as_str() {
            "" => None,
            folder => Some(
                devices::by_folder(c, folder)?
                    .ok_or_else(|| ApiError::bad(format!("There is no folder called {folder} to mirror")))?
                    .0,
            ),
        };
        let new = links::New {
            dir: Dir::Out,
            user_id: None,
            name: &mirror.name,
            kind: mirror.kind.as_str(),
            config: &config.to_string(),
            folder_id,
            interval_secs: secs,
        };
        if links::add(c, &new, unix_now())?.is_none() {
            return Err(ApiError::conflict(format!("A mirror named {} already exists", mirror.name)).into());
        }
        db::notify(c, db::Topic::Jobs, "mirrors")?;
        Ok(())
    })
    .await
}

/// The folders a mirror can be told to hold, for the form that adds one.
pub async fn mirror_folders(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<String>> {
    who.require_admin()?;
    let mut folders: Vec<String> = run(&state, |c| Ok(mirrors::device_folders(c)?))
        .await?
        .into_iter()
        .map(|(_, folder)| folder)
        .collect();
    folders.sort();
    Ok(Json(folders))
}

/// Forget what was uploaded: the next run re-sends everything (use after emptying the remote).
pub async fn resync_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    who.require_admin()?;
    act(&state, move |c| {
        let id = mirror_id(c, &name)?;
        links::forget_items(c, id)?;
        touch(c, id, "mirrors", Touch::RunNow)
    })
    .await
}

/// The rclone config that restores a backup mirror without fafnirx (administrators). The one place a credential
/// leaves the master: a backup is worthless without its key.
pub async fn mirror_restore_kit(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Value> {
    who.require_admin()?;
    let key = run(&state, move |c| Ok(mirrors::key(c, &name)?.ok_or_else(|| ApiError::not_found("No such mirror"))?)).await?;
    if key.kind != MirrorKind::Backup {
        return Err(ApiError::bad("Only a backup is encrypted: a replica is restored by copying its files"));
    }
    let bin = crate::integrations::mirror::rclone_bin();
    let obscured = |p: &str| {
        let p = p.to_string();
        let bin = bin.clone();
        async move { crate::integrations::mirror::obscure(&bin, &p).await.map_err(|e| ApiError::bad(format!("{e:#}"))) }
    };
    let (password, salt) = (obscured(&key.config.crypt_password).await?, obscured(&key.config.crypt_salt).await?);
    let config = crate::integrations::mirror::restore_config(&key, &password, &salt);
    Ok(Json(json!({ "config": config })))
}

pub async fn delete_mirror(State(state): State<Shared>, Extension(who): Extension<Access>, Path(name): Path<String>) -> Res<Done> {
    who.require_admin()?;
    act(&state, move |c| {
        let id = mirror_id(c, &name)?;
        links::delete(c, id)?;
        alerts::resolve(c, "mirror_failed", &name)?;
        db::notify(c, db::Topic::Jobs, "mirrors")?;
        Ok(())
    })
    .await
}
