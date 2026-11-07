use super::*;

#[test]
fn version_ordering() {
    assert!(newer("0.2.0", "0.1.9"));
    assert!(newer("1.0", "0.9.9"));
    assert!(!newer("0.1.0", "0.1.0"));
    assert!(!newer("0.1.0", "0.2.0")); // replayed older release
    assert!(newer("0.1.1", "0.1"));
}
