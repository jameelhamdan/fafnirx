//! Devices: names, tokens, releasing one, and what the admin sets up.

use super::*;

#[test]
fn devices_are_slugs_and_tokens_are_shown_once() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        assert_eq!(
            api.json(Method::POST, "/api/devices", json!({"name": "Bad Name"})).await.0,
            StatusCode::BAD_REQUEST
        );
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
        let direct =
            json!({"name": "offsite", "remote_type": "S3", "options": "region=eu\nprovider=AWS", "remote_path": "b/x"});
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
                    Ok(c.query_one("SELECT instance_id FROM devices WHERE name = 'pc'", &[])?
                        .get::<_, Option<String>>(0))
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
