use super::*;

#[test]
fn the_client_of_a_user_starts_with_their_config_when_they_log_in() {
    let (exe, cfg) = (Path::new("/apps/fafnirx"), Path::new("/me/config.toml"));
    let (client, tray) = user_commands(exe, cfg);
    assert_eq!(client, "\"/apps/fafnirx\" --config \"/me/config.toml\" run --log-file --hidden");
    assert_eq!(tray, "\"/apps/fafnirx\" --config \"/me/config.toml\" tray --hidden");
}

#[test]
fn installing_for_a_user_needs_their_config_and_a_dry_run_changes_nothing() {
    let (exe, cfg) = (Path::new("fafnirx"), Path::new("no-such-dir/config.toml"));
    assert!(install_service(cfg, exe, false).is_err(), "enroll comes first");
    install_service(cfg, exe, true).unwrap();
}
