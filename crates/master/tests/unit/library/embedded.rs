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
        vec![
            field(Tag::GPSDateStamp, ascii(day)),
            field(Tag::GPSTimeStamp, rational(&[(22, 1), (13, 1), (20, 1)])),
        ]
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
