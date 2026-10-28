//! FHIR R4, the standard API for health data.
//!
//! - [`resources`]: the subset of FHIR R4 types HealthCraft reads and writes.
//! - [`export`]: a [`Chart`](healthcraft_record::Chart) as FHIR resources. The FHIR API is a
//!   projection of the event stream, like every other view.
//! - [`import`]: FHIR bundles (such as the synthetic patients Synthea generates) as clinical events.
//!
//! The target is US Core (the profile ONC certification tests); today's coverage is listed in
//! `docs/roadmap.md`.

pub mod export;
pub mod import;
pub mod resources;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use healthcraft_core::Timestamp;

/// Parses a FHIR `dateTime` or `date` (`2019`, `2019-02`, `2019-02-15`, `2019-02-15T09:12:33-05:00`).
/// Partial dates resolve to their first instant, in UTC.
pub fn parse_datetime(text: &str) -> Option<Timestamp> {
    let text = text.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(text) {
        return Some(dt.with_timezone(&Utc));
    }
    parse_date(text).map(|d| d.and_time(NaiveTime::MIN).and_utc())
}

/// Parses a FHIR `date` (`2019`, `2019-02` or `2019-02-15`), or the date part of a `dateTime`.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    let date_part = text.split('T').next().unwrap_or(text);
    let mut parts = date_part.splitn(3, '-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next().map_or(Some(1), |m| m.parse().ok())?;
    let day: u32 = parts.next().map_or(Some(1), |d| d.parse().ok())?;
    if !(1..=9999).contains(&year) {
        return None;
    }
    NaiveDate::from_ymd_opt(year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fhir_dates() {
        assert_eq!(parse_datetime("2019-02-15T09:12:33-05:00").unwrap().to_rfc3339(), "2019-02-15T14:12:33+00:00");
        assert_eq!(parse_datetime("2019-02-15").unwrap().to_rfc3339(), "2019-02-15T00:00:00+00:00");
        assert_eq!(parse_date("2019-02"), NaiveDate::from_ymd_opt(2019, 2, 1));
        assert_eq!(parse_date("2019"), NaiveDate::from_ymd_opt(2019, 1, 1));
        assert_eq!(parse_date("2019-02-15T10:00:00Z"), NaiveDate::from_ymd_opt(2019, 2, 15));
        for bad in ["", "abc", "2019-13-01", "2019-02-30", "0-01-01", "99999-01-01"] {
            assert_eq!(parse_datetime(bad), None, "{bad}");
        }
    }
}
