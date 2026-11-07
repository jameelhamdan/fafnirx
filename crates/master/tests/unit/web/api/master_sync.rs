//! A computer syncing a folder of the master files through the device API: paths in its messages are below the folder.

use super::*;
use crate::devices::grpc::Service;
use fafnirx_common::pb::{change_event, collector_server::Collector, CommitRequest, DeleteRequest, DownloadFolderRequest, ListChangesRequest, Manifest, WatchFolderRequest};
use fafnirx_common::{chunk_hash, root_hash};
use tokio_stream::StreamExt;

fn asking<T>(msg: T, token: &str, folder: &str) -> tonic::Request<T> {
    let mut request = tonic::Request::new(msg);
    request.metadata_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());
    request.metadata_mut().insert("x-fafnirx-folder", folder.parse().unwrap());
    request
}

fn manifest(path: &str, data: &[u8]) -> Manifest {
    let chunk = chunk_hash(data);
    Manifest {
        path: path.into(),
        size: data.len() as u64,
        root_hash: root_hash(&[chunk], data.len() as u64).to_vec(),
        chunk_hashes: vec![chunk.to_vec()],
    }
}

#[test]
fn a_computer_syncs_a_master_folder_by_its_paths_below_it() {
    let mut api = Api::new();
    block(async {
        api.cookie = login(&api).await;
        master_folder(&api, "Music").await;
        master_folder(&api, "Other").await;
        api.upload("master:Music", "a.txt", b"one").await;
        api.upload("master:Music", "Rock/b.txt", b"two").await;
        api.upload("master:Other", "x.txt", b"outside").await;
        let (_, made) = api.json(Method::POST, "/api/devices", json!({"name": "laptop"})).await;
        let token = made["token"].as_str().unwrap().to_string();

        let (events, _) = tokio::sync::broadcast::channel(8);
        let service = Service { db: api.db.clone(), store: api.store.clone(), trusted: Default::default(), events };
        let folder = "master:Music";

        // It lists what is in the folder, named from it, and nothing of the rest of the master files.
        let stream = service
            .list_changes(asking(ListChangesRequest { folder: folder.into(), since: 0 }, &token, folder))
            .await
            .unwrap();
        let listed: Vec<_> = stream.into_inner().collect::<Vec<_>>().await;
        let mut paths: Vec<String> = listed
            .into_iter()
            .filter_map(|e| match e.unwrap().item {
                Some(change_event::Item::Change(c)) => Some(c.path),
                _ => None,
            })
            .collect();
        paths.sort();
        assert_eq!(paths, ["Rock/b.txt", "a.txt"]);

        // It reads a file by that path, and none outside.
        let download = |path: &str| {
            let request = asking(DownloadFolderRequest { folder: folder.into(), path: path.into() }, &token, folder);
            let service = &service;
            async move {
                match service.download_folder(request).await {
                    Ok(stream) => {
                        let parts: Vec<_> = stream.into_inner().collect::<Vec<_>>().await;
                        Ok(parts.into_iter().flat_map(|p| p.unwrap().data).collect::<Vec<u8>>())
                    }
                    Err(status) => Err(status.code()),
                }
            }
        };
        assert_eq!(download("a.txt").await.unwrap(), b"one");
        assert_eq!(download("Other/x.txt").await.unwrap_err(), tonic::Code::NotFound);

        // Content the master has outside the folder is not taken from there (that would tell what is outside): it must be sent.
        let elsewhere = CommitRequest { manifest: Some(manifest("copy.txt", b"outside")), mtime: 1, mode: 0, renamed_from: String::new() };
        assert_eq!(
            service.commit(asking(elsewhere, &token, folder)).await.unwrap_err().code(),
            tonic::Code::FailedPrecondition
        );

        // It writes below the folder: a file whose content the folder has already is taken from there.
        let new = manifest("new/c.txt", b"one");
        let committed = CommitRequest { manifest: Some(new), mtime: 1, mode: 0o644, renamed_from: String::new() };
        let version = service.commit(asking(committed, &token, folder)).await.unwrap().into_inner().version_id;
        assert!(version > 0);
        let (_, inside) = api.get("/api/files?device=master:Music&path=new").await;
        assert_eq!(inside["entries"][0]["name"], "c.txt", "{inside}");
        assert!(api._dir.path().join("files/Music/new/c.txt").is_file(), "below the folder, not at the root");
        assert!(!api._dir.path().join("files/new").exists());

        // A name that not every kind of computer can keep is refused, like in a shared folder.
        let bad = CommitRequest { manifest: Some(manifest("a:b.txt", b"one")), mtime: 1, mode: 0, renamed_from: String::new() };
        assert_eq!(service.commit(asking(bad, &token, folder)).await.unwrap_err().code(), tonic::Code::InvalidArgument);
        let clash = CommitRequest { manifest: Some(manifest("A.TXT", b"one")), mtime: 1, mode: 0, renamed_from: String::new() };
        assert_eq!(service.commit(asking(clash, &token, folder)).await.unwrap_err().code(), tonic::Code::AlreadyExists);

        // It deletes below the folder, and only there.
        service.mark_deleted(asking(DeleteRequest { path: "a.txt".into() }, &token, folder)).await.unwrap();
        let (_, left) = api.get("/api/files?device=master:Music").await;
        let names: Vec<_> = left["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap().to_string())
            .collect();
        assert!(!names.contains(&"a.txt".to_string()), "{names:?}");
        let (_, rest) = api.get("/api/files?device=master:Other").await;
        assert_eq!(rest["entries"][0]["name"], "x.txt", "the rest of the master files is untouched");

        // A folder can be named by its stable id, for calls and for the watch (a computer keeps the id).
        let (_, all) = api.get("/api/folders").await;
        let folders = master_folders_of(&all);
        let id = folders[0]["public_id"].as_str().unwrap().to_string();
        let by_id = service.list_changes(asking(ListChangesRequest { folder: id.clone(), since: 0 }, &token, &id)).await;
        assert!(by_id.is_ok(), "the id names the folder");
        let watch = WatchFolderRequest { folders: vec![id.clone(), "master:Nope".into()] };
        let mut told = service.watch_folder(asking(watch, &token, "")).await.unwrap().into_inner();
        assert_eq!(told.next().await.unwrap().unwrap().folder, id, "told about it under the name it was asked for");

        // Renaming it moves its files; a computer that holds its id keeps working, one that holds the old name is told it is gone.
        let renamed = api
            .json(Method::POST, &format!("/api/master-folders/{id}/rename"), json!({"path": "Tunes/Rock"}))
            .await;
        assert_eq!(renamed.0, StatusCode::OK, "{}", renamed.1);
        assert!(api._dir.path().join("files/Tunes/Rock/Rock/b.txt").is_file() && !api._dir.path().join("files/Music").exists());
        let after = service
            .list_changes(asking(ListChangesRequest { folder: id.clone(), since: 0 }, &token, &id))
            .await
            .unwrap();
        let count = after
            .into_inner()
            .filter_map(|e| match e.unwrap().item {
                Some(change_event::Item::Change(c)) if !c.deleted => Some(c.path),
                _ => None,
            })
            .collect::<Vec<_>>()
            .await
            .len();
        assert_eq!(count, 2, "Rock/b.txt and new/c.txt (a.txt was deleted above) are below the new path");
        let old = service
            .list_changes(asking(ListChangesRequest { folder: folder.into(), since: 0 }, &token, folder))
            .await;
        assert_eq!(old.err().map(|s| s.code()), Some(tonic::Code::NotFound));

        // A folder that is not one is not found, and a removed one stops working.
        let none = service
            .list_changes(asking(ListChangesRequest { folder: "master:Nope".into(), since: 0 }, &token, "master:Nope"))
            .await;
        assert_eq!(none.err().map(|s| s.code()), Some(tonic::Code::NotFound));
    });
}
