//! The photo library, albums and the modules.

use super::*;
use fafnirx_common::LockExt;

#[test]
fn the_library_groups_photos_by_month_and_keeps_what_people_say_about_them() {
    let _on = crate::library::meta::THUMBNAILS_TEST.locked();
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "photos"})).await;
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
        assert_eq!(api.get("/api/timeline/buckets?folder=nowhere").await.1["total"], 0);

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

        // A corrected place shows in the details; nonsense places and the "null island" are refused.
        assert!(api.get(&format!("/api/library/{a}")).await.1["lat"].is_null());
        let place = |lat: f64, lon: f64| json!({"location": {"lat": lat, "lon": lon}});
        assert_eq!(edit(&a, place(95.0, 0.0)).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(edit(&a, place(0.0, 0.0)).await.0, StatusCode::BAD_REQUEST);
        edit(&a, place(52.5, 13.4)).await;
        let detail = api.get(&format!("/api/library/{a}")).await.1;
        assert_eq!(
            (detail["lat"].as_f64(), detail["lon"].as_f64(), &detail["place_edited"]),
            (Some(52.5), Some(13.4), &json!(true))
        );
        edit(&a, json!({"location": null})).await;
        assert!(api.get(&format!("/api/library/{a}")).await.1["lat"].is_null());

        // Pictures and originals by id. A picture is made when it is asked for: the first answer is "accepted", and once the
        // job is done it is there.
        let thumb_uri = format!("/api/media/{a}/thumb");
        let first = api.send(Method::GET, &thumb_uri, Body::empty(), &[]).await;
        assert_eq!(first.0, StatusCode::ACCEPTED);
        assert_eq!(first.1[header::CACHE_CONTROL], "no-store", "the answer must not be kept");
        assert_eq!(api.send(Method::GET, &thumb_uri, Body::empty(), &[]).await.0, StatusCode::ACCEPTED);
        assert_eq!(api.picture_jobs().await, 1, "asking again does not queue it again");
        api.make_pictures().await;
        let (status, headers, thumb) = api.send(Method::GET, &thumb_uri, Body::empty(), &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(image::load_from_memory(&thumb).unwrap().width(), 8);
        let (_, _, original) = api.send(Method::GET, &format!("/api/media/{a}/original"), Body::empty(), &[]).await;
        assert_eq!(original, png(8, 4, [200, 10, 10]));
        let bogus = api.send(Method::GET, &format!("/api/media/{a}/bogus"), Body::empty(), &[]).await;
        assert_eq!(bogus.0, StatusCode::NOT_FOUND);

        // Deleting moves it to the trash (its picture still works there); restoring brings it back with its state.
        api.json(
            Method::POST,
            "/api/files/delete",
            json!({"items": [{"folder": "admin/photos", "path": "trip/a.png"}]}),
        )
        .await;
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 1);
        let (_, trash) = api.get("/api/trash").await;
        assert_eq!(trash["total"], 1);
        assert_eq!(trash["entries"][0]["name"], "a.png");
        assert_eq!(trash["entries"][0]["kind"], "image");
        assert!(trash["entries"][0]["purge_at"].as_i64().unwrap() > trash["entries"][0]["deleted_at"].as_i64().unwrap());
        let in_trash = api.send(Method::GET, &format!("/api/media/{a}/thumb"), Body::empty(), &[]).await;
        assert_eq!(in_trash.0, StatusCode::OK);
        assert_eq!(edit(&a, json!({"favorite": false})).await.0, StatusCode::NOT_FOUND);
        let version = trash["entries"][0]["version"].clone();
        let (status, _) = api
            .json(
                Method::POST,
                "/api/files/restore",
                json!({"folder": "admin/photos", "path": "trip/a.png", "version": version}),
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

        // Removing from the trash for good takes the file, its versions and its state; what is not in the trash is refused.
        let delete = json!({"items": [{"folder": "admin/photos", "path": "trip/a.png"}]});
        api.json(Method::POST, "/api/files/delete", delete).await;
        let (_, trash) = api.get("/api/trash").await;
        let purge = json!({"ids": [trash["entries"][0]["id"].clone()]});
        let (status, purged) = api.json(Method::POST, "/api/trash/purge", purge.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(purged["files"], 1);
        assert!(purged["bytes"].as_i64().unwrap() > 0);
        assert_eq!(api.get("/api/trash").await.1["total"], 0);
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 1);
        assert_eq!(api.json(Method::POST, "/api/trash/purge", purge).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.json(Method::POST, "/api/trash/purge", json!({"ids": []})).await.0, StatusCode::BAD_REQUEST);
    });
}

#[test]
fn the_folder_of_a_viewer_takes_wildcards_in_names_literally() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "pics"})).await;
        for (path, shade) in [("a_b/one.png", 1), ("axb/two.png", 2), ("a_b/sub/three.png", 3)] {
            api.upload("admin/pics", path, &png(4, 4, [shade, shade, shade])).await;
        }
        let one = photo_id(&api, "one.png").await;
        let (_, folder) = api.get(&format!("/api/library/{one}/folder")).await;
        let names: Vec<&str> = folder["assets"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["one.png"], "neither axb/ nor the subfolder");
    });
}

#[test]
fn the_photos_switch_is_told_to_everybody_and_changes_nothing_else() {
    // Saving the settings also sets the thumbnails flag, which another test changes.
    let _on = crate::library::meta::THUMBNAILS_TEST.locked();
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        assert_eq!(api.get("/api/session").await.1["photos"], true, "on until it is turned off");
        assert_eq!(api.get("/api/settings").await.1["photos"], true);

        let body = |photos: bool| json!({"version_count": 5, "retention_days": 30, "thumbnails": true, "photos": photos});
        assert_eq!(api.json(Method::PUT, "/api/settings", body(false)).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/settings").await.1["photos"], false);
        assert_eq!(api.get("/api/session").await.1["photos"], false);
        // Only the web app hides the pages: the library is still served.
        assert_eq!(api.get("/api/timeline/buckets").await.0, StatusCode::OK);

        // A body without the field leaves it as it is: a caller that does not know the switch cannot turn it on.
        let old = json!({"version_count": 5, "retention_days": 30, "thumbnails": true});
        assert_eq!(api.json(Method::PUT, "/api/settings", old).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/session").await.1["photos"], false);
        assert_eq!(api.json(Method::PUT, "/api/settings", body(true)).await.0, StatusCode::OK);
        assert_eq!(api.get("/api/session").await.1["photos"], true);

        // Logged out: nothing is told.
        api.cookie = String::new();
        assert!(api.get("/api/session").await.1.get("photos").is_none());
    });
}

#[test]
fn nothing_is_made_when_thumbnails_are_off() {
    let _on = crate::library::meta::THUMBNAILS_TEST.locked();
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "photos"})).await;
        api.upload("admin/photos", "a.png", &png(8, 4, [200, 10, 10])).await;
        let a = photo_id(&api, "a.png").await;
        let settings = |on: bool| json!({"version_count": 5, "retention_days": 30, "thumbnails": on});
        let uri = format!("/api/media/{a}/thumb");

        // Off: no job, no picture, and the file is not offered a preview.
        assert_eq!(api.json(Method::PUT, "/api/settings", settings(false)).await.0, StatusCode::OK);
        assert_eq!(api.send(Method::GET, &uri, Body::empty(), &[]).await.0, StatusCode::NOT_FOUND);
        assert_eq!(
            api.send(Method::GET, &format!("/api/media/{a}/preview"), Body::empty(), &[]).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(api.picture_jobs().await, 0, "not even on demand");
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["preview"], false);
        assert_eq!(api.get("/api/settings").await.1["thumbnails"], false);
        // The original is still there.
        assert_eq!(
            api.send(Method::GET, &format!("/api/media/{a}/original"), Body::empty(), &[]).await.0,
            StatusCode::OK
        );

        // On again: asked for, made, and sent.
        assert_eq!(api.json(Method::PUT, "/api/settings", settings(true)).await.0, StatusCode::OK);
        assert_eq!(api.send(Method::GET, &uri, Body::empty(), &[]).await.0, StatusCode::ACCEPTED);
        api.make_pictures().await;
        assert_eq!(api.send(Method::GET, &uri, Body::empty(), &[]).await.0, StatusCode::OK);
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["preview"], true);
        // Off hides what was made as well: nothing is sent while the setting says no.
        assert_eq!(api.json(Method::PUT, "/api/settings", settings(false)).await.0, StatusCode::OK);
        assert_eq!(api.send(Method::GET, &uri, Body::empty(), &[]).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.json(Method::PUT, "/api/settings", settings(true)).await.0, StatusCode::OK);
    });
}
