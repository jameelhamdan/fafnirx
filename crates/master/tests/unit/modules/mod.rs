use super::*;

const SECRET: &str = "shared-secret-0123456789";
const BEARER: &str = "Bearer shared-secret-0123456789";

#[test]
fn a_plugin_joins_with_the_secret_and_nothing_else_does() {
    let r = Registry::with_secret(Some(SECRET.into()));
    assert_eq!(r.register(Some(BEARER), "video", "http://video:8090/"), Ok(()));
    assert_eq!(r.all().len(), 1);
    assert_eq!(r.all()[0].url, "http://video:8090", "the slash at the end goes");
    for header in [None, Some(""), Some("Bearer wrong"), Some("shared-secret-0123456789")] {
        assert!(matches!(r.register(header, "docs", "http://docs"), Err(Refusal::Secret(_))), "{header:?}");
    }
    for (id, address) in [("Odd", "http://x"), ("", "http://x"), ("docs", "ftp://x"), ("docs", "docs:8090")] {
        assert!(matches!(r.register(Some(BEARER), id, address), Err(Refusal::Invalid(_))), "{id} {address}");
    }
    assert_eq!(r.all().len(), 1, "only the first was taken");
    assert!(r.up("video").is_none() && r.active().is_empty(), "a module is not up before it has answered");
    let listed = r.plugins();
    assert_eq!((listed[0].id.as_str(), listed[0].state), ("video", "down"));
}

#[test]
fn a_plugin_that_must_run_is_reported_when_it_is_not_connected() {
    let r = Registry::with_secret(Some(SECRET.into())).expecting(" Video, fetch,video,BAD ID,");
    assert_eq!(r.expected, ["video", "fetch"]);
    r.register(Some(BEARER), "video", "http://video:8090").unwrap();
    r.register(Some(BEARER), "docs", "http://docs:1").unwrap();
    let listed: Vec<_> = r.plugins().iter().map(|s| (s.id.clone(), s.state, s.expected)).collect();
    assert_eq!(
        listed,
        [("docs".into(), "down", false), ("fetch".into(), "missing", true), ("video".into(), "down", true)]
    );
    let none = Registry::with_secret(None).expecting("video");
    assert!(none.plugins()[0].problem.as_deref().unwrap().contains("FAFNIRX_PLUGIN_SECRET"));
}

#[test]
fn without_a_secret_or_with_a_short_one_no_plugin_may_join() {
    for secret in [None, Some("short".to_string())] {
        let r = Registry::with_secret(secret);
        assert!(matches!(r.register(Some("Bearer short"), "video", "http://video"), Err(Refusal::Secret(_))));
        assert!(r.all().is_empty());
    }
}

#[test]
fn announcing_again_keeps_the_entry_a_new_address_replaces_it_and_silence_ends_it() {
    let r = Registry::with_secret(Some(SECRET.into()));
    r.register(Some(BEARER), "video", "http://a:1").unwrap();
    let first = r.all()[0].clone();
    r.register(Some(BEARER), "video", "http://a:1").unwrap();
    assert!(Arc::ptr_eq(&first, &r.all()[0]), "the same address is the same plugin");
    r.register(Some(BEARER), "video", "http://b:2").unwrap();
    assert_eq!((r.all().len(), r.all()[0].url.as_str()), (1, "http://b:2"));
    *r.all()[0].seen.lock().unwrap() = Instant::now() - FORGET_AFTER - Duration::from_secs(1);
    r.forget_silent();
    assert!(r.all().is_empty());
}
