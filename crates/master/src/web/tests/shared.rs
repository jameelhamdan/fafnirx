//! Shared folders in the web app: the owner makes them and decides who else may use them.

use super::*;

fn members(folders: &Value, folder: &str) -> Vec<(String, bool)> {
    let found = folders.as_array().unwrap().iter().find(|f| f["folder"] == folder).expect("the folder is listed");
    found["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["user"].as_str().unwrap().into(), m["can_write"] == true))
        .collect()
}

#[test]
fn the_owner_makes_a_shared_folder_and_lets_other_users_in_one_by_one() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let alice = add_user(&mut api, "alice", "normal").await;
        let carol = add_user(&mut api, "carol", "readonly").await;

        // Bob makes one: named like a device, in his folder, empty and private.
        api.cookie = bob.clone();
        assert_eq!(api.json(Method::POST, "/api/shared", json!({"name": "Bad Name"})).await.0, StatusCode::BAD_REQUEST);
        let (status, body) = api.json(Method::POST, "/api/shared", json!({"name": " Family "})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let made = api.get("/api/shared").await.1;
        assert_eq!(made[0]["folder"], "bob/family");
        assert_eq!((made[0]["mine"].clone(), made[0]["can_write"].clone()), (json!(true), json!(true)));
        assert!(members(&made, "bob/family").is_empty(), "nobody else can use it yet");
        assert!(api._dir.path().join("files/users/bob/family").is_dir(), "the folder exists for a network share");
        assert_eq!(api.json(Method::POST, "/api/shared", json!({"name": "family"})).await.0, StatusCode::CONFLICT);

        // It is a folder of Bob's that the web app can change, like a local one.
        assert_eq!(try_upload(&api, "bob/family", "a.txt", b"hello").await, StatusCode::OK);
        let (_, inside) = api.get("/api/files?device=bob/family").await;
        assert_eq!((inside["device_kind"].clone(), inside["writable"].clone()), (json!("shared"), json!(true)));

        // Nobody else sees it, and nobody but its owner manages it.
        api.cookie = alice.clone();
        assert_eq!(api.get("/api/shared").await.1, json!([]));
        assert_eq!(api.get("/api/files?device=bob/family").await.0, StatusCode::NOT_FOUND);
        let add = json!({"user": "alice"});
        assert_eq!(api.json(Method::POST, "/api/shared/family/members", add.clone()).await.0, StatusCode::NOT_FOUND);

        // Bob adds Alice to read only.
        api.cookie = bob.clone();
        let (status, body) =
            api.json(Method::POST, "/api/shared/family/members", json!({"user": "alice", "can_write": false})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(members(&api.get("/api/shared").await.1, "bob/family"), [("alice".to_string(), false)]);
        for (body, why) in [
            (json!({"user": "bob"}), "he already has it on all his computers"),
            (json!({"user": "ghost"}), "there is no such user"),
            (json!({"user": "carol", "can_write": true}), "a read-only account cannot be given write access"),
        ] {
            assert_eq!(
                api.json(Method::POST, "/api/shared/family/members", body).await.0,
                StatusCode::BAD_REQUEST,
                "{why}"
            );
        }
        let (status, body) =
            api.json(Method::POST, "/api/shared/family/members", json!({"user": "carol", "can_write": false})).await;
        assert_eq!(status, StatusCode::OK, "{body}");

        // Alice now sees it, as Bob's and read-only, without the list of who else has it.
        api.cookie = alice.clone();
        let (_, theirs) = api.get("/api/shared").await;
        assert_eq!(theirs.as_array().unwrap().len(), 1);
        assert_eq!(theirs[0]["folder"], "bob/family");
        assert_eq!((theirs[0]["mine"].clone(), theirs[0]["can_write"].clone()), (json!(false), json!(false)));
        assert_eq!(theirs[0]["members"], json!([]));
        assert_eq!(
            api.json(Method::DELETE, "/api/shared/family", json!({})).await.0,
            StatusCode::NOT_FOUND,
            "not hers to delete"
        );

        // She sees it in the web app too: in the folder list, its files, and as a device. She may not change it yet.
        let (status, inside) = api.get("/api/files?device=bob/family").await;
        assert_eq!(status, StatusCode::OK, "{inside}");
        assert_eq!((inside["device_kind"].clone(), inside["writable"].clone()), (json!("shared"), json!(false)));
        assert_eq!(inside["entries"][0]["name"], "a.txt");
        let (_, top) = api.get("/api/files").await;
        assert!(
            top["entries"].as_array().unwrap().iter().any(|e| e["device"] == "bob/family"),
            "listed in the folders"
        );
        let (_, devices) = api.get("/api/devices").await;
        assert!(devices.as_array().unwrap().iter().any(|d| d["folder"] == "bob/family" && d["kind"] == "shared"));
        assert_eq!(try_upload(&api, "bob/family", "b.txt", b"hers").await, StatusCode::FORBIDDEN, "read-only");
        let (status, _, _) =
            api.send(Method::GET, "/api/files/download?device=bob/family&path=a.txt", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK, "and she can download");

        // Carol can read it too, but a read-only account has no shared folders of its own.
        api.cookie = carol.clone();
        assert_eq!(api.get("/api/shared").await.1.as_array().unwrap().len(), 1);
        assert_eq!(api.get("/api/files?device=bob/family").await.0, StatusCode::OK, "a read-only account reads it");
        assert_eq!(api.json(Method::POST, "/api/shared", json!({"name": "mine"})).await.0, StatusCode::FORBIDDEN);

        // Bob lets Alice change it, and she can upload.
        api.cookie = bob.clone();
        api.json(Method::POST, "/api/shared/family/members", json!({"user": "alice", "can_write": true})).await;
        assert_eq!(
            members(&api.get("/api/shared").await.1, "bob/family"),
            [("alice".to_string(), true), ("carol".to_string(), false)]
        );
        api.cookie = alice.clone();
        assert_eq!(try_upload(&api, "bob/family", "b.txt", b"hers").await, StatusCode::OK);

        // Then he takes her out again, and it is gone for her everywhere.
        api.cookie = bob.clone();
        let (status, body) = api.json(Method::DELETE, "/api/shared/family/members/alice", json!({})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(members(&api.get("/api/shared").await.1, "bob/family"), [("carol".to_string(), false)]);
        assert_eq!(
            api.json(Method::DELETE, "/api/shared/family/members/alice", json!({})).await.0,
            StatusCode::NOT_FOUND
        );
        api.cookie = alice.clone();
        assert_eq!(api.get("/api/shared").await.1, json!([]), "and it is gone for her");
        assert_eq!(api.get("/api/files?device=bob/family").await.0, StatusCode::NOT_FOUND);

        // A folder with files cannot be deleted. Once they are deleted it can, and it takes the trash and the history along.
        api.cookie = bob.clone();
        assert_eq!(api.json(Method::DELETE, "/api/shared/family", json!({})).await.0, StatusCode::CONFLICT);
        let both = json!({"items": [
            {"device": "bob/family", "path": "a.txt"},
            {"device": "bob/family", "path": "b.txt"},
        ]});
        assert_eq!(api.json(Method::POST, "/api/files/delete", both).await.0, StatusCode::OK);
        assert_eq!(api.json(Method::DELETE, "/api/shared/family", json!({})).await.0, StatusCode::OK);
        let (_, left) = api.get("/api/shared").await;
        assert_eq!(left, json!([]));
        assert!(!api._dir.path().join("files/users/bob/family").exists(), "and its folder");
        let versions = std::fs::read_dir(api._dir.path().join("versions")).map(|d| d.count()).unwrap_or(0);
        assert_eq!(versions, 0, "and the stored copies of the deleted files");
        assert_eq!(api.get("/api/files?device=bob/family").await.0, StatusCode::NOT_FOUND);
        // The name is free again.
        assert_eq!(api.json(Method::POST, "/api/shared", json!({"name": "family"})).await.0, StatusCode::OK);
    });
}
