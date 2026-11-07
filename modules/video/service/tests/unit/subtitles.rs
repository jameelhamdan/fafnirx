use super::sidecar_subtitles;

#[test]
fn subtitles_are_found_in_a_crowded_folder_and_in_subs() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..300 {
        std::fs::write(dir.path().join(format!("photo{i}.jpg")), b"").unwrap();
    }
    std::fs::write(dir.path().join("Show - 01.mkv"), b"").unwrap();
    std::fs::write(dir.path().join("Show - 01.eng.srt"), b"").unwrap();
    std::fs::create_dir(dir.path().join("Subs")).unwrap();
    std::fs::write(dir.path().join("Subs").join("Show - 01.jpn.ass"), b"").unwrap();
    let video = crate::service::Video {
        name: "Show - 01.mkv".into(),
        source: dir.path().join("Show - 01.mkv"),
        dir: dir.path().join("cache"),
    };
    let found: Vec<String> = sidecar_subtitles(&video).into_iter().map(|s| s.file).collect();
    assert_eq!(found, ["Show - 01.eng.srt", "Subs/Show - 01.jpn.ass"]);
}
