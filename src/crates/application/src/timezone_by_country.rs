//! Timezone of a new system from the country its location lies in.
//!
//! The lookup uses the public-domain `zone.tab` of the IANA timezone database,
//! which lists each country's timezones with a reference position, most
//! populous first. That is exact for countries with one timezone. For the
//! others the nearest reference position decides, which can be wrong close to
//! an internal boundary, so the owner can always choose the timezone instead.

use std::str::FromStr as _;

use chrono::{Offset as _, TimeZone as _};
use chrono_tz::Tz;

const ZONE_TABLE: &str = include_str!("../../../../assets/tzdata/zone.tab");

/// One timezone of a country with its reference position in degrees.
struct CountryZone {
    name: &'static str,
    latitude: f64,
    longitude: f64,
}

/// Returns the timezone to assume for a location in `country_code`.
///
/// With a `position` (microdegrees) the country's timezone with the nearest
/// reference position decides which clock applies. It is reported under the
/// client's timezone when that keeps the same clock in the same country, and
/// otherwise under the country's most populous timezone with that clock, so a
/// plant in Munich is `Europe/Berlin`, not the exclave zone that happens to be
/// closer. Without a position the client's timezone is used when it belongs to
/// the country, else the most populous one. Returns `None` for an unknown
/// country.
#[must_use]
pub fn timezone_for_country(
    country_code: &str,
    position: Option<(i32, i32)>,
    client_timezone: Option<&str>,
) -> Option<String> {
    let zones = country_zones(country_code);
    let first = zones.first()?;
    let client = client_timezone.filter(|client| zones.iter().any(|zone| zone.name == *client));
    let Some((latitude, longitude)) = position else {
        return Some(client.unwrap_or(first.name).to_owned());
    };
    let latitude = f64::from(latitude) / 1_000_000.0;
    let longitude = f64::from(longitude) / 1_000_000.0;
    let nearest = zones.iter().min_by(|left, right| {
        distance(left, latitude, longitude).total_cmp(&distance(right, latitude, longitude))
    })?;
    let Some(clock) = clock_signature(nearest.name) else {
        return Some(nearest.name.to_owned());
    };
    if let Some(client) = client
        && clock_signature(client) == Some(clock)
    {
        return Some(client.to_owned());
    }
    let representative = zones
        .iter()
        .find(|zone| clock_signature(zone.name) == Some(clock))
        .unwrap_or(nearest);
    Some(representative.name.to_owned())
}

/// Lists the timezones of a country in the table's order.
fn country_zones(country_code: &str) -> Vec<CountryZone> {
    ZONE_TABLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let code = columns.next()?;
            let coordinates = columns.next()?;
            let name = columns.next()?;
            if !code.eq_ignore_ascii_case(country_code) {
                return None;
            }
            let (latitude, longitude) = parse_coordinates(coordinates)?;
            Some(CountryZone {
                name,
                latitude,
                longitude,
            })
        })
        .collect()
}

/// Parses `±DDMM±DDDMM` or `±DDMMSS±DDDMMSS` into decimal degrees.
fn parse_coordinates(value: &str) -> Option<(f64, f64)> {
    let split = value
        .char_indices()
        .skip(1)
        .find(|(_, character)| matches!(character, '+' | '-'))?
        .0;
    let (latitude, longitude) = value.split_at(split);
    Some((parse_angle(latitude, 2)?, parse_angle(longitude, 3)?))
}

/// Parses one signed angle whose degrees take `degree_digits` digits.
fn parse_angle(value: &str, degree_digits: usize) -> Option<f64> {
    let sign = if value.starts_with('-') { -1.0 } else { 1.0 };
    let digits = value.get(1..)?;
    let degrees: f64 = digits.get(..degree_digits)?.parse().ok()?;
    let minutes: f64 = digits.get(degree_digits..degree_digits + 2)?.parse().ok()?;
    let seconds: f64 = match digits.get(degree_digits + 2..) {
        Some("") | None => 0.0,
        Some(rest) => rest.parse().ok()?,
    };
    Some(sign * (degrees + minutes / 60.0 + seconds / 3_600.0))
}

/// Squared distance on an equirectangular projection; enough to rank zones.
fn distance(zone: &CountryZone, latitude: f64, longitude: f64) -> f64 {
    let mut delta_longitude = (zone.longitude - longitude).abs();
    if delta_longitude > 180.0 {
        delta_longitude = 360.0 - delta_longitude;
    }
    let scale = f64::midpoint(zone.latitude, latitude).to_radians().cos();
    let x = delta_longitude * scale;
    let y = zone.latitude - latitude;
    x * x + y * y
}

/// UTC offsets in winter and summer; equal signatures keep the same clock.
fn clock_signature(name: &str) -> Option<(i32, i32)> {
    let zone = Tz::from_str(name).ok()?;
    let offset_at = |month: u32| {
        zone.with_ymd_and_hms(2026, month, 15, 12, 0, 0)
            .single()
            .map(|instant| instant.offset().fix().local_minus_utc())
    };
    Some((offset_at(1)?, offset_at(7)?))
}
