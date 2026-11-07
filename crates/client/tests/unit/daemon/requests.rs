use super::*;

fn config_in(dir: &Path) -> PathBuf {
    let path = dir.join("config.toml");
    std::fs::write(&path, "server = \"http://master:7443\"\n").unwrap();
    path
}

#[test]
fn a_user_changes_only_what_is_theirs() {
    assert!(may_change(Some(501), Some(501)));
    assert!(!may_change(Some(501), Some(502)), "another user's folder");
    assert!(!may_change(Some(501), None), "a folder that is not there");
    assert!(!may_change(None, None), "an owner that cannot be told is not trusted");
    assert!(may_change(Some(0), Some(502)), "root may change anything");
}

#[test]
fn a_request_is_the_command_it_stands_for() {
    let words = |r: Request| r.args().iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>();
    let add = Request::AddFolder { paths: vec!["C:/a b".into(), "D:/c".into()] };
    assert_eq!(words(add), ["folder", "add", "C:/a b", "D:/c"], "one command for all the folders");
    assert_eq!(words(Request::RemoveFolder { name: "docs".into() }), ["folder", "remove", "docs"]);
    let attach = Request::AttachSynced { name: "ana/family".into(), path: "/f".into(), create: true };
    assert_eq!(words(attach), ["folder", "add", "/f", "--target", "ana/family", "--create"]);
    let set = Request::SetMode { name: "family".into(), mode: crate::config::Mode::Keep };
    assert_eq!(words(set), ["folder", "mode", "family", "keep"]);
    let one = Request::Resolve { name: "family".into(), keep: "both".into(), path: Some("a b.txt".into()) };
    assert_eq!(words(one), ["folder", "resolve", "family", "both", "--", "a b.txt"]);
    let all = Request::Resolve { name: "family".into(), keep: "theirs".into(), path: None };
    assert_eq!(words(all), ["folder", "resolve", "family", "theirs"]);
}

#[test]
fn a_request_survives_the_file() {
    let request = Request::AttachSynced { name: "docs".into(), path: "/a b".into(), create: false };
    let text = serde_json::to_string(&request).unwrap();
    assert!(text.contains(r#""op":"attach-synced""#), "{text}");
    assert_eq!(serde_json::from_str::<Request>(&text).unwrap(), request);
    assert!(
        serde_json::from_str::<Request>(r#"{"op":"enroll"}"#).is_err(),
        "only folder changes can be asked for"
    );
}

#[test]
fn answers_are_ok_or_an_error_with_the_text() {
    assert_eq!(parse_answer("ok\n2 folders added\n").unwrap(), "2 folders added");
    assert_eq!(parse_answer("error\nnot yours\n").unwrap_err().to_string(), "not yours");
    assert!(parse_answer("").is_err());
}

#[tokio::test]
async fn folders_are_added_and_removed_for_their_owner_only() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_in(dir.path());
    let (docs, other) = (dir.path().join("docs"), dir.path().join("other"));
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    let only_docs = |p: &Path| p.ends_with("docs");

    let add = |paths: Vec<PathBuf>| Request::AddFolder { paths };
    apply(&config, add(vec![docs.clone()]), &only_docs).await.unwrap();
    assert_eq!(Config::load(&config).unwrap().folders[0].name, "docs");

    let e = apply(&config, add(vec![other.clone()]), &only_docs).await.unwrap_err();
    assert!(format!("{e:#}").contains("is not yours"), "{e:#}");
    assert_eq!(Config::load(&config).unwrap().folders.len(), 1, "the folder of another user is not collected");

    let remove = Request::RemoveFolder { name: "docs".into() };
    let e = apply(&config, remove, &|_| false).await.unwrap_err();
    assert!(format!("{e:#}").contains("is not yours"), "{e:#}");
    assert_eq!(Config::load(&config).unwrap().folders.len(), 1, "nor is it removed by someone else");

    apply(&config, Request::RemoveFolder { name: "docs".into() }, &only_docs).await.unwrap();
    assert!(Config::load(&config).unwrap().folders.is_empty());
}

#[tokio::test]
async fn a_request_file_is_answered_and_taken_away() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_in(dir.path());
    let control = dir.path().join("control");
    std::fs::create_dir_all(&control).unwrap();

    std::fs::write(request_file(&control, "1-2"), r#"{"op":"remove-folder","name":"nothing"}"#).unwrap();
    assert!(!serve_one(&config, &control, "request.1-2").await, "no such folder");
    assert!(!request_file(&control, "1-2").exists(), "taken, so it is not answered twice");
    let text = std::fs::read_to_string(answer_file(&control, "1-2")).unwrap();
    assert!(parse_answer(&text).unwrap_err().to_string().contains("no folder named"), "{text}");

    std::fs::write(request_file(&control, "3-4"), "not json").unwrap();
    assert!(!serve_one(&config, &control, "request.3-4").await);
    assert!(std::fs::read_to_string(answer_file(&control, "3-4")).unwrap().starts_with("error\nnot a request"));
}
