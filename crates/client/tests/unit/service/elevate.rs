use super::*;

#[test]
fn a_package_is_installed_by_whoever_can_ask_for_permission_where_they_are() {
    let f = |root, terminal, graphical, sudo, pkexec| choose(&Facts { root, terminal, graphical, sudo, pkexec });
    assert_eq!(f(true, false, false, false, false), Way::Direct, "root needs nobody's permission");
    assert_eq!(f(false, true, true, true, true), Way::Sudo, "at a terminal, the terminal asks");
    assert_eq!(f(false, false, true, true, true), Way::Pkexec, "from the tray, a window asks");
    assert_eq!(f(false, true, false, false, true), Way::Manual, "a terminal without sudo, and no window: told");
    assert_eq!(f(false, false, false, true, true), Way::Manual, "nothing can ask");
    assert_eq!(f(false, false, true, true, false), Way::Manual, "no polkit");
}

#[test]
fn the_command_to_type_is_short_and_safe_for_the_shell() {
    let line = |words: &[&str]| words.iter().map(OsString::from).collect::<Vec<_>>();
    let default = crate::config::default_config_path();
    let default = default.to_string_lossy();
    assert_eq!(
        by_hand(&line(&["/usr/local/bin/fafnirx", "--config", &default, "folder", "remove", "Documents"])),
        "sudo fafnirx folder remove Documents",
        "the program by its name, the usual config left out"
    );
    assert_eq!(
        by_hand(&line(&["/usr/local/bin/fafnirx", "--config", "/Library/Application Support/x.toml", "folders"])),
        "sudo fafnirx --config '/Library/Application Support/x.toml' folders",
        "another config is kept, quoted"
    );
    assert_eq!(by_hand(&line(&["fafnirx", "folder", "add", "/a/it's"])), r"sudo fafnirx folder add '/a/it'\''s'");
}

#[test]
fn words_are_quoted_for_a_windows_command_line() {
    assert_eq!(quote(OsStr::new("run")), "run");
    assert_eq!(quote(OsStr::new(r"C:\Program Files\fafnirx")), r#""C:\Program Files\fafnirx""#);
    assert_eq!(quote(OsStr::new(r#"a "b""#)), r#""a \"b\"""#);
    assert_eq!(quote(OsStr::new("")), "\"\"");
}
