//! Reading dates and places out of the text that photos and videos keep them in.

/// "2023-11-14 22:13:20" (as EXIF displays it), "2023:11:14 22:13:20" (as it is stored) or "2023-11-14T22:13:20Z" to unix
/// seconds, taken as UTC: EXIF has no time zone. Seconds may be missing, and a bare day is midnight.
pub fn parse_time(text: &str) -> Option<i64> {
    let digits: Vec<i64> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse().ok())
        .collect::<Option<_>>()?;
    let (y, mo, d, h, mi, s) = match digits[..] {
        [y, mo, d] => (y, mo, d, 0, 0, 0),
        [y, mo, d, h, mi] => (y, mo, d, h, mi, 0),
        [y, mo, d, h, mi, s, ..] => (y, mo, d, h, mi, s),
        _ => return None,
    };
    if !(1..=9999).contains(&y) || !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || s > 60 {
        return None;
    }
    // Days since 1970-01-01 (civil-from-days, Howard Hinnant).
    let (y, m) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + s)
}

/// A latitude and longitude that mean a place: the latitude in range, the longitude wrapped into -180..180 (some
/// writers count 0..360), and not the 0,0 that broken cameras write.
pub fn valid_position(lat: f64, lon: f64) -> Option<(f64, f64)> {
    if !lat.is_finite() || !lon.is_finite() || lat.abs() > 90.0 || lon.abs() > 360.0 {
        return None;
    }
    let lon = if lon > 180.0 {
        lon - 360.0
    } else if lon < -180.0 {
        lon + 360.0
    } else {
        lon
    };
    (!(lat == 0.0 && lon == 0.0)).then_some((lat, lon))
}

#[cfg(test)]
#[path = "../tests/unit/meta.rs"]
mod tests;
