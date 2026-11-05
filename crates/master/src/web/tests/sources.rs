//! Sources: the folder they write into, how they are edited, and that a folder they write into can be renamed.

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

        // With no folder named it gets one of its own; with one it writes there.
        assert_eq!(api.json(Method::POST, "/api/sources", source("alone", None)).await.0, StatusCode::OK);
        assert_eq!(listed(&api, "alone").await["detail"]["folder"], "admin/source-alone");
        let place = json!({"device": "admin/media"});
        let (status, body) = api.json(Method::POST, "/api/sources", source("pics", Some(place))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(listed(&api, "pics").await["detail"]["folder"], "admin/media");
        assert_eq!(api.json(Method::POST, "/api/sources", source("pics", None)).await.0, StatusCode::CONFLICT);

        // Not just anywhere: the master folders are open to it, but not a folder that does not exist.
        master_folder(&api, "Media").await;
        let master = json!({"device": "master:Media"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("feeds", Some(master))).await.0, StatusCode::OK);
        let nowhere = json!({"device": "admin/nothing"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("bad", Some(nowhere))).await.0, StatusCode::NOT_FOUND);

        // Editing: the name, the schedule and the folder. A name that is taken is refused.
        let edit = |body: Value| async { api.json(Method::PATCH, "/api/sources/admin/pics", body).await };
        assert_eq!(edit(json!({"name": "alone"})).await.0, StatusCode::CONFLICT);
        assert_eq!(edit(json!({"name": "Photos", "interval_hours": 12})).await.0, StatusCode::OK);
        let renamed = listed(&api, "photos").await;
        assert_eq!(
            (renamed["key"].clone(), renamed["interval_secs"].clone()),
            (json!("admin/photos"), json!(12 * 3600))
        );
        let moved = json!({"folder": {"device": "admin/source-alone"}});
        assert_eq!(api.json(Method::PATCH, "/api/sources/admin/photos", moved).await.0, StatusCode::OK);
        assert_eq!(listed(&api, "photos").await["detail"]["folder"], "admin/source-alone");
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
        let place = json!({"device": "admin/media"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("feed", Some(place))).await.0, StatusCode::OK);
        // One file the source collected (as its record says), and one somebody put there.
        assert_eq!(try_upload(&api, "admin/media", "from-source.txt", b"a").await, StatusCode::OK);
        assert_eq!(try_upload(&api, "admin/media", "mine.txt", b"b").await, StatusCode::OK);
        let db = api.db.clone();
        crate::platform::db::with(&db, |c| {
            c.execute(
                "INSERT INTO link_items (link_id, device_id, path, remote_id, version) SELECT id, folder_id, 'from-source.txt', 'r', 'm' FROM links",
                &[],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        // The folder is renamed: the source still writes into it, under its new name.
        let (status, renamed) = api.json(Method::POST, "/api/devices/admin%2Fmedia/rename", json!({"name": "films"})).await;
        assert_eq!(status, StatusCode::OK, "{renamed}");
        assert_eq!(listed(&api, "feed").await["detail"]["folder"], "admin/films");
        assert_eq!(listed(&api, "feed").await["detail"]["files"], 1);

        // Deleting the source deletes what it collected, and nothing else in the folder.
        let del = api.send(Method::DELETE, "/api/sources/admin/feed", Body::empty(), None, &[]).await;
        assert_eq!(del.0, StatusCode::OK);
        let (_, inside) = api.get("/api/files?device=admin/films").await;
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
fn changing_the_folder_of_a_source_trashes_what_it_collected_and_it_collects_again() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        for name in ["media", "other"] {
            api.json(Method::POST, "/api/folders", json!({"name": name})).await;
        }
        let place = json!({"device": "admin/media"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("feed", Some(place))).await.0, StatusCode::OK);
        assert_eq!(try_upload(&api, "admin/media", "got.txt", b"collected").await, StatusCode::OK);
        assert_eq!(try_upload(&api, "admin/media", "mine.txt", b"somebody's").await, StatusCode::OK);
        let db = api.db.clone();
        crate::platform::db::with(&db, |c| {
            c.execute(
                "INSERT INTO link_items (link_id, device_id, path, remote_id, version) SELECT id, folder_id, 'got.txt', 'r', 'm' FROM links",
                &[],
            )?;
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(listed(&api, "feed").await["detail"]["files"], 1);

        // Another folder: what it collected goes to the Trash, the file somebody else put there stays, and it remembers nothing.
        let away = json!({"folder": {"device": "admin/other"}});
        assert_eq!(api.json(Method::PATCH, "/api/sources/admin/feed", away).await.0, StatusCode::OK);
        let (_, trash) = api.get("/api/files?device=admin/media&deleted=1").await;
        assert_eq!(trash["entries"][0]["name"], "got.txt", "{trash}");
        let (_, left) = api.get("/api/files?device=admin/media").await;
        assert_eq!(left["entries"][0]["name"], "mine.txt", "{left}");
        assert_eq!(listed(&api, "feed").await["detail"]["files"], 0, "it remembers nothing about the old folder");
        assert_eq!(listed(&api, "feed").await["detail"]["folder"], "admin/other");
    });
}

#[test]
fn the_runs_of_sources_are_in_the_same_list_as_the_other_work() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        let (status, body) = api
            .json(
                Method::POST,
                "/api/sources",
                json!({"name": "feed", "kind": "rclone", "remote_type": "local", "remote_path": "/nonexistent", "interval_hours": 24}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        crate::platform::db::with(&api.db, |c| {
            c.execute("UPDATE links SET last_status = 'failed', last_message = 'Could not read it', last_run_at = 5", &[])?;
            Ok(())
        })
        .await
        .unwrap();
        let queue = api.get("/api/queue").await.1;
        let run = queue["jobs"].as_array().unwrap().iter().find(|j| j["kind"] == "source").expect("the run is listed");
        assert_eq!(
            (run["status"].clone(), run["path"].clone(), run["error"].clone()),
            (json!("failed"), json!("admin/feed"), json!("Could not read it"))
        );
        let counted = queue["summary"].as_array().unwrap().iter().find(|s| s["kind"] == "source").expect("and counted");
        assert_eq!(counted["failed"], 1);
        // The folder page says what writes into the folder.
        let folders = api.get("/api/folders").await.1;
        let own = folders
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["folder"] == "admin/source-feed")
            .expect("the source's own folder");
        assert_eq!(own["rules"]["sources"][0]["key"], "admin/feed");
    });
}
