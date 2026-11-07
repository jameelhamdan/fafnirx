use super::*;

#[test]
fn an_address_gives_the_name_of_its_file() {
    assert_eq!(name_of("https://example.test/files/Report%20final.pdf?x=1#top"), "Report final.pdf");
    assert_eq!(name_of("https://example.test/dir/"), "dir");
    assert_eq!(name_of("https://example.test/"), "download");
    assert_eq!(name_of("https://example.test/a/..%2F..%2Fetc%2Fpasswd"), "_.._etc_passwd");
}

#[test]
fn a_feed_gives_the_file_of_each_entry_enclosure_first() {
    let rss = r#"<rss><channel>
            <item><title>One</title><link>https://example.test/page/1</link><enclosure url="https://example.test/f/one.zip?a=1&amp;b=2" type="application/zip"/></item>
            <item><title>Two</title><link>https://example.test/f/two.pdf</link></item>
            <item><title>Same</title><enclosure url="https://example.test/f/two.pdf"/></item>
            <item><title>No file</title><link>ftp://example.test/x</link></item>
        </channel></rss>"#;
    assert_eq!(feed_addresses(rss), ["https://example.test/f/one.zip?a=1&b=2", "https://example.test/f/two.pdf"]);
    let atom = r#"<feed><entry><title>A</title><link rel="alternate" href="https://example.test/f/a.txt"/></entry>
            <entry><link href="https://example.test/f/b.txt"></link></entry></feed>"#;
    assert_eq!(feed_addresses(atom), ["https://example.test/f/a.txt", "https://example.test/f/b.txt"]);
    assert!(feed_addresses("<html>not a feed</html>").is_empty());
}

#[test]
fn settings_say_what_is_kept() {
    let cfg = Config::parse(&json!({"urls": ["https://x.test/a.mkv", "https://x.test/b.srt"], "include": r"\.mkv$", "min_mb": 1})).unwrap();
    assert!(cfg.wants("a.mkv", Some(5 << 20)) && !cfg.wants("b.srt", Some(5 << 20)));
    assert!(!cfg.wants("a.mkv", Some(10)), "smaller than the least");
    assert!(cfg.wants("a.mkv", None), "a size that is not known is not held against a file");
    assert!(Config::parse(&json!({})).is_err() && Config::parse(&json!({"urls": ["ftp://x"]})).is_err());
    assert!(Config::parse(&json!({"feed": "https://x.test/rss", "include": "("})).is_err());
}
