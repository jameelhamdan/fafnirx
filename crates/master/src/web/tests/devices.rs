//! Devices: names, tokens, releasing one, and what the admin sets up.

use super::*;

#[test]
fn devices_are_slugs_and_tokens_are_shown_once() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        assert_eq!(api.json(Method::POST, "/api/devices", json!({"name": "Bad Name"})).await.0, StatusCode::BAD_REQUEST);
        let (status, made) = api.json(Method::POST, "/api/devices", json!({"name": " Laptop "})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(made["device"]["name"], "laptop");
        assert!(made["token"].as_str().unwrap().len() >= 40);
        assert_eq!(api.json(Method::POST, "/api/devices", json!({"name": "laptop"})).await.0, StatusCode::CONFLICT);
        let (_, again) = api.get("/api/devices/admin%2Flaptop").await;
        assert!(again["token"].is_null(), "the token is never shown again");
        let (_, rotated) = api.json(Method::POST, "/api/devices/admin%2Flaptop/token", json!({})).await;
        assert_ne!(rotated["token"], made["token"]);
        assert_eq!(api.json(Method::POST, "/api/devices/admin%2Flaptop/revoke", json!({})).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/devices/admin%2Flaptop").await.1["device"]["revoked"], true);
    });
}

#[test]
fn what_the_admin_sets_up_stays_in_the_database_and_the_file_is_not_touched() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        // A mirror needs a remote of the rclone config or a backend.
        assert_eq!(api.json(Method::POST, "/api/mirrors", json!({"name": "m"})).await.0, StatusCode::BAD_REQUEST);
        let mirror = json!({"name": "vault", "remote": "crypt:backups/fafnirx", "keep_runs": 5});
        assert_eq!(api.json(Method::POST, "/api/mirrors", mirror).await.0, StatusCode::OK);
        let direct = json!({"name": "offsite", "remote_type": "S3", "options": "region=eu\nprovider=AWS", "remote_path": "b/x"});
        assert_eq!(api.json(Method::POST, "/api/mirrors", direct).await.0, StatusCode::OK);
        api.json(Method::PUT, "/api/settings", json!({"version_count": 9, "retention_days": 45})).await;

        let (_, mirrors) = api.get("/api/mirrors").await;
        assert_eq!(mirrors.as_array().unwrap().len(), 2);
        assert_eq!(mirrors[1]["detail"]["option_keys"], json!([]), "the mirror of the rclone config has no options");
        assert_eq!(
            mirrors[0]["detail"]["option_keys"],
            json!(["provider", "region"]),
            "credentials are never sent back"
        );
        let (_, settings) = api.get("/api/settings").await;
        assert_eq!((settings["version_count"].clone(), settings["retention_days"].clone()), (json!(9), json!(45)));
    });
}

#[test]
fn a_device_can_be_released_for_another_computer() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        api.cookie = login(&api).await;
        let admin = api.cookie.clone();
        let (status, made) = api.json(Method::POST, "/api/devices", json!({"name": "pc"})).await;
        assert_eq!(status, StatusCode::OK, "{made}");
        let hold = |api: &Api| {
            let db = api.db.clone();
            async move {
                crate::platform::db::with(&db, |c| {
                    c.execute("UPDATE devices SET instance_id = 'abc' WHERE name = 'pc'", &[])?;
                    Ok(())
                })
                .await
                .unwrap();
            }
        };
        let held = |api: &Api| {
            let db = api.db.clone();
            async move {
                crate::platform::db::with(&db, |c| {
                    Ok(c.query_one("SELECT instance_id FROM devices WHERE name = 'pc'", &[])?.get::<_, Option<String>>(0))
                })
                .await
                .unwrap()
            }
        };
        hold(&api).await;
        api.cookie = bob;
        assert_eq!(api.json(Method::POST, "/api/devices/admin%2Fpc/release", json!({})).await.0, StatusCode::NOT_FOUND);
        assert_eq!(held(&api).await.as_deref(), Some("abc"), "someone else's device is not theirs to release");
        api.cookie = admin;
        assert_eq!(api.json(Method::POST, "/api/devices/admin%2Fpc/release", json!({})).await.0, StatusCode::OK);
        assert_eq!(held(&api).await, None);
        // A new token is for a new computer: it frees the device too.
        hold(&api).await;
        assert_eq!(api.json(Method::POST, "/api/devices/admin%2Fpc/token", json!({})).await.0, StatusCode::OK);
        assert_eq!(held(&api).await, None);
        assert_eq!(
            api.json(Method::POST, "/api/devices/admin%2Fnope/release", json!({})).await.0,
            StatusCode::NOT_FOUND
        );
    });
}

#[test]
fn a_mirror_holds_one_whole_folder_or_everything() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        api.cookie = bob.clone();
        api.json(Method::POST, "/api/folders", json!({"name": "family"})).await;
        api.cookie = login(&api).await;
        master_folder(&api, "Photos").await;

        let offered = api.get("/api/mirrors/folders").await.1;
        let offered: Vec<&str> = offered.as_array().unwrap().iter().map(|f| f.as_str().unwrap()).collect();
        assert!(offered.contains(&"master:Photos") && offered.contains(&"bob/family"), "{offered:?}");

        // A mirror takes any of these; what is not a folder is refused.
        let new = |name: &str, folder: &str| json!({"name": name, "remote": "crypt:x", "folder": folder});
        assert_eq!(api.json(Method::POST, "/api/mirrors", new("a", "bob/nothing")).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(
            api.json(Method::POST, "/api/mirrors", new("a", "bob/family/2024")).await.0,
            StatusCode::BAD_REQUEST,
            "no part of a folder"
        );
        assert_eq!(api.json(Method::POST, "/api/mirrors", new("a", " /bob/family/ ")).await.0, StatusCode::OK);
        assert_eq!(api.json(Method::POST, "/api/mirrors", new("b", "")).await.0, StatusCode::OK);
        assert_eq!(api.json(Method::POST, "/api/mirrors", new("b", "")).await.0, StatusCode::CONFLICT);
        let listed = api.get("/api/mirrors").await.1;
        assert_eq!(listed[0]["detail"]["folder"], "bob/family", "stored clean");
        assert_eq!(listed[1]["detail"]["folder"], "", "everything");
    });
}

#[test]
fn a_mirror_copies_only_the_files_of_its_folder() {
    let db = crate::platform::db::testing::db_with_device();
    let mut c = db.get().unwrap();
    // Device 1 has the files `docs/a`, `docs/b`, `docsx/c` and `top`.
    for path in ["docs/a", "docs/b", "docsx/c", "top"] {
        let file: uuid::Uuid = c
            .query_one("INSERT INTO files (device_id, path) VALUES (1, $1) RETURNING id", &[&path])
            .unwrap()
            .get(0);
        let version: i64 = c
            .query_one(
                "INSERT INTO versions (file_id, root_hash, size, mtime, created_at) VALUES ($1, decode('00', 'hex'), 1, 0, 0) RETURNING id",
                &[&file],
            )
            .unwrap()
            .get(0);
        c.execute("UPDATE files SET current_version_id = $1 WHERE id = $2", &[&version, &file]).unwrap();
    }
    let paths = |c: &mut postgres::Client, folder: Option<i64>| -> Vec<String> { crate::repo::mirrors::live_files(c, folder).unwrap().into_iter().map(|f| f.path).collect() };
    assert_eq!(paths(&mut c, None).len(), 4, "no folder is everything");
    assert_eq!(paths(&mut c, Some(1)), ["docs/a", "docs/b", "docsx/c", "top"]);
    assert!(paths(&mut c, Some(0)).is_empty(), "another folder holds none of them");
}
