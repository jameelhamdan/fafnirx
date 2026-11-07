use super::*;

#[test]
fn a_source_shows_the_label_its_module_gave() {
    assert_eq!(label(&json!({"label": "3 files", "x": 1})), "3 files");
    assert_eq!(label(&json!({})), "");
}

#[test]
fn the_limit_per_run_has_bounds() {
    assert_eq!(per_run(&json!({})).unwrap(), DEFAULT_PER_RUN);
    assert_eq!(per_run(&json!({"max_per_run": 500})).unwrap(), 500);
    assert!(per_run(&json!({"max_per_run": 0})).is_err());
    assert!(per_run(&json!({"max_per_run": 501})).is_err());
}
