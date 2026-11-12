//! The settings an administrator can change: how many versions are kept, for how long deleted files stay, and two switches.

use crate::repo::settings;
use crate::web::access::Ctx;
use crate::web::{run, ApiError, Res};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Settings {
    version_count: i64,
    retention_days: i64,
    /// Whether pictures (thumbnails and previews) are made, when somebody looks at a photo or video. Off: none is made.
    thumbnails: bool,
    /// Whether the web app offers the photo pages. Off: their menu items are hidden and the pages closed; nothing else changes.
    photos: bool,
}

/// What a save says. A switch that is left out keeps its value, so a caller that does not know it cannot turn it back on.
#[derive(Deserialize)]
pub struct SettingsChange {
    version_count: i64,
    retention_days: i64,
    thumbnails: Option<bool>,
    photos: Option<bool>,
}

pub async fn get(Ctx { state, who }: Ctx) -> Res<Settings> {
    who.require_admin()?;
    let s = run(&state, |c| {
        Ok(Settings {
            version_count: settings::version_count(c),
            retention_days: settings::retention_days(c),
            thumbnails: settings::get_i64(c, crate::library::meta::THUMBNAILS, 1) != 0,
            photos: settings::photos_on(c),
        })
    })
    .await?;
    Ok(Json(s))
}

pub async fn save(Ctx { state, who }: Ctx, Json(s): Json<SettingsChange>) -> Res<Settings> {
    who.require_admin()?;
    if !(1..=1000).contains(&s.version_count) || !(1..=3650).contains(&s.retention_days) {
        return Err(ApiError::bad("Versions kept: 1-1000. Retention: 1-3650 days."));
    }
    let saved = run(&state, move |c| {
        let thumbnails = s.thumbnails.unwrap_or_else(|| settings::get_i64(c, crate::library::meta::THUMBNAILS, 1) != 0);
        let photos = s.photos.unwrap_or_else(|| settings::photos_on(c));
        for (key, value) in [
            ("version_count", s.version_count),
            ("retention_days", s.retention_days),
            (crate::library::meta::THUMBNAILS, i64::from(thumbnails)),
            (settings::PHOTOS, i64::from(photos)),
        ] {
            settings::set(c, key, &value.to_string())?;
        }
        crate::library::meta::set_thumbnails(thumbnails);
        Ok(Settings { version_count: s.version_count, retention_days: s.retention_days, thumbnails, photos })
    })
    .await?;
    Ok(Json(saved))
}
