use super::*;

#[test]
fn text_is_decoded_and_cdata_is_literal() {
    let doc = Doc::parse("<a><t> x &amp; y &lt;z&gt; &#65;&#x42; </t><c><![CDATA[ <b> &amp; ]]></c><e/></a>");
    assert_eq!(doc.text("t").as_deref(), Some("x & y <z> AB"));
    assert_eq!(doc.text("c").as_deref(), Some("<b> &amp;"));
    assert_eq!(doc.text("e"), None, "an empty element has no text");
    assert_eq!(doc.text("missing"), None);
}

#[test]
fn a_name_is_matched_whole_and_the_first_one_wins() {
    let doc = Doc::parse("<r><dc:subjectCode>no</dc:subjectCode><dc:subject>yes</dc:subject><dc:subject>later</dc:subject></r>");
    assert_eq!(doc.text("dc:subject").as_deref(), Some("yes"));
}

#[test]
fn a_list_is_its_entries_a_plain_value_is_its_text_and_an_attribute_stands_in() {
    let doc = Doc::parse(r#"<d x:short="s" a:one="1"><dc:subject><rdf:Bag><rdf:li>a &amp; b</rdf:li><rdf:li/><rdf:li>c</rdf:li></rdf:Bag></dc:subject><p:plain>text</p:plain></d>"#);
    assert_eq!(doc.items("dc:subject"), ["a & b", "", "c"]);
    assert_eq!(doc.items("p:plain"), ["text"]);
    assert_eq!(doc.items("x:short"), ["s"]);
    assert!(doc.items("nothing").is_empty());
    assert_eq!(doc.property("a:one").as_deref(), Some("1"));
    assert_eq!(doc.property("p:plain").as_deref(), Some("text"));
}

#[test]
fn what_was_read_before_a_break_is_kept() {
    let doc = Doc::parse("<a><t>kept</t><u>cut off");
    assert_eq!(doc.text("t").as_deref(), Some("kept"));
}
