//! The words, places and dates that people and programs write into a photo: a caption, keywords (what photo managers
//! and AI taggers store as tags), a GPS position and the time it was taken. They live in three places, and a file can
//! have any of them:
//! EXIF (ImageDescription, UserComment, the Windows XP* fields), IPTC (a JPEG's Photoshop segment) and XMP (an XML
//! packet that sits in the file, whatever its format). Nothing here fails: what cannot be read is just not there.

use std::io::Read;
use std::path::Path;

/// How much of a file is searched for an XMP packet or IPTC block (they sit near the start; HEIC puts them in a box).
const SCAN_MAX: u64 = 16 << 20;
/// A tag is at most this long (the limit of the tags table).
const KEYWORD_MAX: usize = 64;
const KEYWORDS_MAX: usize = 50;

#[derive(Debug, Default, PartialEq)]
pub struct Embedded {
    pub description: Option<String>,
    pub keywords: Vec<String>,
    pub position: Option<(f64, f64)>,
    /// Unix seconds of the wall clock when it was taken.
    pub taken_at: Option<i64>,
}

impl Embedded {
    /// Fills what is missing from `other`; what is already here wins.
    fn fill(&mut self, other: Embedded) {
        if self.description.is_none() {
            self.description = other.description;
        }
        for k in other.keywords {
            self.keywords.push(k);
        }
        if self.position.is_none() {
            self.position = other.position;
        }
        if self.taken_at.is_none() {
            self.taken_at = other.taken_at;
        }
    }

    fn tidy(mut self) -> Self {
        let mut seen = std::collections::HashSet::new();
        self.keywords = self
            .keywords
            .into_iter()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty() && k.chars().count() <= KEYWORD_MAX && seen.insert(k.to_lowercase()))
            .take(KEYWORDS_MAX)
            .collect();
        self
    }
}

/// Everything embedded in the file: the EXIF fields read already (`exif`) plus a look at the bytes for XMP and IPTC.
pub fn read(path: &Path, exif: Option<&exif::Exif>) -> Embedded {
    let mut bytes = Vec::new();
    if let Ok(file) = std::fs::File::open(path) {
        let _ = file.take(SCAN_MAX).read_to_end(&mut bytes);
    }
    combine(exif.map(from_exif).unwrap_or_default(), &bytes)
}

/// EXIF first, then the IPTC and XMP found in `bytes`.
fn combine(mut found: Embedded, bytes: &[u8]) -> Embedded {
    found.fill(from_iptc(bytes));
    if let Some(xmp) = find_xmp(bytes) {
        let x = from_xmp(&xmp);
        // XMP is what current software writes, and the caption in it is the one people edited last.
        let mut merged = Embedded { description: x.description, keywords: x.keywords, ..Embedded::default() };
        // The camera's own position and time are kept over what a program wrote later.
        let (position, taken_at) = (found.position.or(x.position), found.taken_at.or(x.taken_at));
        merged.fill(found);
        merged.position = position;
        merged.taken_at = taken_at;
        return merged.tidy();
    }
    found.tidy()
}

// --- EXIF ---------------------------------------------------------------------------------------------------------------

/// Captions the cameras write when the owner wrote nothing.
fn is_filler(text: &str) -> bool {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    !t.chars().any(char::is_alphanumeric)
        || ["olympus digital camera", "sony dsc", "dcim", "exif_jpeg", "created with", "default"]
            .iter()
            .any(|f| lower.starts_with(f))
}

fn text(s: &str) -> Option<String> {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    (!t.is_empty() && !is_filler(t)).then(|| t.to_string())
}

/// The text of an EXIF "UserComment": 8 bytes saying how it is encoded, then the text.
fn user_comment(bytes: &[u8]) -> Option<String> {
    if bytes.len() <= 8 {
        return None;
    }
    let (code, body) = bytes.split_at(8);
    match code {
        b"UNICODE\0" => {
            let big = !body.starts_with(&[0xFF, 0xFE]) && body.first() == Some(&0);
            let body = body.strip_prefix(&[0xFE, 0xFF]).or_else(|| body.strip_prefix(&[0xFF, 0xFE])).unwrap_or(body);
            let units = body.chunks(2).filter(|p| p.len() == 2).map(|p| {
                if big {
                    u16::from_be_bytes([p[0], p[1]])
                } else {
                    u16::from_le_bytes([p[0], p[1]])
                }
            });
            text(&char::decode_utf16(units).map(|c| c.unwrap_or('\u{FFFD}')).collect::<String>())
        }
        _ => text(&String::from_utf8_lossy(body)),
    }
}

/// The Windows "XP" fields are UTF-16 little endian bytes.
fn xp_text(value: &exif::Value) -> Option<String> {
    let exif::Value::Byte(bytes) = value else { return None };
    text(
        &char::decode_utf16(bytes.chunks(2).filter(|p| p.len() == 2).map(|p| u16::from_le_bytes([p[0], p[1]])))
            .map(|c| c.unwrap_or('\u{FFFD}'))
            .collect::<String>(),
    )
}

fn from_exif(data: &exif::Exif) -> Embedded {
    let field = |tag| data.get_field(tag, exif::In::PRIMARY);
    let ascii = |tag| match &field(tag)?.value {
        exif::Value::Ascii(parts) => {
            text(&parts.iter().map(|p| String::from_utf8_lossy(p)).collect::<Vec<_>>().join(" "))
        }
        _ => None,
    };
    let xp = |number| field(exif::Tag(exif::Context::Tiff, number)).and_then(|f| xp_text(&f.value));
    let comment = || match &field(exif::Tag::UserComment)?.value {
        exif::Value::Undefined(bytes, _) => user_comment(bytes),
        _ => None,
    };
    Embedded {
        description: ascii(exif::Tag::ImageDescription).or_else(comment).or_else(|| xp(0x9c9c)),
        keywords: xp(0x9c9e).map(|k| k.split(';').map(str::to_string).collect()).unwrap_or_default(),
        position: gps(data),
        taken_at: taken_at(data),
    }
}

/// The text of an EXIF field of ASCII text.
fn ascii_of(data: &exif::Exif, tag: exif::Tag) -> Option<String> {
    match &data.get_field(tag, exif::In::PRIMARY)?.value {
        exif::Value::Ascii(parts) => {
            Some(parts.iter().map(|p| String::from_utf8_lossy(p)).collect::<Vec<_>>().join(" "))
        }
        _ => None,
    }
}

/// When the photo was taken: the camera's own time, else the time it was digitised, else the clock of the GPS receiver, else
/// the time the file was last written (an editor sets that to when it saved). A date that means nothing (cameras with no clock
/// set write zeros or 1970) is skipped for the next.
pub fn taken_at(data: &exif::Exif) -> Option<i64> {
    let stamp = |tag| super::meta::parse_time(&ascii_of(data, tag)?).filter(|t| *t > 0);
    stamp(exif::Tag::DateTimeOriginal)
        .or_else(|| stamp(exif::Tag::DateTimeDigitized))
        .or_else(|| gps_time(data))
        .or_else(|| stamp(exif::Tag::DateTime))
}

/// The date and time the GPS receiver reported (UTC), when nothing better is there.
fn gps_time(data: &exif::Exif) -> Option<i64> {
    let day = ascii_of(data, exif::Tag::GPSDateStamp)?;
    let exif::Value::Rational(hms) = &data.get_field(exif::Tag::GPSTimeStamp, exif::In::PRIMARY)?.value else {
        return None;
    };
    let [h, m, s] = [0, 1, 2].map(|i| hms.get(i).filter(|r| r.denom != 0).map_or(0, |r| r.num / r.denom));
    super::meta::parse_time(&format!("{day} {h}:{m}:{s}")).filter(|t| *t > 0)
}

/// Degrees from the three numbers of a GPS coordinate: degrees, minutes and seconds. Some writers give degrees and
/// decimal minutes, or only degrees; a number with a zero denominator is no number (minutes and seconds then count as 0).
fn degrees_of(value: &exif::Value) -> Option<f64> {
    let parts: Vec<Option<f64>> = match value {
        exif::Value::Rational(p) => p.iter().map(|r| (r.denom != 0).then(|| r.to_f64())).collect(),
        exif::Value::SRational(p) => p.iter().map(|r| (r.denom != 0).then(|| r.to_f64())).collect(),
        _ => return None,
    };
    let degrees = (*parts.first()?)?;
    let part = |i: usize| parts.get(i).copied().flatten().unwrap_or(0.0).abs();
    // The sign belongs to the whole value: -33 degrees 30 minutes is -33.5, not -32.5.
    Some(degrees.signum() * (degrees.abs() + part(1) / 60.0 + part(2) / 3600.0))
}

/// The GPS position of a photo, in degrees (south and west are negative).
pub fn gps(data: &exif::Exif) -> Option<(f64, f64)> {
    let coordinate = |tag, reference, negative: u8| -> Option<f64> {
        let value = degrees_of(&data.get_field(tag, exif::In::PRIMARY)?.value)?;
        // A reference that is missing or unreadable means north and east.
        let behind = data.get_field(reference, exif::In::PRIMARY).is_some_and(|f| match &f.value {
            exif::Value::Ascii(r) => {
                r.first().and_then(|r| r.first()).is_some_and(|b| b.to_ascii_uppercase() == negative)
            }
            _ => false,
        });
        Some(if behind { -value.abs() } else { value })
    };
    super::meta::valid_position(
        coordinate(exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef, b'S')?,
        coordinate(exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef, b'W')?,
    )
}

// --- IPTC (a JPEG's Photoshop segment) -------------------------------------------------------------------------------

/// Keywords (2:25) and the caption (2:120) of the IPTC block in a JPEG.
fn from_iptc(jpeg: &[u8]) -> Embedded {
    let mut out = Embedded::default();
    if !jpeg.starts_with(&[0xFF, 0xD8]) {
        return out;
    }
    let mut at = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF {
        let marker = jpeg[at + 1];
        if marker == 0xDA || marker == 0xD9 {
            break; // the picture itself starts
        }
        let len = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        let body = jpeg.get(at + 4..(at + 2 + len).min(jpeg.len())).unwrap_or(&[]);
        if marker == 0xED && body.starts_with(b"Photoshop 3.0\0") {
            iptc_of(&body[14..], &mut out);
        }
        at += 2 + len;
    }
    out
}

/// The 8BIM resources of a Photoshop segment; resource 0x0404 holds the IPTC records.
fn iptc_of(mut data: &[u8], out: &mut Embedded) {
    while data.len() >= 12 && &data[..4] == b"8BIM" {
        let id = u16::from_be_bytes([data[4], data[5]]);
        let name = 1 + data[6] as usize;
        let name = name + name % 2; // the name is padded to an even length
        let Some(head) = data.get(6 + name..10 + name) else { return };
        let size = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let Some(body) = data.get(10 + name..10 + name + size) else { return };
        if id == 0x0404 {
            iptc_records(body, out);
        }
        data = data.get(10 + name + size + size % 2..).unwrap_or(&[]);
    }
}

fn iptc_records(mut data: &[u8], out: &mut Embedded) {
    while data.len() >= 5 && data[0] == 0x1C {
        let (record, set) = (data[1], data[2]);
        let size = u16::from_be_bytes([data[3], data[4]]) as usize;
        let Some(value) = data.get(5..5 + size) else { return };
        let value = String::from_utf8_lossy(value);
        match (record, set) {
            (2, 25) => out.keywords.push(value.into_owned()),
            (2, 120) if out.description.is_none() => out.description = text(&value),
            _ => {}
        }
        data = &data[5 + size..];
    }
}

// --- XMP ----------------------------------------------------------------------------------------------------------------

fn find(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|i| i + from)
}

/// The XMP packet in the bytes of a file of any format.
fn find_xmp(bytes: &[u8]) -> Option<String> {
    let start = find(bytes, b"<x:xmpmeta", 0).or_else(|| find(bytes, b"<xmpmeta", 0))?;
    let end = find(bytes, b"</x:xmpmeta>", start)
        .map(|e| e + 12)
        .or_else(|| find(bytes, b"</xmpmeta>", start).map(|e| e + 10))?;
    Some(String::from_utf8_lossy(&bytes[start..end]).into_owned())
}

fn unescape(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("<![CDATA[").and_then(|s| s.strip_suffix("]]>")).unwrap_or(s);
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#xA;", "\n")
        .replace("&amp;", "&")
}

/// The body of the first `<tag ...>...</tag>`.
fn element<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let mut from = 0;
    loop {
        let start = xml[from..].find(&open)? + from;
        let after = start + open.len();
        // `<dc:subject` must not match `<dc:subjectCode`.
        if !xml[after..].starts_with(|c: char| c == '>' || c == '/' || c.is_whitespace()) {
            from = after;
            continue;
        }
        let head_end = xml[after..].find('>')? + after;
        if xml[..head_end].ends_with('/') {
            return Some("");
        }
        let end = xml[head_end..].find(&format!("</{tag}>"))? + head_end;
        return Some(&xml[head_end + 1..end]);
    }
}

/// The value of `name="..."` on any element (the short form of a simple property).
fn attribute(xml: &str, name: &str) -> Option<String> {
    let key = format!("{name}=");
    let mut from = 0;
    while let Some(i) = xml[from..].find(&key) {
        let at = from + i;
        let quote = xml[at + key.len()..].chars().next()?;
        let preceded = xml[..at].ends_with(|c: char| c.is_whitespace());
        if preceded && (quote == '"' || quote == '\'') {
            let rest = &xml[at + key.len() + 1..];
            return Some(unescape(&rest[..rest.find(quote)?]));
        }
        from = at + key.len();
    }
    None
}

/// The `<rdf:li>` entries of a list property, or the text itself when it is a plain value.
fn items(xml: &str, tag: &str) -> Vec<String> {
    let Some(body) = element(xml, tag) else {
        return attribute(xml, tag).into_iter().collect();
    };
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(i) = rest.find("<rdf:li") {
        let Some(head_end) = rest[i..].find('>').map(|e| e + i) else { break };
        if rest[..head_end].ends_with('/') {
            rest = &rest[head_end..];
            continue;
        }
        let Some(end) = rest[head_end..].find("</rdf:li>").map(|e| e + head_end) else { break };
        out.push(unescape(&rest[head_end + 1..end]));
        rest = &rest[end..];
    }
    if out.is_empty() && !body.contains('<') {
        out.push(unescape(body));
    }
    out
}

/// "Animals|Dogs" and "Animals/Dogs" are paths of tags: the last part is the tag.
fn leaf(tag: &str) -> &str {
    tag.rsplit(['|', '/']).next().unwrap_or(tag)
}

/// "52,30.5N", "52,30,15N" or "52.5N": the XMP way to write a coordinate. Some programs write a plain signed number.
fn xmp_coordinate(text: &str) -> Option<f64> {
    let text = text.trim();
    let reference = text.chars().last().filter(char::is_ascii_alphabetic).map(|c| c.to_ascii_uppercase());
    let digits = if reference.is_some() { &text[..text.len() - 1] } else { text };
    let numbers: Vec<f64> = digits.split(',').map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let part = |i: usize| numbers.get(i).copied().unwrap_or(0.0);
    let value = part(0) + part(1) / 60.0 + part(2) / 3600.0;
    Some(if matches!(reference, Some('S' | 'W')) { -value.abs() } else { value })
}

/// A simple property, written as an attribute or as an element.
fn property(xml: &str, name: &str) -> Option<String> {
    attribute(xml, name).or_else(|| element(xml, name).map(unescape))
}

fn from_xmp(xml: &str) -> Embedded {
    // For the caption, the language-neutral entry (`x-default`) is first in the list.
    let description = ["dc:description", "exif:UserComment", "photoshop:Headline", "dc:title"]
        .iter()
        .find_map(|tag| items(xml, tag).into_iter().find_map(|d| text(&d)));
    let mut keywords = Vec::new();
    for tag in [
        "dc:subject",
        "digiKam:TagsList",
        "lr:hierarchicalSubject",
        "MicrosoftPhoto:LastKeywordXMP",
        "mwg-kw:Keywords",
        "pdf:Keywords",
    ] {
        for k in items(xml, tag) {
            // `pdf:Keywords` is one string with separators.
            for part in k.split(if tag == "pdf:Keywords" { &[',', ';'][..] } else { &['\n'][..] }) {
                keywords.push(leaf(part).to_string());
            }
        }
    }
    let position = property(xml, "exif:GPSLatitude")
        .zip(property(xml, "exif:GPSLongitude"))
        .and_then(|(lat, lon)| super::meta::valid_position(xmp_coordinate(&lat)?, xmp_coordinate(&lon)?));
    let taken_at = ["exif:DateTimeOriginal", "xmp:CreateDate", "photoshop:DateCreated", "exif:DateTimeDigitized"]
        .iter()
        .find_map(|tag| super::meta::parse_time(&property(xml, tag)?))
        .filter(|t| *t > 0);
    Embedded { description, keywords, position, taken_at }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exif::{Field, In, Rational, Tag, Value};

    fn rational(v: &[(u32, u32)]) -> Value {
        Value::Rational(v.iter().map(|&(num, denom)| Rational { num, denom }).collect())
    }

    /// A small JPEG, with an EXIF block made of `fields` when there are any.
    fn jpeg(fields: Vec<Field>, extra_segments: &[u8]) -> Vec<u8> {
        let mut plain = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut plain)
            .encode_image(&image::RgbImage::from_pixel(8, 4, image::Rgb([30, 60, 90])))
            .unwrap();
        let mut out = plain[..2].to_vec();
        if !fields.is_empty() {
            let mut tiff = std::io::Cursor::new(Vec::new());
            let mut writer = exif::experimental::Writer::new();
            for f in &fields {
                writer.push_field(f);
            }
            writer.write(&mut tiff, false).unwrap();
            let mut app1 = b"Exif\0\0".to_vec();
            app1.extend(tiff.into_inner());
            out.extend([0xFF, 0xE1]);
            out.extend(((app1.len() + 2) as u16).to_be_bytes());
            out.extend(app1);
        }
        out.extend(extra_segments);
        out.extend(&plain[2..]);
        out
    }

    fn segment(marker: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![0xFF, marker];
        out.extend(((body.len() + 2) as u16).to_be_bytes());
        out.extend(body);
        out
    }

    fn xmp_segment(xml: &str) -> Vec<u8> {
        let mut body = b"http://ns.adobe.com/xap/1.0/\0".to_vec();
        body.extend(xml.as_bytes());
        segment(0xE1, &body)
    }

    fn iptc_segment(records: &[(u8, &str)]) -> Vec<u8> {
        let mut iptc = Vec::new();
        for (set, value) in records {
            iptc.extend([0x1C, 2, *set]);
            iptc.extend((value.len() as u16).to_be_bytes());
            iptc.extend(value.as_bytes());
        }
        let mut body = b"Photoshop 3.0\0".to_vec();
        body.extend(b"8BIM");
        body.extend(0x0404u16.to_be_bytes());
        body.extend([0, 0]); // an empty name, padded
        body.extend((iptc.len() as u32).to_be_bytes());
        body.extend(&iptc);
        if iptc.len() % 2 == 1 {
            body.push(0);
        }
        segment(0xED, &body)
    }

    fn field(tag: Tag, value: Value) -> Field {
        Field { tag, ifd_num: In::PRIMARY, value }
    }

    fn ascii(s: &str) -> Value {
        Value::Ascii(vec![s.as_bytes().to_vec()])
    }

    fn embedded_in(bytes: &[u8]) -> Embedded {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.jpg");
        std::fs::write(&path, bytes).unwrap();
        let meta = super::super::meta::read(&path).unwrap();
        Embedded {
            description: meta.description,
            keywords: meta.keywords,
            position: meta.position,
            taken_at: meta.taken_at,
        }
    }

    #[test]
    fn exif_gps_and_caption_of_a_camera_photo() {
        // 52°30'15" N, 13°24'36" E (Berlin), with a caption in ImageDescription.
        let bytes = jpeg(
            vec![
                field(Tag::ImageDescription, ascii("Sunset over the lake")),
                field(Tag::GPSLatitudeRef, ascii("N")),
                field(Tag::GPSLatitude, rational(&[(52, 1), (30, 1), (15, 1)])),
                field(Tag::GPSLongitudeRef, ascii("E")),
                field(Tag::GPSLongitude, rational(&[(13, 1), (24, 1), (36, 1)])),
            ],
            &[],
        );
        let e = embedded_in(&bytes);
        let (lat, lon) = e.position.expect("position");
        assert!((lat - 52.504_166).abs() < 1e-4 && (lon - 13.41).abs() < 1e-4, "{lat} {lon}");
        assert_eq!(e.description.as_deref(), Some("Sunset over the lake"));
    }

    #[test]
    fn south_and_west_are_negative_and_minutes_may_be_decimal() {
        // Phones and some apps write degrees and decimal minutes (two numbers) with the reference in lower case.
        let bytes = jpeg(
            vec![
                field(Tag::GPSLatitudeRef, ascii("S")),
                field(Tag::GPSLatitude, rational(&[(33, 1), (5160, 100)])),
                field(Tag::GPSLongitudeRef, ascii("w")),
                field(Tag::GPSLongitude, rational(&[(151, 1), (1250, 100)])),
            ],
            &[],
        );
        let (lat, lon) = embedded_in(&bytes).position.unwrap();
        assert!((lat + 33.86).abs() < 1e-4 && (lon + 151.208_333).abs() < 1e-4, "{lat} {lon}");
    }

    #[test]
    fn a_user_comment_and_camera_filler_captions() {
        let mut comment = b"ASCII\0\0\0".to_vec();
        comment.extend(b"Dinner with friends");
        let bytes = jpeg(
            vec![
                field(Tag::ImageDescription, ascii("OLYMPUS DIGITAL CAMERA   ")),
                field(Tag::UserComment, Value::Undefined(comment, 0)),
            ],
            &[],
        );
        assert_eq!(embedded_in(&bytes).description.as_deref(), Some("Dinner with friends"));
        let bare = jpeg(vec![field(Tag::UserComment, Value::Undefined(b"ASCII\0\0\0        ".to_vec(), 0))], &[]);
        assert_eq!(embedded_in(&bare), Embedded::default());
    }

    #[test]
    fn xmp_gives_caption_keywords_and_a_position() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
            <rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/" exif:GPSLatitude="48,51.5N" exif:GPSLongitude="2,21.25E">
              <dc:description><rdf:Alt><rdf:li xml:lang="x-default">Eiffel &amp; friends</rdf:li></rdf:Alt></dc:description>
              <dc:subject><rdf:Bag><rdf:li>Paris</rdf:li><rdf:li>tower</rdf:li><rdf:li>paris</rdf:li></rdf:Bag></dc:subject>
              <lr:hierarchicalSubject><rdf:Bag><rdf:li>Places|France|Paris</rdf:li><rdf:li>Things|Architecture</rdf:li></rdf:Bag></lr:hierarchicalSubject>
            </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let e = embedded_in(&jpeg(vec![], &xmp_segment(xml)));
        assert_eq!(e.description.as_deref(), Some("Eiffel & friends"));
        assert_eq!(e.keywords, ["Paris", "tower", "Architecture"], "duplicates in any case are one tag");
        let (lat, lon) = e.position.unwrap();
        assert!((lat - 48.858_333).abs() < 1e-4 && (lon - 2.354_166).abs() < 1e-4, "{lat} {lon}");
    }

    #[test]
    fn iptc_keywords_and_caption() {
        let bytes = jpeg(vec![], &iptc_segment(&[(25, "beach"), (25, "holiday"), (120, "Day at the beach")]));
        let e = embedded_in(&bytes);
        assert_eq!(e.keywords, ["beach", "holiday"]);
        assert_eq!(e.description.as_deref(), Some("Day at the beach"));
    }

    #[test]
    fn the_windows_fields_are_read() {
        let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_le_bytes).chain([0, 0]).collect::<Vec<u8>>();
        let bytes = jpeg(
            vec![
                field(Tag(exif::Context::Tiff, 0x9c9e), Value::Byte(utf16("cat;sofa;ünï"))),
                field(Tag(exif::Context::Tiff, 0x9c9c), Value::Byte(utf16("Our cat"))),
            ],
            &[],
        );
        let e = embedded_in(&bytes);
        assert_eq!(e.keywords, ["cat", "sofa", "ünï"]);
        assert_eq!(e.description.as_deref(), Some("Our cat"));
    }

    #[test]
    fn xmp_in_a_png_and_the_ai_keywords_of_a_tagger_are_found() {
        // Any format works: the packet is found in the bytes. Here a PNG whose iTXt chunk carries it, tags as the
        // flat `digiKam:TagsList` and a caption in the short attribute form.
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description dc:description="A dog on grass">
            <digiKam:TagsList><rdf:Seq><rdf:li>Animals/Dog</rdf:li><rdf:li>outdoor</rdf:li></rdf:Seq></digiKam:TagsList>
            </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let mut png = Vec::new();
        image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let at = png.len() - 12; // before IEND
        let mut chunk = b"XML:com.adobe.xmp\0\0\0\0\0".to_vec();
        chunk.extend(xml.as_bytes());
        let mut itxt = (chunk.len() as u32).to_be_bytes().to_vec();
        itxt.extend(b"iTXt");
        itxt.extend(&chunk);
        itxt.extend([0, 0, 0, 0]); // the checksum is not checked here
        png.splice(at..at, itxt);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.png");
        std::fs::write(&path, &png).unwrap();
        let meta = super::super::meta::read(&path).unwrap();
        assert_eq!(meta.description.as_deref(), Some("A dog on grass"));
        assert_eq!(meta.keywords, ["Dog", "outdoor"]);
    }

    #[test]
    fn a_photo_with_nothing_embedded_has_nothing() {
        assert_eq!(embedded_in(&jpeg(vec![], &[])), Embedded::default());
        // A position of 0,0 is a camera without a fix, not a place.
        let zero = jpeg(
            vec![
                field(Tag::GPSLatitudeRef, ascii("N")),
                field(Tag::GPSLatitude, rational(&[(0, 1), (0, 1), (0, 1)])),
                field(Tag::GPSLongitudeRef, ascii("E")),
                field(Tag::GPSLongitude, rational(&[(0, 1), (0, 1), (0, 1)])),
            ],
            &[],
        );
        assert_eq!(embedded_in(&zero).position, None);
    }

    fn srational(v: &[(i32, i32)]) -> Value {
        Value::SRational(v.iter().map(|&(num, denom)| exif::SRational { num, denom }).collect())
    }

    fn position_of(fields: Vec<Field>) -> Option<(f64, f64)> {
        embedded_in(&jpeg(fields, &[])).position
    }

    fn taken(fields: Vec<Field>) -> Option<i64> {
        embedded_in(&jpeg(fields, &[])).taken_at
    }

    #[test]
    fn a_position_without_references_is_north_and_east() {
        let at = position_of(vec![
            field(Tag::GPSLatitude, rational(&[(52, 1), (30, 1), (0, 1)])),
            field(Tag::GPSLongitude, rational(&[(13, 1), (24, 1), (0, 1)])),
        ]);
        assert_eq!(at, Some((52.5, 13.4)));
        // A reference that is not text is as good as none.
        let odd = position_of(vec![
            field(Tag::GPSLatitudeRef, Value::Byte(vec![b'S'])),
            field(Tag::GPSLatitude, rational(&[(52, 1)])),
            field(Tag::GPSLongitude, rational(&[(13, 1)])),
        ]);
        assert_eq!(odd, Some((52.0, 13.0)));
    }

    #[test]
    fn signed_numbers_and_zero_denominators_are_understood() {
        let signed = position_of(vec![
            field(Tag::GPSLatitudeRef, ascii("S")),
            field(Tag::GPSLatitude, srational(&[(-33, 1), (30, 1)])),
            field(Tag::GPSLongitude, srational(&[(151, 1), (0, 1), (0, 1)])),
        ]);
        assert_eq!(signed, Some((-33.5, 151.0)), "a south reference and a negative number do not cancel out");
        // Minutes and seconds written as 0/0 count as nothing; degrees written so are no position.
        let lazy = position_of(vec![
            field(Tag::GPSLatitude, rational(&[(40, 1), (0, 0), (0, 0)])),
            field(Tag::GPSLongitude, rational(&[(8, 1), (0, 0), (0, 0)])),
        ]);
        assert_eq!(lazy, Some((40.0, 8.0)));
        let broken = position_of(vec![
            field(Tag::GPSLatitude, rational(&[(0, 0), (30, 1), (0, 1)])),
            field(Tag::GPSLongitude, rational(&[(8, 1), (0, 1), (0, 1)])),
        ]);
        assert_eq!(broken, None);
        // Only one of the two numbers is no place.
        assert_eq!(position_of(vec![field(Tag::GPSLatitude, rational(&[(40, 1), (0, 1), (0, 1)]))]), None);
        // The equator and the prime meridian are places; 0,0 is not (see the test below), and 91 degrees is not.
        let equator = position_of(vec![
            field(Tag::GPSLatitude, rational(&[(0, 1), (0, 1), (0, 1)])),
            field(Tag::GPSLongitude, rational(&[(10, 1), (0, 1), (0, 1)])),
        ]);
        assert_eq!(equator, Some((0.0, 10.0)));
        let over = position_of(vec![
            field(Tag::GPSLatitude, rational(&[(91, 1), (0, 1), (0, 1)])),
            field(Tag::GPSLongitude, rational(&[(10, 1), (0, 1), (0, 1)])),
        ]);
        assert_eq!(over, None);
    }

    #[test]
    fn the_time_is_the_cameras_then_the_next_best() {
        let date = |tag, text| field(tag, ascii(text));
        let original = || date(Tag::DateTimeOriginal, "2023:11:14 22:13:20");
        let digitized = || date(Tag::DateTimeDigitized, "2023:11:15 08:00:00");
        let written = || date(Tag::DateTime, "2023:11:16 09:00:00");
        assert_eq!(taken(vec![original(), digitized(), written()]), Some(1_700_000_000));
        assert_eq!(taken(vec![digitized(), written()]), Some(1_700_035_200), "no camera time: when it was digitised");
        assert_eq!(taken(vec![written()]), Some(1_700_125_200), "a screenshot or an edit has only this one");
        // A clock that was never set writes zeros or 1970: the next date is used.
        assert_eq!(taken(vec![date(Tag::DateTimeOriginal, "0000:00:00 00:00:00"), written()]), Some(1_700_125_200));
        assert_eq!(taken(vec![date(Tag::DateTimeOriginal, "1970:01:01 00:00:00"), written()]), Some(1_700_125_200));
        assert_eq!(taken(vec![date(Tag::DateTimeOriginal, "                   ")]), None);
        // Writers that leave out the seconds or use other separators still say when.
        assert_eq!(taken(vec![date(Tag::DateTimeOriginal, "2023-11-14 22:13")]), Some(1_699_999_980));
        assert_eq!(taken(vec![]), None);
    }

    #[test]
    fn the_gps_clock_comes_before_the_date_a_program_wrote() {
        let gps = |day: &str| {
            vec![field(Tag::GPSDateStamp, ascii(day)), field(Tag::GPSTimeStamp, rational(&[(22, 1), (13, 1), (20, 1)]))]
        };
        assert_eq!(taken(gps("2023:11:14")), Some(1_700_000_000));
        assert_eq!(taken(gps("0000:00:00")), None);
        let mut both = gps("2023:11:14");
        both.push(field(Tag::DateTime, ascii("2023:11:16 09:00:00")));
        assert_eq!(
            taken(both),
            Some(1_700_000_000),
            "the receiver's clock is when it was taken; DateTime is when it was saved"
        );
    }

    #[test]
    fn only_the_photos_own_fields_are_shown_not_its_thumbnail() {
        let bytes = jpeg(
            vec![
                field(Tag::Make, ascii("Canon")),
                Field { tag: Tag::Make, ifd_num: In::THUMBNAIL, value: ascii("Thumbnail Maker") },
                Field { tag: Tag::JPEGInterchangeFormat, ifd_num: In::THUMBNAIL, value: Value::Long(vec![0]) },
                Field { tag: Tag::JPEGInterchangeFormatLength, ifd_num: In::THUMBNAIL, value: Value::Long(vec![0]) },
            ],
            &[],
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.jpg");
        std::fs::write(&path, &bytes).unwrap();
        let meta = super::super::meta::read(&path).unwrap();
        assert_eq!(meta.exif.get("Make").and_then(|m| m.as_str()), Some("\"Canon\""));
    }

    #[test]
    fn xmp_gives_the_time_and_plain_decimal_coordinates() {
        let xml = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description xmp:CreateDate="2023-11-14T22:13:20"
            exif:GPSLatitude="52.52" exif:GPSLongitude="-13.405"></rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let e = embedded_in(&jpeg(vec![], &xmp_segment(xml)));
        assert_eq!(e.taken_at, Some(1_700_000_000));
        assert_eq!(e.position, Some((52.52, -13.405)));
        // The camera's own time and place win over what a program wrote.
        let both = jpeg(
            vec![
                field(Tag::DateTimeOriginal, ascii("2020:01:01 00:00:00")),
                field(Tag::GPSLatitude, rational(&[(10, 1), (0, 1), (0, 1)])),
                field(Tag::GPSLongitude, rational(&[(20, 1), (0, 1), (0, 1)])),
            ],
            &xmp_segment(xml),
        );
        let e = embedded_in(&both);
        assert_eq!((e.taken_at, e.position), (Some(1_577_836_800), Some((10.0, 20.0))));
    }

    #[test]
    fn a_damaged_part_of_the_exif_does_not_lose_the_rest() {
        // One IFD with a date, and a pointer to a next IFD that is not in the data: the whole block used to be dropped.
        let mut tiff = b"II*\0".to_vec();
        tiff.extend(8u32.to_le_bytes());
        tiff.extend(1u16.to_le_bytes());
        for part in [0x0132u16.to_le_bytes().to_vec(), 2u16.to_le_bytes().to_vec()] {
            tiff.extend(part);
        }
        tiff.extend(20u32.to_le_bytes());
        tiff.extend(26u32.to_le_bytes());
        tiff.extend(0xFFFFu32.to_le_bytes());
        tiff.extend(b"2023:11:14 22:13:20\0");
        let mut app1 = b"Exif\0\0".to_vec();
        app1.extend(tiff);
        assert_eq!(embedded_in(&jpeg(vec![], &segment(0xE1, &app1))).taken_at, Some(1_700_000_000));
    }
}
