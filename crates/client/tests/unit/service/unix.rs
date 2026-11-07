use super::*;

#[test]
fn unit_files_reference_the_binary_and_config() {
    let exe = Path::new("/usr/local/bin/fafnirx");
    let cfg = Path::new("/etc/fafnirx/config.toml");
    let unit = systemd_unit(exe, cfg);
    assert!(unit.contains("ExecStart=\"/usr/local/bin/fafnirx\" --config \"/etc/fafnirx/config.toml\" run --log-file"));
    assert!(
        unit.contains("Restart=on-failure") && unit.contains("WantedBy=multi-user.target"),
        "a system unit, not a user's"
    );
    let plist = client_plist(exe, cfg);
    assert!(plist.contains("<string>/usr/local/bin/fafnirx</string>") && plist.contains("<key>SuccessfulExit</key><false/>"));
    let tray = tray_plist(exe, cfg);
    assert!(tray.contains("io.fafnirx.tray") && tray.contains("<string>tray</string>") && !tray.contains("<string>run</string>") && tray.contains("<key>KeepAlive</key><false/>"));
    assert!(tray_desktop_entry(exe, cfg).contains("fafnirx\" --config \"/etc/fafnirx/config.toml\" tray"));
}
