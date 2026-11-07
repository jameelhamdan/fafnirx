use super::*;

fn count(kind: &str, status: &str, n: i64) -> jobs::Count {
    jobs::Count { kind: kind.into(), status: status.into(), n }
}

#[test]
fn process_missing_includes_videos_read_without_details() {
    use crate::platform::db::testing;
    let db = testing::db_with_device();
    let mut c = db.get().unwrap();
    for (path, root) in [("a/new.jpg", 1u8), ("a/seen.jpg", 2), ("a/blind.mp4", 3), ("a/known.mp4", 4)] {
        c.execute("INSERT INTO files (device_id, path) VALUES (1, $1)", &[&path]).unwrap();
        let version: i64 = c
            .query_one(
                "INSERT INTO versions (file_id, root_hash, size, mtime, created_at) SELECT id, $2, 1, 0, 0 FROM files WHERE path = $1 RETURNING id",
                &[&path, &vec![root; 32]],
            )
            .unwrap()
            .get(0);
        c.execute("UPDATE files SET current_version_id = $1 WHERE path = $2", &[&version, &path]).unwrap();
    }
    // The image and the video with details are read; the video that was read without any is not.
    for (root, kind, duration) in [(2u8, "image", None), (3, "video", None), (4, "video", Some(5000i64))] {
        c.execute(
            "INSERT INTO file_meta (root_hash, mime, kind, exif, read_at, duration_ms) VALUES ($1, 'x/y', $2, '{}', 0, $3)",
            &[&vec![root; 32], &kind, &duration],
        )
        .unwrap();
    }
    let mut missing: Vec<String> = jobs::media_to_read(&mut *c, false, None, 100)
        .unwrap()
        .into_iter()
        .map(|(_, _, path, _)| path)
        .collect();
    missing.sort();
    assert_eq!(missing, ["a/blind.mp4", "a/new.jpg"], "never read, and a video without details");
    assert_eq!(jobs::media_to_read(&mut *c, true, None, 100).unwrap().len(), 4, "everything");
    let first = jobs::media_to_read(&mut *c, true, None, 3).unwrap();
    let rest = jobs::media_to_read(&mut *c, true, first.last().map(|f| f.0), 3).unwrap();
    assert_eq!((first.len(), rest.len()), (3, 1), "in batches, each after the one before");
}

#[test]
fn counts_are_gathered_per_kind() {
    let s = summarize(vec![count("thumb", "queued", 4), count("meta", "failed", 1), count("thumb", "running", 2)]);
    assert_eq!(s.iter().map(|s| s.kind.as_str()).collect::<Vec<_>>(), ["meta", "thumb"]);
    assert_eq!((s[1].queued, s[1].running, s[1].failed, s[1].done), (4, 2, 0, 0));
    assert_eq!(s[0].failed, 1);
}

#[test]
fn the_same_work_is_queued_once_and_a_finished_job_stays_listed() {
    let db = crate::platform::db::testing::db_with_device();
    let mut c = db.get().unwrap();
    let c = &mut *c;
    let work = serde_json::json!({ "device": "master", "path": "a.mp4", "root": "00" });
    jobs::enqueue(c, JobKind::Thumb, &work).unwrap();
    jobs::enqueue(c, JobKind::Thumb, &work).unwrap();
    assert_eq!(jobs::items(c, true, -1, LIST).unwrap().len(), 1, "waiting work is not queued twice");

    let job = jobs::claim(c, 0).unwrap().unwrap();
    jobs::fail(c, job.id, true, 0, "no frame").unwrap();
    let listed = jobs::items(c, true, -1, LIST).unwrap();
    assert_eq!((listed[0].status.as_str(), listed[0].error.as_deref()), ("failed", Some("no frame")));
    assert_eq!(jobs::retry_failed(c, None).unwrap(), 1);

    let job = jobs::claim(c, 0).unwrap().unwrap();
    jobs::complete(c, job.id, Some("picture: poster.jpg")).unwrap();
    let summary = summarize(jobs::counts(c, true, -1).unwrap());
    assert_eq!((summary[0].queued, summary[0].failed, summary[0].done), (0, 0, 1));
    assert_eq!(jobs::items(c, false, -1, LIST).unwrap().len(), 0, "a user who sees no device of it does not see it");
    jobs::enqueue(c, JobKind::Thumb, &work).unwrap();
    assert_eq!(jobs::items(c, true, -1, LIST).unwrap().len(), 2, "once done, the work may be queued again");
    assert_eq!(jobs::purge_done(c).unwrap(), 0, "a job done just now is kept");
}
