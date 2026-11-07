use super::*;

#[test]
fn attributes_are_read_with_either_quote() {
    let tag = r#"<enclosure url="https://x.test/a?b=1&amp;c=2" type='audio/mpeg'/>"#;
    assert_eq!(attribute(tag, "url").as_deref(), Some("https://x.test/a?b=1&amp;c=2"));
    assert_eq!(attribute(tag, "type").as_deref(), Some("audio/mpeg"));
    assert_eq!(attribute(tag, "length"), None);
}

#[test]
fn text_is_decoded_and_cdata_is_literal() {
    assert_eq!(unescape(" a &amp; b &lt;c&gt; &#65;&#x42; &bogus; & "), "a & b <c> AB &bogus; &");
    assert_eq!(unescape("<![CDATA[ a &amp; b ]]>"), "a &amp; b");
}
