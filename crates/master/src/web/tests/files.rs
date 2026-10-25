//! The file browser: uploads, browsing, downloads, history and delete.

use super::*;

#[test]
fn upload_browse_download_history_and_delete() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        // Uploads go to local folders only.
        api.json(Method::POST, "/api/devices", json!({"name": "pc"})).await;
        let (status, _) =
            api.json(Method::POST, "/api/uploads", json!({"device": "admin/pc", "path": "a.txt", "size": 1})).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            api.json(Method::POST, "/api/devices", json!({"name": "photos", "kind": "local"})).await.0,
            StatusCode::OK
        );

        api.upload("admin/photos", "2024/a.txt", b"hello world").await;
        api.upload("admin/photos", "2024/b.txt", b"second").await;
        api.upload("admin/photos", "top.txt", b"top").await;

        let (_, before) = api.get("/api/files").await;
        assert!(
            before["entries"].as_array().unwrap().iter().any(|e| e["name"] == "photos" && e["files"] == 0),
            "the totals of a device are not live"
        );
        api.catch_up_totals().await;
        let (_, root) = api.get("/api/files").await;
        assert!(root["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "device" && e["name"] == "photos" && e["files"] == 3));
        let (_, top) = api.get("/api/files?device=admin/photos").await;
        let kinds: Vec<_> = top["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["kind"].as_str().unwrap(), e["name"].as_str().unwrap()))
            .collect();
        assert_eq!(kinds, [("dir", "2024"), ("file", "top.txt")]);
        let (_, year) = api.get("/api/files?device=admin/photos&path=2024&sort=size&order=desc").await;
        let names: Vec<_> = year["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["a.txt", "b.txt"], "largest first");
        let (_, found) = api.get("/api/files?device=admin/photos&q=B.TX").await;
        assert_eq!(found["entries"][0]["path"], "2024/b.txt");
        assert_eq!(api.get("/api/files?device=admin/photos&sort=bogus").await.0, StatusCode::BAD_REQUEST);

        let (status, headers, body) = api
            .send(Method::GET, "/api/files/download?device=admin/photos&path=2024/a.txt", Body::empty(), None, &[])
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"hello world"[..]));
        assert!(headers[header::CONTENT_DISPOSITION].to_str().unwrap().starts_with("attachment"));
        let (status, headers, body) = api
            .send(
                Method::GET,
                "/api/files/download?device=admin/photos&path=2024/a.txt",
                Body::empty(),
                None,
                &[("range", "bytes=6-")],
            )
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::PARTIAL_CONTENT, &b"world"[..]));
        assert_eq!(headers[header::CONTENT_RANGE], "bytes 6-10/11");

        let form = format!("items={}", urlencode(&json!([{"device": "admin/photos", "path": "2024"}]).to_string()));
        let (status, _, zipped) = api
            .send(Method::POST, "/api/files/zip", Body::from(form), Some("application/x-www-form-urlencoded"), &[])
            .await;
        assert_eq!(status, StatusCode::OK);
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zipped)).unwrap();
        let mut names: Vec<_> = archive.file_names().map(String::from).collect();
        names.sort();
        assert_eq!(names, ["2024/a.txt", "2024/b.txt"], "the zip starts at the selected folder");
        let mut text = String::new();
        std::io::Read::read_to_string(&mut archive.by_name("2024/b.txt").unwrap(), &mut text).unwrap();
        assert_eq!(text, "second");

        // A second version, then the history and a restore.
        api.upload("admin/photos", "2024/a.txt", b"hello again").await;
        let (_, info) = api.get("/api/files/info?device=admin/photos&path=2024/a.txt").await;
        let versions = info["versions"].as_array().unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().all(|v| v["available"] == true));
        let old = versions.iter().find(|v| v["current"] == false).unwrap()["id"].as_i64().unwrap();
        let (_, _, body) = api
            .send(
                Method::GET,
                &format!("/api/files/download?device=admin/photos&path=2024/a.txt&version={old}"),
                Body::empty(),
                None,
                &[],
            )
            .await;
        assert_eq!(body, b"hello world");
        assert_eq!(
            api.json(
                Method::POST,
                "/api/files/restore",
                json!({"device": "admin/photos", "path": "2024/a.txt", "version": old})
            )
            .await
            .0,
            StatusCode::OK
        );
        let (_, _, body) = api
            .send(Method::GET, "/api/files/download?device=admin/photos&path=2024/a.txt", Body::empty(), None, &[])
            .await;
        assert_eq!(body, b"hello world", "restored as the newest version");

        // Delete a folder: its files leave the listing but stay downloadable until retention removes them.
        let (_, removed) = api
            .json(Method::POST, "/api/files/delete", json!({"items": [{"device": "admin/photos", "path": "2024"}]}))
            .await;
        assert_eq!(removed["deleted"], 2);
        let (_, after) = api.get("/api/files?device=admin/photos").await;
        assert_eq!(after["entries"].as_array().unwrap().len(), 1);
        let (_, with_deleted) = api.get("/api/files?device=admin/photos&deleted=1").await;
        assert_eq!(with_deleted["entries"].as_array().unwrap().len(), 2);
        let (status, _, body) = api
            .send(Method::GET, "/api/files/download?device=admin/photos&path=2024/b.txt", Body::empty(), None, &[])
            .await;
        assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"second"[..]));
    });
}

#[test]
fn an_upload_resumes_where_it_stopped_and_rejects_gaps() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "up", "kind": "local"})).await;
        let (_, started) =
            api.json(Method::POST, "/api/uploads", json!({"device": "admin/up", "path": "f.bin", "size": 10})).await;
        let id = started["id"].as_str().unwrap();
        let put = |offset: u64, data: &'static [u8]| {
            api.send(Method::PUT, format!("/api/uploads/{id}?offset={offset}").leak(), Body::from(data), None, &[])
        };
        assert_eq!(put(0, b"01234").await.0, StatusCode::OK);
        let (status, _, body) = put(7, b"789").await;
        assert_eq!(status, StatusCode::CONFLICT, "a gap is refused");
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap()["received"],
            5,
            "and the server says where to continue"
        );
        assert_eq!(api.get(&format!("/api/uploads/{id}")).await.1["received"], 5);
        assert_eq!(
            api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await.0,
            StatusCode::CONFLICT,
            "incomplete"
        );
        assert_eq!(put(5, b"56789").await.0, StatusCode::OK);
        assert_eq!(api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await.0, StatusCode::OK);
    });
}

#[test]
fn an_upload_that_could_not_be_finished_can_be_finished_again() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "up", "kind": "local"})).await;
        assert_eq!(try_upload(&api, "admin/up", "a", b"one").await, StatusCode::OK);
        // `a` is a file now, so nothing can be placed below it.
        let (_, started) =
            api.json(Method::POST, "/api/uploads", json!({"device": "admin/up", "path": "a/b.txt", "size": 3})).await;
        let id = started["id"].as_str().unwrap();
        api.send(Method::PUT, &format!("/api/uploads/{id}?offset=0"), Body::from("two"), None, &[]).await;
        let first = api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await;
        assert_ne!(first.0, StatusCode::OK, "{}", first.1);
        let again = api.json(Method::POST, &format!("/api/uploads/{id}/finish"), json!({})).await;
        assert_eq!(again.0, first.0, "the upload is still there to be finished: {}", again.1);
        assert_eq!(api.get(&format!("/api/uploads/{id}")).await.1["received"], 3);
    });
}

#[test]
fn names_in_any_script_are_listed_downloaded_and_found() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        assert_eq!(
            api.json(Method::POST, "/api/devices", json!({"name": "d", "kind": "local"})).await.0,
            StatusCode::OK
        );
        // A folder in Arabic, a name with an accent written decomposed (as macOS does) and one in Japanese, at depth.
        let files = ["اناشيد/ألبوم الطفل/ملف.txt", "cafe\u{301}/menu.txt", "日本語/写真/猫.txt"];
        for path in files {
            assert_eq!(try_upload(&api, "admin/d", path, path.as_bytes()).await, StatusCode::OK, "{path}");
        }
        let names = |listing: &Value| -> Vec<String> {
            listing["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_string()).collect()
        };
        let (_, top) = api.get("/api/files?device=admin%2Fd").await;
        let mut top = names(&top);
        top.sort();
        assert_eq!(top, ["caf\u{e9}", "اناشيد", "日本語"], "names are whole, and stored composed");
        let (_, inner) = api.get("/api/files?device=admin%2Fd&path=%D8%A7%D9%86%D8%A7%D8%B4%D9%8A%D8%AF").await;
        assert_eq!(names(&inner), ["ألبوم الطفل"]);
        let (_, deep) = api
            .get("/api/files?device=admin%2Fd&path=%D8%A7%D9%86%D8%A7%D8%B4%D9%8A%D8%AF%2F%D8%A3%D9%84%D8%A8%D9%88%D9%85%20%D8%A7%D9%84%D8%B7%D9%81%D9%84")
            .await;
        assert_eq!(names(&deep), ["ملف.txt"]);
        let uri = "/api/files/download?device=admin%2Fd&path=%E6%97%A5%E6%9C%AC%E8%AA%9E%2F%E5%86%99%E7%9C%9F%2F%E7%8C%AB.txt";
        let (status, headers, body) = api.send(Method::GET, uri, Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "日本語/写真/猫.txt".as_bytes());
        let disposition = headers.get("content-disposition").unwrap().to_str().unwrap();
        assert!(disposition.contains("filename*=UTF-8''%E7%8C%AB.txt"), "{disposition}");
        let items = json!([{"device": "admin/d", "path": "اناشيد"}]).to_string();
        let form = format!("items={}&name={}", urlencode(&items), urlencode("اناشيد"));
        let (status, headers, zipped) = api
            .send(Method::POST, "/api/files/zip", Body::from(form), Some("application/x-www-form-urlencoded"), &[])
            .await;
        assert_eq!(status, StatusCode::OK);
        let disposition = headers.get("content-disposition").unwrap().to_str().unwrap();
        assert!(disposition.contains("filename*=UTF-8''%D8%A7%D9%86%D8%A7%D8%B4%D9%8A%D8%AF.zip"), "{disposition}");
        let archive = zip::ZipArchive::new(std::io::Cursor::new(zipped)).unwrap();
        assert_eq!(archive.file_names().collect::<Vec<_>>(), ["اناشيد/ألبوم الطفل/ملف.txt"]);
    });
}

#[test]
fn files_and_folders_of_the_master_can_be_moved_but_not_those_of_a_device() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "docs", "kind": "local"})).await;
        api.upload("admin/docs", "Show/S1/a.txt", b"one").await;
        api.upload("admin/docs", "Show/S1/b.txt", b"two").await;
        api.upload("admin/docs", "keep.txt", b"keep").await;
        let mv = |from: &str, to: &str| json!({"device": "admin/docs", "from": from, "to": to});

        // A folder moves with everything below it; a name in the way stops it before anything moves.
        let in_the_way = api.json(Method::POST, "/api/files/move", mv("Show", "keep.txt")).await.0;
        assert_eq!(in_the_way, StatusCode::CONFLICT, "a file where a folder should be");
        assert_eq!(
            api.json(Method::POST, "/api/files/move", mv("Show/S1/a.txt", "keep.txt")).await.0,
            StatusCode::CONFLICT
        );
        let (status, moved) = api.json(Method::POST, "/api/files/move", mv("Show", "Series/Show")).await;
        assert_eq!((status, moved["moved"].clone()), (StatusCode::OK, json!(2)));
        let (_, listing) = api.get("/api/files?device=admin/docs&path=Series/Show/S1").await;
        assert_eq!(listing["entries"].as_array().unwrap().len(), 2);
        let (_, old) = api.get("/api/files?device=admin/docs&path=Show&deleted=1").await;
        assert!(old["entries"].as_array().unwrap().is_empty(), "the old place is not a deleted folder");
        assert_eq!(
            api.json(Method::POST, "/api/files/move", mv("Series", "Series/inside")).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(api.json(Method::POST, "/api/files/move", mv("missing", "x")).await.0, StatusCode::NOT_FOUND);

        // The trash has nothing: a move is not a delete.
        assert_eq!(api.get("/api/trash").await.1["total"], 0);
        let (_, info) = api.get("/api/files/info?device=admin/docs&path=Series/Show/S1/a.txt").await;
        assert_eq!(info["events"][0]["action"], "rename");
        assert_eq!(info["events"][0]["detail"], "Show/S1/a.txt");

        // A computer's files are not moved from the web app.
        let (_, made) = api.json(Method::POST, "/api/devices", json!({"name": "laptop"})).await;
        assert!(made["token"].is_string());
        let (status, _) =
            api.json(Method::POST, "/api/files/move", json!({"device": "admin/laptop", "from": "a", "to": "b"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
    });
}
