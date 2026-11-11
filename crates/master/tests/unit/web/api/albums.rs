//! Albums: a folder is an album, shown with its poster, stepped through by a viewer.

use super::*;

#[test]
fn a_folder_is_an_album_and_what_comes_into_it_is_in_the_album() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "show"})).await;
        api.json(Method::POST, "/api/folders", json!({"name": "other"})).await;
        for (path, shade) in [("S1/Episode 10.png", 1), ("S1/Episode 2.png", 2)] {
            api.upload("admin/show", path, &png(4, 4, [shade, shade, shade])).await;
        }
        api.upload("admin/other", "x.png", &png(4, 4, [3, 3, 3])).await;
        let folder = json!({"device": "admin/show", "path": ""});

        // The folder is the album: its name is the folder's, and what is below it is in it, in the order of the names.
        let (status, album) = api.json(Method::POST, "/api/albums", json!({"name": "", "folder": folder})).await;
        assert_eq!(status, StatusCode::OK, "{album}");
        let id = album["id"].as_str().unwrap().to_string();
        assert_eq!(
            (album["name"].clone(), album["count"].clone(), album["folder"].clone()),
            (json!("show"), json!(2), folder.clone())
        );
        let names = |detail: &Value| -> Vec<String> {
            detail["assets"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a["path"].as_str().unwrap().to_string())
                .collect()
        };
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(names(&detail), ["S1/Episode 2.png", "S1/Episode 10.png"], "Episode 2 comes before Episode 10");
        assert_eq!(detail["more"], false, "all of it is in the answer");

        // Asking again gives the same album, and a new file in the folder is in it without doing anything.
        let (_, again) = api.json(Method::POST, "/api/albums", json!({"name": "", "folder": folder})).await;
        assert_eq!(again["id"].as_str(), Some(id.as_str()));
        api.upload("admin/show", "S1/Episode 3.png", &png(4, 4, [9, 9, 9])).await;
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(
            names(&detail),
            ["S1/Episode 2.png", "S1/Episode 3.png", "S1/Episode 10.png"],
            "a file that comes later is in the album"
        );
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 3);

        // A folder that is not there for this user cannot be an album.
        let nobody = json!({"device": "nobody/none", "path": ""});
        assert_eq!(
            api.json(Method::POST, "/api/albums", json!({"name": "x", "folder": nobody})).await.0,
            StatusCode::NOT_FOUND
        );
    });
}

#[test]
fn an_album_shows_its_folder_and_leaves_the_files_alone() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "photos"})).await;
        api.upload("admin/photos", "a.png", &png(8, 4, [1, 1, 1])).await;
        api.upload("admin/photos", "b.png", &png(4, 4, [2, 2, 2])).await;
        api.json(Method::POST, "/api/folders", json!({"name": "other"})).await;
        api.upload("admin/other", "c.png", &png(2, 2, [3, 3, 3])).await;
        let (a, b) = (photo_id(&api, "a.png").await, photo_id(&api, "b.png").await);
        let whole = json!({"device": "admin/photos", "path": ""});

        // An album needs a folder.
        assert_eq!(
            api.json(Method::POST, "/api/albums", json!({"name": "Trip"})).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let (status, album) = api.json(Method::POST, "/api/albums", json!({"name": " Trip ", "folder": whole})).await;
        assert_eq!(status, StatusCode::OK, "{album}");
        let id = album["id"].as_str().unwrap().to_string();
        assert_eq!((album["name"].clone(), album["count"].clone()), (json!("Trip"), json!(2)));
        let (_, detail) = api.get(&format!("/api/albums/{id}")).await;
        assert_eq!(detail["assets"].as_array().unwrap().len(), 2);
        assert_eq!(api.get(&format!("/api/library/{a}")).await.1["albums"][0]["name"], "Trip");

        // The cover is a file of the album, or chosen again.
        let c = photo_id(&api, "c.png").await;
        let outside = api.json(Method::PATCH, &format!("/api/albums/{id}"), json!({"cover": c})).await;
        assert_eq!(outside.0, StatusCode::BAD_REQUEST, "c is not in the folder");
        let (_, edited) = api
            .json(
                Method::PATCH,
                &format!("/api/albums/{id}"),
                json!({"name": "Trip 2024", "cover": b, "description": "x"}),
            )
            .await;
        assert_eq!((edited["name"].clone(), edited["cover"].clone()), (json!("Trip 2024"), json!(b)));

        // Deleting a file takes it out of the album's count; restoring puts it back; deleting the album leaves the photos.
        api.json(Method::POST, "/api/files/delete", json!({"items": [{"device": "admin/photos", "path": "a.png"}]}))
            .await;
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 1);
        let version = api.get("/api/trash").await.1["entries"][0]["version"].clone();
        let restored = api
            .json(
                Method::POST,
                "/api/files/restore",
                json!({"device": "admin/photos", "path": "a.png", "version": version}),
            )
            .await;
        assert_eq!(restored.0, StatusCode::OK);
        assert_eq!(api.get("/api/albums").await.1[0]["count"], 2, "and it is back in the album when restored");
        assert_eq!(api.send(Method::DELETE, &format!("/api/albums/{id}"), Body::empty(), &[]).await.0, StatusCode::OK);
        assert_eq!(api.get(&format!("/api/albums/{id}")).await.0, StatusCode::NOT_FOUND);
        assert_eq!(api.get("/api/timeline/buckets").await.1["total"], 3);
    });
}

#[test]
fn an_album_uses_its_poster_as_the_cover_and_a_viewer_steps_through_a_folder() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        api.json(Method::POST, "/api/folders", json!({"name": "show"})).await;
        for (path, shade) in [("S1/E1.png", 1), ("S1/E2.png", 2), ("poster.png", 3), ("S1/cover.png", 4)] {
            api.upload("admin/show", path, &png(4, 4, [shade, shade, shade])).await;
        }
        let poster = photo_id(&api, "poster.png").await;
        let (_, album) = api
            .json(Method::POST, "/api/albums", json!({"name": "", "folder": {"device": "admin/show", "path": ""}}))
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
        let names: Vec<&str> = folder["assets"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["cover.png", "E1.png", "E2.png"]);
    });
}
