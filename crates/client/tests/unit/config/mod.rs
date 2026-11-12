use super::*;

fn add(cfg: &Path, dir: &Path, name: Option<&str>) -> Result<Folder> {
    add_folder_to_file(cfg, NewFolder { dir, target: Target::Device, name, id: None, mode: Mode::Both })
}

fn attach(cfg: &Path, name: &str, id: Option<&str>, dir: &Path, mode: Mode) -> Result<Folder> {
    add_folder_to_file(cfg, NewFolder { dir, target: Target::Folder, name: Some(name), id, mode })
}

fn attached(cfg: &Config) -> Vec<Folder> {
    cfg.attached_folders().cloned().collect()
}

#[test]
fn roundtrip_and_validation() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        r#"
server = "http://master:7443"
[[folders]]
path = "/data/docs"
name = "docs"
"#,
    )
    .unwrap();
    let cfg = Config::load(&path).unwrap();
    assert!(cfg.watch && cfg.scan_interval_secs == 3600 && cfg.bandwidth_limit_kbps == 0);
    cfg.save(&path).unwrap();
    assert_eq!(Config::load(&path).unwrap().folders.len(), 1);
}

#[test]
fn misplaced_or_misspelled_keys_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    // a top-level key written after [[folders]] belongs to the folder table
    std::fs::write(
        &path,
        "server = \"https://x\"
[[folders]]
path = \"a\"
name = \"a\"
watch = false
",
    )
    .unwrap();
    assert!(format!("{:#}", Config::load(&path).unwrap_err()).contains("unknown field"));
}

#[test]
fn folders_are_added_and_removed_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.toml");
    std::fs::write(&cfg, "# my own notes\nserver = \"http://x:7443\"\nbandwidth_limit_kbps = 500 # keep it polite\n").unwrap();
    for d in ["a/photos", "b/photos", "c", "a/photos/raw"] {
        std::fs::create_dir_all(dir.path().join(d)).unwrap();
    }
    let a = add(&cfg, &dir.path().join("a/photos"), None).unwrap();
    let b = add(&cfg, &dir.path().join("b/photos"), None).unwrap();
    let c = add(&cfg, &dir.path().join("c"), Some("docs")).unwrap();
    assert_eq!(
        (a.name.as_str(), b.name.as_str(), c.name.as_str()),
        ("photos", "photos-2", "docs"),
        "same-named folders get distinct names"
    );

    let text = std::fs::read_to_string(&cfg).unwrap();
    assert!(text.contains("# my own notes") && text.contains("# keep it polite"), "comments survive: {text}");
    let loaded = Config::load(&cfg).unwrap();
    assert_eq!(loaded.folders.len(), 3);
    assert_eq!(loaded.bandwidth_limit_kbps, 500);

    let err = |r: Result<Folder>| format!("{:#}", r.unwrap_err());
    assert!(err(add(&cfg, &dir.path().join("a/photos"), None)).contains("already collected"));
    assert!(
        err(add(&cfg, &dir.path().join("a/photos/raw"), None)).contains("overlaps"),
        "a folder inside a collected one"
    );
    assert!(err(add(&cfg, dir.path(), Some("all"))).contains("overlaps"), "a folder containing collected ones");
    assert!(err(add(&cfg, &dir.path().join("missing"), None)).contains("cannot read"));

    // Names that differ only in case or in Unicode form are the same name to the master.
    let same = |name: &str| err(add(&cfg, &dir.path().join("c"), Some(name)));
    assert!(same("docs").contains("already collected"), "the same folder again");
    std::fs::create_dir_all(dir.path().join("d")).unwrap();
    let clash = |name: &str| err(add(&cfg, &dir.path().join("d"), Some(name)));
    assert!(clash("DOCS").contains("different name"), "case: {}", clash("DOCS"));
    assert!(clash("photos").contains("\"photos-2\""), "the error suggests a free name");
    add(&cfg, &dir.path().join("d"), Some("caf\u{e9}")).unwrap();
    std::fs::create_dir_all(dir.path().join("e")).unwrap();
    assert!(
        err(add(&cfg, &dir.path().join("e"), Some("cafe\u{301}"))).contains("different name"),
        "an accent as its own character"
    );
    remove_folder_from_file(&cfg, "caf\u{e9}").unwrap();

    remove_folder_from_file(&cfg, "photos-2").unwrap();
    let names: Vec<String> = Config::load(&cfg).unwrap().folders.into_iter().map(|f| f.name).collect();
    assert_eq!(names, ["photos", "docs"]);
    assert!(remove_folder_from_file(&cfg, "nope").is_err());
    assert!(std::fs::read_to_string(&cfg).unwrap().contains("# my own notes"));
}

#[test]
fn folders_can_be_added_to_a_config_written_by_install() {
    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("config.toml");
    let cfg = Config::new("http://x:7443");
    cfg.save(&cfg_path).unwrap();
    std::fs::create_dir_all(dir.path().join("one")).unwrap();
    add(&cfg_path, &dir.path().join("one"), None).unwrap();
    assert_eq!(Config::load(&cfg_path).unwrap().folders.len(), 1);
    remove_folder_from_file(&cfg_path, "one").unwrap();
    assert!(Config::load(&cfg_path).unwrap().folders.is_empty());
}

#[cfg(feature = "tray")]
#[test]
fn dashboard_url_defaults_to_port_8080_on_the_server_host() {
    let mut cfg = Config::new("http://nas.lan:7443");
    assert_eq!(cfg.dashboard_url(), "http://nas.lan:8080");
    cfg.server = "http://10.0.0.5".into();
    assert_eq!(cfg.dashboard_url(), "http://10.0.0.5:8080");
    cfg.web_url = Some("https://fafnirx.example.com".into());
    assert_eq!(cfg.dashboard_url(), "https://fafnirx.example.com");
    cfg.web_url = Some("javascript:alert(1)".into());
    assert_eq!(cfg.dashboard_url(), "http://10.0.0.5:8080", "only http(s) URLs are honoured");
}

#[test]
fn rejects_other_schemes_and_bad_names() {
    let mut cfg = Config::new("ftp://x");
    assert!(cfg.validate().is_err());
    cfg.server = "https://x".into();
    cfg.folders.push(Folder::device("a".into(), "a/b".into()));
    assert!(cfg.validate().is_err());
}

#[test]
fn each_synced_folder_has_its_own_mode() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("fafnirx.toml");
    std::fs::write(
        &cfg,
        "server = \"http://x:7443\"
",
    )
    .unwrap();
    for d in ["a", "b"] {
        std::fs::create_dir_all(dir.path().join(d)).unwrap();
    }
    attach(&cfg, "plain", None, &dir.path().join("a"), Mode::Both).unwrap();
    attach(&cfg, "mine", None, &dir.path().join("b"), Mode::Keep).unwrap();
    let text = std::fs::read_to_string(&cfg).unwrap();
    assert_eq!(text.matches("mode").count(), 1, "only a choice that is not the default is written: {text}");
    let mode = |name: &str| Config::load(&cfg).unwrap().attached_named(name).unwrap().mode;
    assert_eq!((mode("plain"), mode("mine")), (Mode::Both, Mode::Keep));

    set_mode_in_file(&cfg, "plain", Mode::Keep, Some(0)).unwrap();
    set_mode_in_file(&cfg, "mine", Mode::Both, None).unwrap();
    let loaded = Config::load(&cfg).unwrap();
    let (plain, mine) = (loaded.attached_named("plain").unwrap(), loaded.attached_named("mine").unwrap());
    assert_eq!((plain.mode, plain.ask_timeout_hours), (Mode::Keep, 0));
    assert_eq!((mine.mode, mine.ask_timeout_hours), (Mode::Both, 24), "back to the defaults");
    assert!(Mode::Both.pulls() && Mode::Keep.pulls() && !Mode::Up.pulls());
    assert!("ASK".parse::<Mode>().is_ok() && "theirs".parse::<Mode>().is_err());
}

#[test]
fn a_master_folder_can_be_synced_up_only_and_is_addressed_by_its_id() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("fafnirx.toml");
    std::fs::write(&cfg, "server = \"http://x:7443\"\n").unwrap();
    let id = "2f1b6a52-6d0e-4c0c-9a54-1f0b7f3e9a11";
    let up = attach(&cfg, "master:Música/Rock", Some(id), &dir.path().join("a"), Mode::Up).unwrap();
    assert_eq!((up.key(), up.mode), (id, Mode::Up));
    attach(&cfg, "master:Photos", None, &dir.path().join("b"), Mode::Both).unwrap();
    let loaded = Config::load(&cfg).unwrap();
    assert_eq!(attached(&loaded)[0], up, "what is written is read back");
    assert_eq!(
        (attached(&loaded)[1].key(), attached(&loaded)[1].mode),
        ("master:Photos", Mode::Both),
        "without an id the name is the key"
    );

    // A master folder is named in full: a bare name never means one.
    assert!(loaded.attached_named("Photos").is_err() && loaded.attached_named("master:Photos").is_ok());
    let bad = attach(&cfg, "master:../x", None, &dir.path().join("c"), Mode::Both);
    assert!(format!("{:#}", bad.unwrap_err()).contains("master folder"));

    assert_eq!(set_mode_in_file(&cfg, "master:Photos", Mode::Up, None).unwrap(), "master:Photos");
    assert_eq!(attached(&Config::load(&cfg).unwrap())[1].mode, Mode::Up);
    set_mode_in_file(&cfg, "master:Photos", Mode::Both, None).unwrap();
    assert!(
        !std::fs::read_to_string(&cfg).unwrap().contains("master:Photos\"\nmode"),
        "the default is not written"
    );
    assert!("UP".parse::<Mode>().is_ok() && "down".parse::<Mode>().is_err());
}

#[test]
fn a_folder_renamed_on_the_master_is_renamed_in_the_config_too() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("fafnirx.toml");
    std::fs::write(
        &cfg,
        "server = \"http://x:7443\"
",
    )
    .unwrap();
    attach(&cfg, "family", Some("id-1"), &dir.path().join("a"), Mode::Both).unwrap();
    attach(&cfg, "master:Music", Some("id-2"), &dir.path().join("b"), Mode::Up).unwrap();
    attach(&cfg, "old", None, &dir.path().join("c"), Mode::Both).unwrap();
    let listed = [
        ("ana/home".to_string(), "id-1".to_string()),
        ("master:Music".to_string(), "id-2".to_string()),
        ("old".to_string(), "id-9".to_string()),
    ];
    let changed = follow_renames_in_file(&cfg, &listed).unwrap();
    assert_eq!(
        changed,
        [("family".to_string(), "ana/home".to_string())],
        "only a folder with an id, and only when the name differs"
    );
    let loaded = Config::load(&cfg).unwrap();
    assert_eq!(
        attached(&loaded).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["ana/home", "master:Music", "old"]
    );
    assert_eq!(
        (attached(&loaded)[0].key(), attached(&loaded)[1].mode),
        ("id-1", Mode::Up),
        "the id and the rest stay"
    );
    assert!(follow_renames_in_file(&cfg, &listed).unwrap().is_empty(), "nothing more to do");
}

#[test]
fn synced_folders_are_attached_and_detached_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.toml");
    std::fs::write(&cfg, "# notes\nserver = \"http://x:7443\"\n").unwrap();
    for d in ["a", "b", "a/inner", "c"] {
        std::fs::create_dir_all(dir.path().join(d)).unwrap();
    }
    assert!(attached(&Config::load(&cfg).unwrap()).is_empty(), "a config without the table has none attached");
    let new = attach(&cfg, "fresh", None, &dir.path().join("not/there/yet"), Mode::Both).unwrap();
    assert!(new.path.is_dir(), "a folder that does not exist is made");
    assert_eq!(remove_folder_from_file(&cfg, "fresh").unwrap(), "fresh", "it says which folder it detached");
    add(&cfg, &dir.path().join("c"), None).unwrap();
    attach(&cfg, "family", None, &dir.path().join("a"), Mode::Both).unwrap();
    attach(&cfg, "work", None, &dir.path().join("b"), Mode::Both).unwrap();
    let loaded = Config::load(&cfg).unwrap();
    assert_eq!(attached(&loaded).iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["family", "work"]);
    assert_eq!(loaded.device_folders().count(), 1, "the backup folders are not touched");
    assert!(std::fs::read_to_string(&cfg).unwrap().contains("# notes"), "comments survive");

    let err = |r: Result<Folder>| format!("{:#}", r.unwrap_err());
    assert!(err(attach(&cfg, "family", None, &dir.path().join("a"), Mode::Both)).contains("already collected"));
    assert!(err(attach(&cfg, "other", None, &dir.path().join("a/inner"), Mode::Both)).contains("overlaps"));
    assert!(
        err(attach(&cfg, "other", None, &dir.path().join("c"), Mode::Both)).contains("already collected"),
        "not a folder that is backed up either"
    );
    std::fs::create_dir_all(dir.path().join("d")).unwrap();
    assert!(err(attach(&cfg, "family", None, &dir.path().join("d"), Mode::Both)).contains("attached twice"));
    assert!(err(attach(&cfg, "Bad Name", None, &dir.path().join("d"), Mode::Both)).contains("lowercase"));

    // Folders of two owners with one name: the short name is ambiguous, the full one is not.
    attach(&cfg, "bob/work", None, &dir.path().join("d"), Mode::Both).unwrap();
    let ambiguous = format!("{:#}", remove_folder_from_file(&cfg, "work").unwrap_err());
    assert!(ambiguous.contains("work, bob/work"), "{ambiguous}");
    remove_folder_from_file(&cfg, "bob/work").unwrap();
    remove_folder_from_file(&cfg, "family").unwrap();
    assert_eq!(attached(&Config::load(&cfg).unwrap()).len(), 1);
    remove_folder_from_file(&cfg, "work").unwrap();
    assert!(attached(&Config::load(&cfg).unwrap()).is_empty(), "nothing is attached any more");
    assert!(remove_folder_from_file(&cfg, "work").is_err());
}

#[test]
fn the_address_of_the_master_is_completed() {
    for (typed, url) in [
        ("nas.lan", "http://nas.lan:7443"),
        ("nas.lan:9000", "http://nas.lan:9000"),
        ("  10.0.0.5 ", "http://10.0.0.5:7443"),
        ("http://nas.lan:7443/", "http://nas.lan:7443"),
        ("https://fafnirx.example.com", "https://fafnirx.example.com"),
    ] {
        assert_eq!(normalize_server(typed), url, "{typed}");
    }
}

#[test]
fn the_old_target_name_computer_is_still_read_as_device() {
    let old: Folder = toml_edit::de::from_str(
        "path = \"/a\"
name = \"a\"
target = \"computer\"
",
    )
    .unwrap();
    assert_eq!(old.target, Target::Device);
    let new: Folder = toml_edit::de::from_str(
        "path = \"/a\"
name = \"a\"
target = \"device\"
",
    )
    .unwrap();
    assert_eq!(new.target, Target::Device);
}
