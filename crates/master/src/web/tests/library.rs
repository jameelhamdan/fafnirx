//! The photo library, albums and the streaming proxy.

use super::*;

#[test]
fn streaming_is_for_whoever_can_see_the_file_and_only_when_a_service_is_set_up() {
    // Not set up: nothing to offer, and nothing is passed on.
    let mut api = Api::new();
    block(async {
        assert_eq!(
            api.get("/api/stream/AAAAAAAAAAAAAAAAAAAAAA/master.m3u8").await.0,
            StatusCode::UNAUTHORIZED,
            "the login comes first"
        );
        api.cookie = login(&api).await;
        assert_eq!(api.get("/api/session").await.1["stream"], false);
        assert_eq!(api.get("/api/stream/AAAAAAAAAAAAAAAAAAAAAA/master.m3u8").await.0, StatusCode::NOT_FOUND);
    });

    // A stand-in for the streaming service: answers only with the right token, and says what it was asked for.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut api = Api::with_stream(Some(crate::library::stream::Backend::new(&url, "stream-token")));
    block(async {
        let service = Router::new().route(
            "/hls/*rest",
            axum::routing::get(
                |axum::extract::Path(rest): axum::extract::Path<String>,
                 axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
                 headers: axum::http::HeaderMap| async move {
                    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer stream-token") {
                        return (StatusCode::UNAUTHORIZED, "no".to_string()).into_response();
                    }
                    if rest.ends_with("seg9.ts") {
                        return (StatusCode::NOT_FOUND, "no such segment".to_string()).into_response();
                    }
                    let body = format!("#EXTM3U {rest} p={} h={}", q["p"], q["h"]);
                    ([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")], body).into_response()
                },
            ),
        );
        tokio::spawn(async move {
            axum::serve(tokio::net::TcpListener::from_std(listener).unwrap(), service).await.unwrap();
        });

        assert_eq!(api.get("/api/stream/AAAAAAAAAAAAAAAAAAAAAA/master.m3u8").await.0, StatusCode::UNAUTHORIZED);
        api.cookie = login(&api).await;
        assert_eq!(api.get("/api/session").await.1["stream"], true);
        api.json(Method::POST, "/api/devices", json!({"name": "videos", "kind": "local"})).await;
        api.upload("admin/videos", "trip/clip.mkv", b"video bytes").await;
        let id: uuid::Uuid = crate::platform::db::with(&api.db, |c| {
            Ok(c.query_one("SELECT id FROM files WHERE path = 'trip/clip.mkv'", &[])?.get(0))
        })
        .await
        .unwrap();
        let id = crate::platform::id::Id(id);

        let (status, headers, body) =
            api.send(Method::GET, &format!("/api/stream/{id}/720p/index.m3u8"), Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "application/vnd.apple.mpegurl");
        let text = String::from_utf8(body).unwrap();
        assert!(
            text.contains("720p/index.m3u8 p=users/admin/videos/trip/clip.mkv h="),
            "the service is told which file and content: {text}"
        );
        assert!(!text.contains("stream-token"));

        let (status, error) = api.get(&format!("/api/stream/{id}/720p/seg9.ts")).await;
        assert_eq!(
            (status, error["error"].as_str()),
            (StatusCode::NOT_FOUND, Some("no such segment")),
            "its answer is passed on"
        );
        for bad in ["../master.m3u8", "720p/seg1.mp4", "other"] {
            assert_eq!(api.get(&format!("/api/stream/{id}/{bad}")).await.0, StatusCode::NOT_FOUND, "{bad}");
        }
        assert_eq!(
            api.get("/api/stream/AAAAAAAAAAAAAAAAAAAAAQ/master.m3u8").await.0,
            StatusCode::NOT_FOUND,
            "no such file"
        );

        api.json(
            Method::POST,
            "/api/files/delete",
            json!({"items": [{"device": "admin/videos", "path": "trip/clip.mkv"}]}),
        )
        .await;
        assert_eq!(
            api.get(&format!("/api/stream/{id}/master.m3u8")).await.0,
            StatusCode::NOT_FOUND,
            "a deleted file is not streamed"
        );
    });
}

#[test]
fn the_library_groups_photos_by_month_and_keeps_what_people_say_about_them() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "photos", "kind": "local"})).await;
        api.upload("admin/photos", "trip/a.png", &png(8, 4, [200, 10, 10])).await;
        api.upload("admin/photos", "b.png", &png(4, 4, [0, 200, 0])).await;
        api.upload("admin/photos", "notes.txt", b"not a photo").await;

        // Only photos and videos are in the library; with no camera date, the file's own date is used.
        let (_, buckets) = api.get("/api/timeline/buckets").await;
        assert_eq!(buckets["total"], 2);
        let this_month = buckets["months"][0]["month"].as_str().unwrap().to_string();
        assert_eq!(buckets["months"][0]["days"], json!([2]), "both were added today: one day of two files");
        assert_eq!(api.get("/api/timeline").await.0, StatusCode::BAD_REQUEST, "a month is needed");
        assert_eq!(api.get("/api/timeline?month=soon").await.0, StatusCode::BAD_REQUEST);
        assert_eq!(api.get("/api/timeline/buckets?scope=bogus").await.0, StatusCode::BAD_REQUEST);
        let (_, listed) = api.get(&format!("/api/timeline?month={this_month}")).await;
        let names: Vec<_> = listed["assets"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"a.png") && names.contains(&"b.png"));
        assert_eq!(listed["assets"][0]["kind"], "image");
        assert_eq!(api.get("/api/timeline/buckets?kind=video").await.1["total"], 0);
        assert_eq!(api.get("/api/timeline/buckets?device=nowhere").await.1["total"], 0);

        let a = photo_id(&api, "a.png").await;
        let b = photo_id(&api, "b.png").await;
        let edit = |id: &str, body: Value| {
            let (api, id) = (&api, id.to_string());
            async move { api.json(Method::PATCH, &format!("/api/library/{id}"), body).await }
        };

        // Favorite, rating, description and tags (case does not make a new tag).
        let (status, _) = edit(
            &a,
            json!({"favorite": true, "rating": 4, "description": " Beach day ", "tags": ["Trip", " trip ", "Summer", ""]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (_, detail) = api.get(&format!("/api/library/{a}")).await;
        assert_eq!((detail["favorite"].clone(), detail["rating"].clone()), (json!(true), json!(4)));
        assert_eq!(detail["description"], "Beach day");
        let tags: Vec<_> = detail["tags"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(tags, ["Summer", "Trip"]);
        assert_eq!(detail["mime"], "image/png");
        assert_eq!(api.get("/api/timeline/buckets?scope=favorites").await.1["total"], 1);
        assert_eq!(edit(&a, json!({"rating": 9})).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(edit("AAAAAAAAAAAAAAAAAAAAAA", json!({"favorite": true})).await.0, StatusCode::NOT_FOUND);
        edit(&a, json!({"rating": null})).await;
        let detail = api.get(&format!("/api/library/{a}")).await.1;
        assert!(detail["rating"].is_null(), "null clears");
        assert_eq!(detail["favorite"], true, "an absent field leaves things alone");

        let (_, all_tags) = api.get("/api/tags").await;
        assert_eq!(all_tags.as_array().unwrap().len(), 2);
        let trip = all_tags.as_array().unwrap().iter().find(|t| t["name"] == "Trip").unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(api.get(&format!("/api/timeline/buckets?tag={trip}")).await.1["total"], 1);
        // Dropping a tag from the only file that has it removes it.
        edit(&a, json!({"tags": ["Summer"]})).await;
        assert_eq!(api.get("/api/tags").await.1.as_array().unwrap().len(), 1);

        // Bulk: favorite both, then archive one: the timeline no longer shows it, the archive does.
        let (_, changed) = api
            .json(Method::POST, "/api/library/bulk", json!({"ids": [a, b], "favorite": true, "add_tags": ["Bulk"]}))
            .await;
        assert_eq!(changed["changed"], 2);
        assert_eq!(api.get("/api/timeline/buckets?scope=favorites").await.1["total"], 2);
        edit(&a, json!({"archived": true})).await;
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 1);
        assert_eq!(api.get("/api/timeline/buckets?scope=archive").await.1["total"], 1);
        assert_eq!(api.json(Method::POST, "/api/library/bulk", json!({"ids": []})).await.0, StatusCode::BAD_REQUEST);
        edit(&a, json!({"archived": false})).await;

        // A corrected date moves the photo to another month and can be taken back.
        edit(&a, json!({"taken_at": 1_700_000_000})).await;
        let (_, moved) = api.get("/api/timeline?month=2023-11").await;
        assert_eq!(moved["assets"][0]["when"], 1_700_000_000);
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["when_edited"], true);
        assert_eq!(api.get("/api/timeline/buckets").await.1["months"].as_array().unwrap().len(), 2);
        edit(&a, json!({"taken_at": null})).await;
        assert_eq!(api.get("/api/timeline/buckets").await.1["months"].as_array().unwrap().len(), 1);

        // A corrected place shows on the map; nonsense places and the "null island" are refused.
        assert_eq!(api.get("/api/map").await.1["points"].as_array().unwrap().len(), 0);
        let place = |lat: f64, lon: f64| json!({"location": {"lat": lat, "lon": lon}});
        assert_eq!(edit(&a, place(95.0, 0.0)).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(edit(&a, place(0.0, 0.0)).await.0, StatusCode::BAD_REQUEST);
        edit(&a, place(52.5, 13.4)).await;
        let (_, map) = api.get("/api/map").await;
        assert_eq!(json!([map["points"][0][0], map["points"][0][1], map["points"][0][2]]), json!([a, 52.5, 13.4]));
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["place_edited"], true);
        edit(&a, json!({"location": null})).await;
        assert_eq!(api.get("/api/map").await.1["points"].as_array().unwrap().len(), 0);

        // Pictures and originals by id.
        let (status, headers, thumb) =
            api.send(Method::GET, &format!("/api/media/{a}/thumb"), Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(image::load_from_memory(&thumb).unwrap().width(), 8);
        let (_, _, original) =
            api.send(Method::GET, &format!("/api/media/{a}/original"), Body::empty(), None, &[]).await;
        assert_eq!(original, png(8, 4, [200, 10, 10]));
        let bogus = api.send(Method::GET, &format!("/api/media/{a}/bogus"), Body::empty(), None, &[]).await;
        assert_eq!(bogus.0, StatusCode::NOT_FOUND);

        // Deleting moves it to the trash (its picture still works there); restoring brings it back with its state.
        api.json(
            Method::POST,
            "/api/files/delete",
            json!({"items": [{"device": "admin/photos", "path": "trip/a.png"}]}),
        )
        .await;
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 1);
        let (_, trash) = api.get("/api/trash").await;
        assert_eq!(trash["total"], 1);
        assert_eq!(trash["entries"][0]["name"], "a.png");
        assert_eq!(trash["entries"][0]["kind"], "image");
        assert!(
            trash["entries"][0]["purge_at"].as_i64().unwrap() > trash["entries"][0]["deleted_at"].as_i64().unwrap()
        );
        let in_trash = api.send(Method::GET, &format!("/api/media/{a}/thumb"), Body::empty(), None, &[]).await;
        assert_eq!(in_trash.0, StatusCode::OK);
        assert_eq!(edit(&a, json!({"favorite": false})).await.0, StatusCode::NOT_FOUND);
        let version = trash["entries"][0]["version"].clone();
        let (status, _) = api
            .json(
                Method::POST,
                "/api/files/restore",
                json!({"device": "admin/photos", "path": "trip/a.png", "version": version}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(api.get("/api/trash").await.1["total"], 0);
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 2);
        assert_eq!(
            api.get(&format!("/api/library/{a}")).await.1["favorite"],
            true,
            "what was said comes back with the file"
        );
    });
}

#[test]
fn a_folder_can_be_an_album_and_files_are_added_from_the_file_browser() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "tv", "kind": "local"})).await;
        for (path, shade) in [("Show/S1/Episode 10.png", 1), ("Show/S1/Episode 2.png", 2), ("Other/x.png", 3)] {
            api.upload("admin/tv", path, &png(4, 4, [shade, shade, shade])).await;
        }
        let folder = json!({"device": "admin/tv", "path": "Show"});

        // The folder is the album: its name is the folder's, and what is below it is in it, in the order of the names.
        let (status, album) = api.json(Method::POST, "/api/albums", json!({"name": "", "folder": folder})).await;
        assert_eq!(status, StatusCode::OK, "{album}");
        let id = album["id"].as_str().unwrap().to_string();
        assert_eq!(
            (album["name"].clone(), album["count"].clone(), album["folder"].clone()),
            (json!("Show"), json!(2), folder.clone())
        );
        let names = |detail: &Value| -> Vec<String> {
            detail["assets"].as_array().unwrap().iter().map(|a| a["path"].as_str().unwrap().to_string()).collect()
        };
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(
            names(&detail),
            ["Show/S1/Episode 2.png", "Show/S1/Episode 10.png"],
            "Episode 2 comes before Episode 10"
        );

        // Asking again gives the same album, and a new file in the folder is in it without doing anything.
        let (_, again) = api.json(Method::POST, "/api/albums", json!({"name": "", "folder": folder})).await;
        assert_eq!(again["id"].as_str(), Some(id.as_str()));
        api.upload("admin/tv", "Show/S1/Episode 3.png", &png(4, 4, [9, 9, 9])).await;
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(
            names(&detail),
            ["Show/S1/Episode 2.png", "Show/S1/Episode 3.png", "Show/S1/Episode 10.png"],
            "a file that comes later is in the album"
        );
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 3);

        // A folder that is not there for this user cannot be an album.
        let nobody = json!({"device": "nobody/none", "path": ""});
        assert_eq!(
            api.json(Method::POST, "/api/albums", json!({"name": "x", "folder": nobody})).await.0,
            StatusCode::NOT_FOUND
        );

        // From the file browser: files and folders by where they are, without ids.
        let (_, trip) = api.json(Method::POST, "/api/albums", json!({"name": "Trip"})).await;
        let trip_id = trip["id"].as_str().unwrap().to_string();
        let items =
            json!({"items": [{"device": "admin/tv", "path": "Other/x.png"}, {"device": "admin/tv", "path": "Show"}]});
        let (_, added) = api.json(Method::POST, &format!("/api/albums/{trip_id}/files"), items).await;
        assert_eq!(added["count"], 4, "the file, and the three below the folder");
        let (_, detail) = api.get(&format!("/api/albums/{trip_id}")).await;
        assert_eq!(detail["album"]["count"], 4);
        assert!(detail["album"]["folder"].is_null());
    });
}

#[test]
fn albums_collect_files_and_leave_the_files_alone() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "photos", "kind": "local"})).await;
        api.upload("admin/photos", "a.png", &png(8, 4, [1, 1, 1])).await;
        api.upload("admin/photos", "b.png", &png(4, 4, [2, 2, 2])).await;
        api.upload("admin/photos", "c.png", &png(2, 2, [3, 3, 3])).await;
        let (a, b, c) = (photo_id(&api, "a.png").await, photo_id(&api, "b.png").await, photo_id(&api, "c.png").await);

        assert_eq!(api.json(Method::POST, "/api/albums", json!({"name": "  "})).await.0, StatusCode::BAD_REQUEST);
        let (status, album) = api.json(Method::POST, "/api/albums", json!({"name": " Trip ", "ids": [a]})).await;
        assert_eq!(status, StatusCode::OK);
        let id = album["id"].as_str().unwrap().to_string();
        assert_eq!(
            (album["name"].clone(), album["count"].clone(), album["cover"].clone()),
            (json!("Trip"), json!(1), json!(a))
        );

        let (_, added) = api.json(Method::POST, &format!("/api/albums/{id}/files"), json!({"ids": [a, b]})).await;
        assert_eq!(added["count"], 1, "a is already in it");
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(detail["album"]["count"], 2);
        assert_eq!(detail["assets"].as_array().unwrap().len(), 2);
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["albums"][0]["name"], "Trip");

        let not_member = api.json(Method::PATCH, &format!("/api/albums/{id}"), json!({"cover": c})).await;
        assert_eq!(not_member.0, StatusCode::BAD_REQUEST, "c is not in it");
        let (_, edited) = api
            .json(
                Method::PATCH,
                &format!("/api/albums/{id}"),
                json!({"name": "Trip 2024", "cover": b, "description": "x"}),
            )
            .await;
        assert_eq!((edited["name"].clone(), edited["cover"].clone()), (json!("Trip 2024"), json!(b)));

        let (_, removed) = api.json(Method::POST, &format!("/api/albums/{id}/files/remove"), json!({"ids": [b]})).await;
        assert_eq!(removed["count"], 1);
        let (_, after) = api.get("/api/albums").await;
        assert_eq!(after[0]["count"], 1);
        assert_eq!(after[0]["cover"], a, "the cover that left is replaced by the newest file");

        // Deleting a file takes it out of the album's count; deleting the album leaves the photos.
        api.json(Method::POST, "/api/files/delete", json!({"items": [{"device": "admin/photos", "path": "a.png"}]}))
            .await;
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 0);
        let version = api.get("/api/trash").await.1["entries"][0]["version"].clone();
        let restored = api
            .json(
                Method::POST,
                "/api/files/restore",
                json!({"device": "admin/photos", "path": "a.png", "version": version}),
            )
            .await;
        assert_eq!(restored.0, StatusCode::OK);
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 1, "and it is back in the album when restored");
        assert_eq!(
            api.send(Method::DELETE, &format!("/api/albums/{id}"), Body::empty(), None, &[]).await.0,
            StatusCode::OK
        );
        assert_eq!(api.get(&format!("/api/albums/{id}")).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 3);
    });
}

#[test]
fn an_album_uses_its_poster_as_the_cover_and_a_viewer_steps_through_a_folder() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "tv", "kind": "local"})).await;
        for (path, shade) in
            [("Show/S1/E1.png", 1), ("Show/S1/E2.png", 2), ("Show/poster.png", 3), ("Show/S1/cover.png", 4)]
        {
            api.upload("admin/tv", path, &png(4, 4, [shade, shade, shade])).await;
        }
        let poster = photo_id(&api, "poster.png").await;
        let (_, album) = api
            .json(Method::POST, "/api/albums", json!({"name": "", "folder": {"device": "admin/tv", "path": "Show"}}))
            .await;
        assert_eq!(album["cover"], poster.as_str(), "the poster nearest the top of the folder is the cover");
        let id = album["id"].as_str().unwrap().to_string();

        // A cover that was chosen wins over the poster.
        let e1 = photo_id(&api, "E1.png").await;
        api.json(Method::PATCH, &format!("/api/albums/{id}"), json!({"cover": e1})).await;
        assert_eq!(api.get(&format!("/api/albums/{id}")).await.1["album"]["cover"], e1.as_str());

        // The viewer of a file lists the pictures of its folder in the order of their names.
        let (status, folder) = api.get(&format!("/api/library/{e1}/folder")).await;
        assert_eq!(status, StatusCode::OK);
        let names: Vec<&str> =
            folder["assets"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["cover.png", "E1.png", "E2.png"]);
    });
}

#[test]
fn the_folder_of_a_viewer_takes_wildcards_in_names_literally() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/devices", json!({"name": "pics", "kind": "local"})).await;
        for (path, shade) in [("a_b/one.png", 1), ("axb/two.png", 2), ("a_b/sub/three.png", 3)] {
            api.upload("admin/pics", path, &png(4, 4, [shade, shade, shade])).await;
        }
        let one = photo_id(&api, "one.png").await;
        let (_, folder) = api.get(&format!("/api/library/{one}/folder")).await;
        let names: Vec<&str> =
            folder["assets"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["one.png"], "neither axb/ nor the subfolder");
    });
}
