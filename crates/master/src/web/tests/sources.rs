//! Sources: where they write, how they are edited, and that a folder they write into can be renamed.

use super::*;

fn source(name: &str, folder: Option<Value>) -> Value {
    let mut body = json!({"name": name, "remote_type": "local", "remote_path": "/data", "interval_hours": 6});
    if let Some(folder) = folder {
        body["folder"] = folder;
    }
    body
}

async fn listed(api: &Api, name: &str) -> Value {
    let (_, all) = api.get("/api/sources").await;
    all.as_array().unwrap().iter().find(|s| s["name"] == name).cloned().unwrap_or(Value::Null)
}

#[test]
fn a_source_writes_into_a_chosen_folder_and_can_be_edited() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "media"})).await;

        // With no folder named it gets one of its own; with one it writes there, in the subfolder.
        assert_eq!(api.json(Method::POST, "/api/sources", source("alone", None)).await.0, StatusCode::OK);
        assert_eq!(listed(&api, "alone").await["detail"]["folder"], json!({"device": "admin/source-alone", "path": ""}));
        let place = json!({"device": "admin/media", "path": "/Shows/Pics/"});
        let (status, body) = api.json(Method::POST, "/api/sources", source("pics", Some(place))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(
            listed(&api, "pics").await["detail"]["folder"],
            json!({"device": "admin/media", "path": "Shows/Pics"})
        );
        assert_eq!(api.json(Method::POST, "/api/sources", source("pics", None)).await.0, StatusCode::CONFLICT);

        // Not just anywhere: the master files are open to it, but not their users/ folder, and not a folder that does not exist.
        let master = json!({"device": "_master", "path": "Media/Feeds"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("feeds", Some(master))).await.0, StatusCode::OK);
        let reserved = json!({"device": "_master", "path": "users/x"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("bad", Some(reserved))).await.0, StatusCode::CONFLICT);
        let nowhere = json!({"device": "admin/nothing", "path": ""});
        assert_eq!(api.json(Method::POST, "/api/sources", source("bad", Some(nowhere))).await.0, StatusCode::NOT_FOUND);

        // Editing: the name, the schedule and the place. A name that is taken is refused.
        let edit = |body: Value| async { api.json(Method::PATCH, "/api/sources/admin/pics", body).await };
        assert_eq!(edit(json!({"name": "alone"})).await.0, StatusCode::CONFLICT);
        assert_eq!(edit(json!({"name": "Photos", "interval_hours": 12})).await.0, StatusCode::OK);
        let renamed = listed(&api, "photos").await;
        assert_eq!(
            (renamed["key"].clone(), renamed["interval_secs"].clone()),
            (json!("admin/photos"), json!(12 * 3600))
        );
        let moved = json!({"folder": {"device": "admin/media", "path": "Elsewhere"}});
        assert_eq!(api.json(Method::PATCH, "/api/sources/admin/photos", moved).await.0, StatusCode::OK);
        assert_eq!(listed(&api, "photos").await["detail"]["folder"]["path"], "Elsewhere");
        assert_eq!(api.json(Method::PATCH, "/api/sources/admin/ghost", json!({})).await.0, StatusCode::NOT_FOUND);
        let (_, kept) = api.json(Method::PATCH, "/api/sources/admin/photos", json!({"remote_path": "/other"})).await;
        assert!(kept.get("error").is_none(), "{kept}");
        assert_eq!(listed(&api, "photos").await["detail"]["remote"], "local:/other", "the rest stays as it was");
    });
}

#[test]
fn a_folder_with_a_source_in_it_can_be_renamed_and_deleting_the_source_leaves_the_other_files() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "media"})).await;
        let place = json!({"device": "admin/media", "path": "Feed"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("feed", Some(place))).await.0, StatusCode::OK);
        // One file the source collected (as its record says), and one somebody put there.
        assert_eq!(try_upload(&api, "admin/media", "Feed/from-source.txt", b"a").await, StatusCode::OK);
        assert_eq!(try_upload(&api, "admin/media", "Feed/mine.txt", b"b").await, StatusCode::OK);
        let db = api.db.clone();
        crate::platform::db::with(&db, |c| {
            c.execute(
                "INSERT INTO source_items (source_id, remote_id, path, remote_version) SELECT id, 'r', 'from-source.txt', 'm' FROM sources",
                &[],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        // The folder is renamed: the source still writes into it, under its new name.
        let (status, renamed) = api.json(Method::POST, "/api/devices/admin%2Fmedia/rename", json!({"name": "films"})).await;
        assert_eq!(status, StatusCode::OK, "{renamed}");
        assert_eq!(listed(&api, "feed").await["detail"]["folder"], json!({"device": "admin/films", "path": "Feed"}));
        assert_eq!(listed(&api, "feed").await["detail"]["files"], 1);

        // Deleting the source deletes what it collected, and nothing else in the folder.
        let del = api.send(Method::DELETE, "/api/sources/admin/feed", Body::empty(), None, &[]).await;
        assert_eq!(del.0, StatusCode::OK);
        let (_, inside) = api.get("/api/files?device=admin/films&path=Feed").await;
        let names: Vec<_> = inside["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["mine.txt"], "{inside}");
    });
}

#[test]
fn moving_a_folder_takes_the_albums_and_sources_that_point_into_it() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "media"})).await;
        assert_eq!(try_upload(&api, "admin/media", "Show/S1/a.txt", b"one").await, StatusCode::OK);
        let place = json!({"device": "admin/media", "path": "Show/Feed"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("feed", Some(place))).await.0, StatusCode::OK);
        let folder = json!({"device": "admin/media", "path": "Show"});
        let (_, album) = api.json(Method::POST, "/api/albums", json!({"name": "", "folder": folder})).await;
        let id = album["id"].as_str().unwrap().to_string();

        // Into a place that is not beside it: the album and the source still follow, because the folder moved as a whole.
        let mv = json!({"device": "admin/media", "from": "Show", "to": "Series/Shows/Show"});
        assert_eq!(api.json(Method::POST, "/api/files/move", mv).await.0, StatusCode::OK);
        assert_eq!(listed(&api, "feed").await["detail"]["folder"]["path"], "Series/Shows/Show/Feed");
        let (_, moved) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(moved["album"]["folder"]["path"], "Series/Shows/Show", "{moved}");
    });
}

#[test]
fn changing_where_a_source_writes_takes_what_it_collected_along() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        for name in ["media", "other"] {
            api.json(Method::POST, "/api/folders", json!({"name": name})).await;
        }
        let place = json!({"device": "admin/media", "path": "Feed"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("feed", Some(place))).await.0, StatusCode::OK);
        assert_eq!(try_upload(&api, "admin/media", "Feed/got.txt", b"collected").await, StatusCode::OK);
        assert_eq!(try_upload(&api, "admin/media", "Feed/mine.txt", b"somebody's").await, StatusCode::OK);
        let db = api.db.clone();
        crate::platform::db::with(&db, |c| {
            c.execute(
                "INSERT INTO source_items (source_id, remote_id, path, remote_version) SELECT id, 'r', 'got.txt', 'm' FROM sources",
                &[],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        let names = |dir: &'static str| {
            let api = &api;
            async move {
                let (_, inside) = api.get(&format!("/api/files?device=admin/{dir}")).await;
                inside["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| e["name"].as_str().unwrap().to_string())
                    .collect::<Vec<_>>()
            }
        };

        // Another subfolder of the same folder: the collected file moves, the other stays, and the source still knows its file.
        let to = json!({"folder": {"device": "admin/media", "path": "Moved/Here"}});
        assert_eq!(api.json(Method::PATCH, "/api/sources/admin/feed", to).await.0, StatusCode::OK);
        let (_, moved) = api.get("/api/files?device=admin/media&path=Moved/Here").await;
        assert_eq!(moved["entries"][0]["name"], "got.txt", "{moved}");
        let (_, left) = api.get("/api/files?device=admin/media&path=Feed").await;
        assert_eq!(left["entries"][0]["name"], "mine.txt", "the file somebody else put there stays");
        assert_eq!(listed(&api, "feed").await["detail"]["files"], 1);

        // A name in the way stops it before anything moves.
        assert_eq!(try_upload(&api, "admin/media", "Back/got.txt", b"in the way").await, StatusCode::OK);
        let back = json!({"folder": {"device": "admin/media", "path": "Back"}});
        assert_eq!(api.json(Method::PATCH, "/api/sources/admin/feed", back).await.0, StatusCode::CONFLICT);
        assert_eq!(listed(&api, "feed").await["detail"]["folder"]["path"], "Moved/Here");

        // Another folder altogether: what it collected goes to the Trash there, and it collects again where it writes now.
        let away = json!({"folder": {"device": "admin/other", "path": ""}});
        assert_eq!(api.json(Method::PATCH, "/api/sources/admin/feed", away).await.0, StatusCode::OK);
        let (_, trash) = api.get("/api/files?device=admin/media&path=Moved/Here&deleted=1").await;
        assert_eq!(trash["entries"][0]["name"], "got.txt", "{trash}");
        assert!(names("other").await.is_empty());
        assert_eq!(listed(&api, "feed").await["detail"]["files"], 0, "it remembers nothing about the old folder");
    });
}
