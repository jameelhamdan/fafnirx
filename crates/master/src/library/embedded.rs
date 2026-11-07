//! The words, places and dates that people and programs write into a photo: a caption, keywords (what photo managers
//! and AI taggers store as tags), a GPS position and the time it was taken. They live in three places, and a file can
//! have any of them:
//! EXIF (ImageDescription, UserComment, the Windows XP* fields), IPTC (a JPEG's Photoshop segment) and XMP (an XML
//! packet that sits in the file, whatever its format). Nothing here fails: what cannot be read is just not there.

use super::xml::Doc;
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
            let units = body
                .chunks(2)
                .filter(|p| p.len() == 2)
                .map(|p| if big { u16::from_be_bytes([p[0], p[1]]) } else { u16::from_le_bytes([p[0], p[1]]) });
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
        exif::Value::Ascii(parts) => text(&parts.iter().map(|p| String::from_utf8_lossy(p)).collect::<Vec<_>>().join(" ")),
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
        exif::Value::Ascii(parts) => Some(parts.iter().map(|p| String::from_utf8_lossy(p)).collect::<Vec<_>>().join(" ")),
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
            exif::Value::Ascii(r) => r.first().and_then(|r| r.first()).is_some_and(|b| b.to_ascii_uppercase() == negative),
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

fn from_xmp(xml: &str) -> Embedded {
    let xml = &Doc::parse(xml);
    // For the caption, the language-neutral entry (`x-default`) is first in the list.
    let description = ["dc:description", "exif:UserComment", "photoshop:Headline", "dc:title"]
        .iter()
        .find_map(|tag| xml.items(tag).into_iter().find_map(|d| text(&d)));
    let mut keywords = Vec::new();
    for tag in [
        "dc:subject",
        "digiKam:TagsList",
        "lr:hierarchicalSubject",
        "MicrosoftPhoto:LastKeywordXMP",
        "mwg-kw:Keywords",
        "pdf:Keywords",
    ] {
        for k in xml.items(tag) {
            // `pdf:Keywords` is one string with separators.
            for part in k.split(if tag == "pdf:Keywords" { &[',', ';'][..] } else { &['\n'][..] }) {
                keywords.push(leaf(part).to_string());
            }
        }
    }
    let position = xml
        .property("exif:GPSLatitude")
        .zip(xml.property("exif:GPSLongitude"))
        .and_then(|(lat, lon)| super::meta::valid_position(xmp_coordinate(&lat)?, xmp_coordinate(&lon)?));
    let taken_at = ["exif:DateTimeOriginal", "xmp:CreateDate", "photoshop:DateCreated", "exif:DateTimeDigitized"]
        .iter()
        .find_map(|tag| super::meta::parse_time(&xml.property(tag)?))
        .filter(|t| *t > 0);
    Embedded { description, keywords, position, taken_at }
}

#[cfg(test)]
#[path = "../../tests/unit/library/embedded.rs"]
mod tests;
