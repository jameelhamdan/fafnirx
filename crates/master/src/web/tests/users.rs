//! Users, roles and workspaces: what each role may reach.

use super::*;

#[test]
fn users_have_roles_and_the_last_administrator_stays() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let admin = api.cookie.clone();

        // Names are slugs, passwords have a length, and a user exists once.
        let weak = json!({"name": "dave", "role": "normal", "password": "short"});
        assert_eq!(api.json(Method::POST, "/api/users", weak).await.0, StatusCode::BAD_REQUEST);
        let bad_name = json!({"name": "Dave Smith", "role": "normal", "password": "password-1"});
        assert_eq!(api.json(Method::POST, "/api/users", bad_name).await.0, StatusCode::BAD_REQUEST);
        let no_role = json!({"name": "dave", "role": "boss", "password": "password-1"});
        assert_eq!(api.json(Method::POST, "/api/users", no_role).await.0, StatusCode::BAD_REQUEST);
        let twice = json!({"name": "bob", "role": "normal", "password": "password-1"});
        assert_eq!(api.json(Method::POST, "/api/users", twice).await.0, StatusCode::CONFLICT);
        assert!(api._dir.path().join("files/users/bob").is_dir(), "the folder of the user exists from the start");

        // Only an administrator manages users, settings and mirrors.
        api.cookie = bob.clone();
        for uri in ["/api/users", "/api/settings", "/api/mirrors"] {
            assert_eq!(api.get(uri).await.0, StatusCode::FORBIDDEN, "{uri}");
        }
        // Only an administrator asks for a rescan of the file store.
        assert_eq!(api.json(Method::POST, "/api/files/rescan", json!({})).await.0, StatusCode::FORBIDDEN);
        // Sources are per user: bob has his own, which nobody but an administrator sees.
        let source = |name: &str| json!({"name": name, "remote_type": "local", "remote_path": "/tmp"});
        assert_eq!(api.json(Method::POST, "/api/sources", source("bobs")).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/sources").await.1.as_array().unwrap().len(), 1);
        api.cookie = admin.clone();
        assert_eq!(api.json(Method::POST, "/api/sources", source("roots")).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/sources").await.1.as_array().unwrap().len(), 2, "an administrator sees all");
        api.cookie = bob.clone();
        assert_eq!(api.get("/api/sources").await.1.as_array().unwrap().len(), 1);
        assert_eq!(api.json(Method::POST, "/api/sources/admin/roots/toggle", json!({})).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.json(Method::POST, "/api/sources/bob/bobs/toggle", json!({})).await.0, StatusCode::OK);

        // A source name is only unique per user: the administrator can use bob's name, bob cannot use it twice.
        assert_eq!(api.json(Method::POST, "/api/sources", source("bobs")).await.0, StatusCode::CONFLICT);
        api.cookie = admin.clone();
        assert_eq!(api.json(Method::POST, "/api/files/rescan", json!({})).await.0, StatusCode::OK);
        assert_eq!(api.json(Method::POST, "/api/sources", source("bobs")).await.0, StatusCode::OK);
        let (_, all) = api.get("/api/sources").await;
        let keys: Vec<&str> = all.as_array().unwrap().iter().map(|s| s["key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["admin/bobs", "admin/roots", "bob/bobs"], "named by the user and the name");
        // The administrator can reach bob's source, and bob's route does not reach the administrator's.
        assert_eq!(api.json(Method::POST, "/api/sources/bob/bobs/toggle", json!({})).await.0, StatusCode::OK);
        api.cookie = bob.clone();
        assert_eq!(api.json(Method::POST, "/api/sources/admin/bobs/toggle", json!({})).await.0, StatusCode::NOT_FOUND);

        // What goes wrong with a source is for its owner (and the administrators), not for everyone.
        crate::platform::db::with(&api.db, |c| {
            crate::repo::alerts::upsert(c, "source_failed", "bob/source-bobs", "Drive said no")?;
            crate::repo::alerts::upsert(c, "source_failed", "admin/source-roots", "Drive said no")?;
            Ok(())
        })
        .await
        .unwrap();
        let (_, board) = api.get("/api/dashboard").await;
        let keys: Vec<&str> = board["alerts"].as_array().unwrap().iter().map(|a| a["key"].as_str().unwrap()).collect();
        assert_eq!(keys, ["bob/source-bobs"], "bob is told about his source only");
        api.cookie = admin.clone();
        assert_eq!(api.get("/api/dashboard").await.1["alerts"].as_array().unwrap().len(), 2);
        api.cookie = bob.clone();
        let (_, session) = api.get("/api/session").await;
        assert_eq!((session["user"]["name"].clone(), session["user"]["role"].clone()), (json!("bob"), json!("normal")));

        // The last administrator can be neither demoted nor disabled; with another one, either is fine.
        api.cookie = admin.clone();
        assert_eq!(
            api.json(Method::PATCH, "/api/users/admin", json!({"role": "normal"})).await.0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            api.json(Method::PATCH, "/api/users/admin", json!({"disabled": true})).await.0,
            StatusCode::CONFLICT
        );
        let second = add_user(&mut api, "root2", "admin").await;
        api.cookie = admin.clone();
        assert_eq!(api.json(Method::PATCH, "/api/users/root2", json!({"disabled": true})).await.0, StatusCode::OK);
        api.cookie = second;
        assert_eq!(api.get("/api/session").await.1["user"], Value::Null, "disabling ends the sessions at once");
        assert!(login_as(&api, "root2", "password-1").await.is_empty(), "and the user cannot log in");
        api.cookie = admin.clone();
        assert_eq!(api.json(Method::PATCH, "/api/users/root2", json!({"disabled": false})).await.0, StatusCode::OK);
        assert!(!login_as(&api, "root2", "password-1").await.is_empty());
        assert_eq!(
            api.json(Method::PATCH, "/api/users/nobody", json!({"role": "normal"})).await.0,
            StatusCode::NOT_FOUND
        );

        // Changing the role ends the sessions too, so that the new rights are the ones in force.
        assert_eq!(api.json(Method::PATCH, "/api/users/admin", json!({"role": "normal"})).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/users").await.0, StatusCode::UNAUTHORIZED);

        // A user changing their own password keeps the session they do it in; the others end.
        let other = login_as(&api, "bob", "password-1").await;
        api.cookie = bob;
        let wrong = json!({"current": "not it", "new": "password-2"});
        assert_eq!(api.json(Method::POST, "/api/me/password", wrong).await.0, StatusCode::BAD_REQUEST);
        let changed = json!({"current": "password-1", "new": "password-2"});
        assert_eq!(api.json(Method::POST, "/api/me/password", changed).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/session").await.1["user"]["name"], "bob", "this session stays");
        api.cookie = other;
        assert_eq!(api.get("/api/session").await.1["user"], Value::Null, "the other one ends");
        assert!(login_as(&api, "bob", "password-1").await.is_empty());
        assert!(!login_as(&api, "bob", "password-2").await.is_empty());
    });
}

#[test]
fn every_user_has_a_workspace_and_the_master_files_belong_to_everyone() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let alice = add_user(&mut api, "alice", "normal").await;
        let carol = add_user(&mut api, "carol", "readonly").await;
        let admin = login(&api).await;

        // The administrator puts a master file in place; nobody owns it.
        api.cookie = admin.clone();
        assert_eq!(try_upload(&api, "_master", "docs/readme.txt", b"for everyone").await, StatusCode::OK);
        assert!(api._dir.path().join("files/docs/readme.txt").is_file(), "master files are at the root of files/");
        let ghost = json!({"name": "a1", "user": "ghost"});
        assert_eq!(api.json(Method::POST, "/api/devices", ghost).await.0, StatusCode::BAD_REQUEST);
        let for_carol = json!({"name": "a1", "user": "carol"});
        assert_eq!(
            api.json(Method::POST, "/api/devices", for_carol).await.0,
            StatusCode::BAD_REQUEST,
            "a user who can only read has no devices"
        );

        // The same device name for two users is two devices, each in the folder of its user.
        for (who, cookie) in [("bob", &bob), ("alice", &alice)] {
            api.cookie = cookie.clone();
            let (status, made) =
                api.json(Method::POST, "/api/devices", json!({"name": "phone", "kind": "local"})).await;
            assert_eq!(status, StatusCode::OK, "{made}");
            assert_eq!(made["device"]["folder"], format!("{who}/phone"));
            assert!(api._dir.path().join(format!("files/users/{who}/phone")).is_dir());
            assert_eq!(api.json(Method::POST, "/api/devices", json!({"name": "phone"})).await.0, StatusCode::CONFLICT);
            assert_eq!(try_upload(&api, &format!("{who}/phone"), "me.txt", who.as_bytes()).await, StatusCode::OK);
        }

        // Each sees their own devices and the master files, and nothing of the other's.
        api.cookie = bob.clone();
        let (_, devices) = api.get("/api/devices").await;
        assert_eq!(devices.as_array().unwrap().len(), 1);
        assert_eq!(devices[0]["folder"], "bob/phone");
        let (_, top) = api.get("/api/files").await;
        let names: Vec<_> = top["entries"].as_array().unwrap().iter().map(|e| e["device"].as_str().unwrap()).collect();
        assert_eq!(names, ["_master", "bob/phone"]);
        // What the web app may change is told with each entry: the master files and his local folder, both his to write.
        let kinds: Vec<_> = top["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["device_kind"].as_str().unwrap(), e["writable"].as_bool().unwrap()))
            .collect();
        assert_eq!(kinds, [("master", true), ("local", true)]);
        let (_, inside) = api.get("/api/files?device=bob/phone").await;
        assert_eq!((inside["device_kind"].clone(), inside["writable"].clone()), (json!("local"), json!(true)));
        assert_eq!(api.get("/api/files?device=alice/phone").await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/files/info?device=alice/phone&path=me.txt").await.0, StatusCode::NOT_FOUND);
        let (status, _, _) =
            api.send(Method::GET, "/api/files/download?device=alice/phone&path=me.txt", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/devices/alice%2Fphone").await.0, StatusCode::NOT_FOUND);
        let revoke = api.json(Method::POST, "/api/devices/alice%2Fphone/revoke", json!({})).await;
        assert_eq!(revoke.0, StatusCode::NOT_FOUND);
        assert_eq!(
            try_upload(&api, "alice/phone", "x.txt", b"x").await,
            StatusCode::NOT_FOUND,
            "no writing in another workspace"
        );
        let delete = json!({"items": [{"device": "alice/phone", "path": "me.txt"}]});
        assert_eq!(api.json(Method::POST, "/api/files/delete", delete).await.0, StatusCode::NOT_FOUND);
        let (_, body) = api.get("/api/files?device=_master&path=docs").await;
        assert_eq!(body["entries"][0]["name"], "readme.txt");
        // A normal user writes to the master files too, but not under users/, where the folders of the users are kept.
        assert_eq!(try_upload(&api, "_master", "docs/from-bob.txt", b"hi").await, StatusCode::OK);
        assert_eq!(
            try_upload(&api, "_master", "users/sneaky.txt", b"no").await,
            StatusCode::CONFLICT,
            "users/ is where the folders of the users are kept"
        );
        let (_, dash) = api.get("/api/dashboard").await;
        assert_eq!(dash["devices"].as_array().unwrap().len(), 1);
        assert_eq!(dash["mirrors"], json!([]));

        // Read-only: sees the master files, has nothing of its own and changes nothing.
        api.cookie = carol.clone();
        let (_, top) = api.get("/api/files").await;
        let names: Vec<_> = top["entries"].as_array().unwrap().iter().map(|e| e["device"].as_str().unwrap()).collect();
        assert_eq!(names, ["_master"]);
        assert_eq!(api.get("/api/devices").await.1, json!([]));
        assert_eq!(api.get("/api/sources").await.0, StatusCode::FORBIDDEN, "no workspace, no sources");
        let (status, _, body) = api
            .send(Method::GET, "/api/files/download?device=_master&path=docs/readme.txt", Body::empty(), None, &[])
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"for everyone"[..]));
        assert_eq!(api.get("/api/files?device=bob/phone").await.0, StatusCode::NOT_FOUND);
        assert_eq!(try_upload(&api, "_master", "docs/x.txt", b"x").await, StatusCode::FORBIDDEN);
        let own = json!({"name": "mine", "kind": "local"});
        assert_eq!(api.json(Method::POST, "/api/devices", own).await.0, StatusCode::FORBIDDEN);
        let delete = json!({"items": [{"device": "_master", "path": "docs/readme.txt"}]});
        assert_eq!(api.json(Method::POST, "/api/files/delete", delete).await.0, StatusCode::FORBIDDEN);
        assert_eq!(api.json(Method::POST, "/api/albums", json!({"name": "mine"})).await.0, StatusCode::FORBIDDEN);

        // The administrator reaches everything.
        api.cookie = admin;
        let (_, devices) = api.get("/api/devices").await;
        let folders: Vec<_> = devices.as_array().unwrap().iter().map(|d| d["folder"].as_str().unwrap()).collect();
        assert_eq!(folders, ["alice/phone", "bob/phone"]);
        let (status, _, body) =
            api.send(Method::GET, "/api/files/download?device=bob/phone&path=me.txt", Body::empty(), None, &[]).await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"bob"[..]));
        assert_eq!(try_upload(&api, "alice/phone", "from-admin.txt", b"x").await, StatusCode::OK);
        let (status, made) = api.json(Method::POST, "/api/devices", json!({"name": "tablet", "user": "bob"})).await;
        assert_eq!((status, made["device"]["folder"].clone()), (StatusCode::OK, json!("bob/tablet")));

        // The folders of the users are in users/, so a user can have the name of a master folder.
        let docs = json!({"name": "docs", "role": "normal", "password": "password-1"});
        assert_eq!(api.json(Method::POST, "/api/users", docs).await.0, StatusCode::OK);
    });
}

#[test]
fn what_people_say_about_a_master_file_is_their_own() {
    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let alice = add_user(&mut api, "alice", "normal").await;
        let carol = add_user(&mut api, "carol", "readonly").await;
        api.cookie = login(&api).await;
        assert_eq!(try_upload(&api, "_master", "shared/pic.png", &png(8, 4, [9, 9, 9])).await, StatusCode::OK);

        api.cookie = bob.clone();
        let id = photo_id(&api, "pic.png").await;
        let edit = json!({"favorite": true, "description": "bob's view", "tags": ["Holiday"]});
        assert_eq!(api.json(Method::PATCH, &format!("/api/library/{id}"), edit).await.0, StatusCode::OK);
        let (status, album) = api.json(Method::POST, "/api/albums", json!({"name": "Bob's", "ids": [id]})).await;
        assert_eq!((status, album["count"].clone()), (StatusCode::OK, json!(1)));
        let album_id = album["id"].as_str().unwrap().to_string();
        assert_eq!(api.get("/api/timeline/buckets?scope=favorites").await.1["total"], 1);

        // Alice sees the same photo with nothing said about it, and none of Bob's albums or tags.
        api.cookie = alice.clone();
        assert_eq!(photo_id(&api, "pic.png").await, id, "one file");
        let detail = api.get(&format!("/api/library/{id}")).await.1;
        let said = (detail["favorite"].clone(), detail["description"].clone(), detail["tags"].clone());
        assert_eq!(said, (json!(false), json!(""), json!([])));
        assert_eq!(api.get("/api/timeline/buckets?scope=favorites").await.1["total"], 0);
        assert_eq!(api.get("/api/albums").await.1, json!([]));
        assert_eq!(api.get(&format!("/api/albums/{album_id}")).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/tags").await.1, json!([]));
        let del = api.send(Method::DELETE, &format!("/api/albums/{album_id}"), Body::empty(), None, &[]).await;
        assert_eq!(del.0, StatusCode::NOT_FOUND);
        api.json(Method::PATCH, &format!("/api/library/{id}"), json!({"tags": ["Holiday"], "rating": 2})).await;
        let mine = api.get(&format!("/api/library/{id}")).await.1;
        assert_eq!((mine["rating"].clone(), mine["tags"][0]["name"].clone()), (json!(2), json!("Holiday")));
        api.json(Method::PATCH, &format!("/api/library/{id}"), json!({"tags": []})).await;

        // Bob's tag is still there after Alice took hers off, and his state is as he left it.
        api.cookie = bob;
        let theirs = api.get(&format!("/api/library/{id}")).await.1;
        let said = (theirs["favorite"].clone(), theirs["rating"].clone(), theirs["tags"][0]["name"].clone());
        assert_eq!(said, (json!(true), Value::Null, json!("Holiday")));

        // Read-only users see the photo and cannot say anything about it.
        api.cookie = carol;
        assert_eq!(photo_id(&api, "pic.png").await, id);
        let fav = json!({"favorite": true});
        assert_eq!(api.json(Method::PATCH, &format!("/api/library/{id}"), fav).await.0, StatusCode::FORBIDDEN);
        let bulk = json!({"ids": [id], "favorite": true});
        assert_eq!(api.json(Method::POST, "/api/library/bulk", bulk).await.0, StatusCode::FORBIDDEN);
        let (status, _, _) =
            api.send(Method::GET, &format!("/api/media/{id}/original"), Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
    });
}
