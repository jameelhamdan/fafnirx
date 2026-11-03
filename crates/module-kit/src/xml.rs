//! Just enough XML for the feeds a module reads: the value of an attribute, and the text of an element as it is meant.

use regex::Regex;

/// The value of attribute `name` in the text of one tag, quoted either way.
pub fn attribute(tag: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"(?s)\b{}\s*=\s*(?:"([^"]*)"|'([^']*)')"#, regex::escape(name))).ok()?;
    let c = re.captures(tag)?;
    Some(c.get(1).or_else(|| c.get(2))?.as_str().to_string())
}

/// Text of an XML element as it is meant: CDATA is literal, anything else has the five entities and numeric ones decoded.
pub fn unescape(s: &str) -> String {
    let s = s.trim();
    if let Some(literal) = s.strip_prefix("<![CDATA[").and_then(|c| c.strip_suffix("]]>")) {
        return literal.trim().to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|e| *e <= 8) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let decoded = match &rest[1..end] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            n => n
                .strip_prefix('#')
                .and_then(|d| d.strip_prefix(['x', 'X']).map_or_else(|| d.parse().ok(), |h| u32::from_str_radix(h, 16).ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
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
}
