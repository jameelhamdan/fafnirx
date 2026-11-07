//! Just enough XML for the feeds a module reads: the value of an attribute, and the text of an element as it is meant.
//!
//! Regexes and not a parser, on purpose: a module reads one tag or element at a time out of feeds and pages that are not always
//! well-formed, and a module should not carry an XML parser for that. The master reads well-formed `.nfo` and XMP files with
//! `quick-xml` (`library::xml`); the two do not share code because they do not read the same documents.

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
#[path = "../tests/unit/xml.rs"]
mod tests;
