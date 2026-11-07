//! What people say about a file: favorite, archive, rating, description, corrected date and place, tags; one file or many.

use super::nullable;
use super::MAX_BULK;
use super::MAX_DESCRIPTION;
use super::MAX_TAGS_PER_FILE;
use crate::library::meta;
use crate::platform::db;
use crate::platform::id::uuids;
use crate::platform::id::Id;
use crate::repo::file_state;
use crate::repo::tags;
use crate::web::access::Access;
use crate::web::act;
use crate::web::change;
use crate::web::done;
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
use fafnirx_db::client::GenericClient;
use fafnirx_db::queries as sql;
use serde::Deserialize;
use serde::Serialize;

#[derive(Deserialize)]
pub struct Position {
    lat: f64,
    lon: f64,
}

/// A change to what is said about one file. Only the fields that are present change.
#[derive(Deserialize, Default)]
pub struct Edit {
    favorite: Option<bool>,
    archived: Option<bool>,
    /// 1 to 5, or null for no rating.
    #[serde(default, deserialize_with = "nullable")]
    rating: Option<Option<i16>>,
    description: Option<String>,
    /// Unix seconds, or null to go back to what the camera said.
    #[serde(default, deserialize_with = "nullable")]
    taken_at: Option<Option<i64>>,
    /// A place, or null to go back to what the camera said.
    #[serde(default, deserialize_with = "nullable")]
    location: Option<Option<Position>>,
    /// The complete list of tags the file should have.
    tags: Option<Vec<String>>,
}

impl Edit {
    /// Refuses what cannot be stored: a rating out of range, a description too long, a date or place that does not exist.
    fn check(&self) -> Result<(), ApiError> {
        if let Some(Some(r)) = self.rating {
            if !(1..=5).contains(&r) {
                return Err(ApiError::bad("A rating is 1 to 5 stars"));
            }
        }
        if self.description.as_ref().is_some_and(|d| d.chars().count() > MAX_DESCRIPTION) {
            return Err(ApiError::bad(format!("A description can have at most {MAX_DESCRIPTION} characters")));
        }
        if let Some(Some(t)) = self.taken_at {
            // Years 1 to 9999: what a date can show.
            if !(-62_135_596_800..=253_402_300_799).contains(&t) {
                return Err(ApiError::bad("That date is out of range"));
            }
        }
        if let Some(Some(p)) = &self.location {
            if meta::valid_position(p.lat, p.lon).is_none() {
                return Err(ApiError::bad("Latitude is -90 to 90, longitude -180 to 180 (and not 0, 0)"));
            }
        }
        Ok(())
    }
}

/// Tag names as they will be stored: trimmed, no empty ones, no duplicates (case does not matter).
fn clean_tags(names: &[String]) -> Result<Vec<String>, ApiError> {
    let mut out: Vec<String> = Vec::new();
    for name in names {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if name.chars().count() > 64 {
            return Err(ApiError::bad("A tag can have at most 64 characters"));
        }
        if !out.iter().any(|t| t.to_lowercase() == name.to_lowercase()) {
            out.push(name.to_string());
        }
    }
    if out.len() > MAX_TAGS_PER_FILE {
        return Err(ApiError::bad(format!("A file can have at most {MAX_TAGS_PER_FILE} tags")));
    }
    Ok(out)
}

/// The ids of these tags of the user, made when they are new.
fn tag_ids(tx: &mut impl GenericClient, uid: i64, names: &[String]) -> Result<Vec<uuid::Uuid>, postgres::Error> {
    names.iter().map(|name| tags::ensure(tx, uid, name)).collect()
}

/// Applies an edit to one file, for the user who made it. The caller's transaction makes it all or nothing.
fn apply(tx: &mut impl GenericClient, who: &Access, id: uuid::Uuid, e: &Edit) -> anyhow::Result<()> {
    who.require_workspace()?;
    let uid = who.user_id;
    e.check()?;
    let new_tags = e.tags.as_deref().map(clean_tags).transpose()?;

    if !file_state::file_is_visible(tx, who.viewer(), id)? {
        return Err(ApiError::not_found("No such file").into());
    }
    let now = unix_now();
    file_state::ensure(tx, uid, id, now)?;
    if let Some(v) = e.favorite {
        file_state::set_favorite(tx, uid, id, v)?;
    }
    if let Some(v) = e.archived {
        file_state::set_archived(tx, uid, id, v)?;
    }
    if let Some(v) = e.rating {
        file_state::set_rating(tx, uid, id, v)?;
    }
    if let Some(v) = &e.description {
        file_state::set_description(tx, uid, id, v.trim())?;
    }
    if let Some(v) = e.taken_at {
        file_state::set_taken_at(tx, uid, id, v)?;
    }
    if let Some(v) = &e.location {
        file_state::set_place(tx, uid, id, v.as_ref().map(|p| (p.lat, p.lon)))?;
    }
    file_state::touch(tx, uid, id, now)?;
    if let Some(names) = new_tags {
        let ids = tag_ids(tx, uid, &names)?;
        tags::replace_on_file(tx, uid, id, &ids)?;
        sql::library::drop_unused_tags().bind(tx).map(drop)?;
    }
    Ok(())
}

pub async fn edit(State(state): State<Shared>, Extension(who): Extension<Access>, Path(Id(id)): Path<Id>, Json(e): Json<Edit>) -> Res<Done> {
    change(&state, crate::platform::db::Topic::Library, move |tx| apply(tx, &who, id, &e)).await?;
    done()
}

#[derive(Deserialize)]
pub struct Bulk {
    ids: Vec<Id>,
    favorite: Option<bool>,
    archived: Option<bool>,
    add_tags: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct Changed {
    changed: u64,
}

/// The same change to many files at once (what the selection bar does).
pub async fn bulk(State(state): State<Shared>, Extension(who): Extension<Access>, Json(b): Json<Bulk>) -> Res<Changed> {
    who.require_workspace()?;
    if b.ids.is_empty() {
        return Err(ApiError::bad("Nothing selected"));
    }
    if b.ids.len() > MAX_BULK {
        return Err(ApiError::bad(format!("Select at most {MAX_BULK} files at a time")));
    }
    let new_tags = b.add_tags.as_deref().map(clean_tags).transpose()?;
    let ids = uuids(&b.ids);
    let changed = change(&state, crate::platform::db::Topic::Library, move |tx| {
        let (viewer, now) = (who.viewer(), unix_now());
        // The files touched by any of the changes asked for are those the user can see.
        let mut changed = 0;
        if let Some(v) = b.favorite {
            changed = changed.max(file_state::bulk_set_favorite(tx, viewer, &ids, v, now)?);
        }
        if let Some(v) = b.archived {
            changed = changed.max(file_state::bulk_set_archived(tx, viewer, &ids, v, now)?);
        }
        if let Some(names) = new_tags.filter(|t| !t.is_empty()) {
            let tag_ids = tag_ids(tx, viewer.uid, &names)?;
            changed = changed.max(tags::add_to_files(tx, viewer, &ids, &tag_ids)?);
        }
        Ok(changed)
    })
    .await?;
    Ok(Json(Changed { changed }))
}

#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "TagInfo"))]
pub struct Tag {
    id: Id,
    name: String,
    count: i64,
}

pub async fn tags(State(state): State<Shared>, Extension(who): Extension<Access>) -> Res<Vec<Tag>> {
    let out = run(&state, move |c| {
        Ok(tags::counted(c, who.viewer())?
            .into_iter()
            .map(|t| Tag { id: Id(t.id), name: t.name, count: t.files })
            .collect())
    })
    .await?;
    Ok(Json(out))
}

pub async fn delete_tag(State(state): State<Shared>, Extension(who): Extension<Access>, Path(Id(id)): Path<Id>) -> Res<Done> {
    act(&state, move |c| {
        if !tags::delete(c, who.user_id, id)? {
            return Err(ApiError::not_found("No such tag").into());
        }
        db::notify(c, db::Topic::Library, "")?;
        Ok(())
    })
    .await
}
