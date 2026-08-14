use time::{Duration, OffsetDateTime};

use crate::config::Config;
pub use time::format_description::well_known::Rfc3339;

pub fn get_year() -> i32 {
    now_utc().year()
}

pub fn now_utc() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

pub fn format_time(time: OffsetDateTime) -> String {
    time.format(&Rfc3339).unwrap() // TODO: need to check if safe.
}

pub fn now_utc_plus_sec_str(sec: f64) -> String {
    let new_time = now_utc() + Duration::seconds_f64(sec);
    format_time(new_time)
}

pub fn parse_utc(moment: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(moment, &Rfc3339).map_err(|_| Error::FailToDateParse(moment.to_string()))
}

// region:    --- Error

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    FailToDateParse(String),
}

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_year_matches_current_year() {
        let expected = OffsetDateTime::now_utc().year();
        assert_eq!(get_year(), expected);
    }

    #[test]
    fn test_now_utc_is_close_to_now() {
        let before = OffsetDateTime::now_utc();
        let now = now_utc();
        let after = OffsetDateTime::now_utc();
        assert!(now >= before, "now_utc() should not be before the check timestamp");
        assert!(now <= after, "now_utc() should not be after the check timestamp");
    }

    #[test]
    fn test_format_time_returns_rfc3339() {
        let moment = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let formatted = format_time(moment);
        let parsed = OffsetDateTime::parse(&formatted, &Rfc3339).unwrap();
        assert_eq!(parsed, moment);
    }

    #[test]
    fn test_now_utc_plus_sec_str_is_rfc3339_near_offset() {
        let s = now_utc_plus_sec_str(60.0);
        let parsed = OffsetDateTime::parse(&s, &Rfc3339).unwrap();
        let diff = parsed - now_utc();
        assert!(
            diff.abs() >= Duration::seconds(59) && diff.abs() <= Duration::seconds(61),
            "expected ~60s ahead, got {diff:?}"
        );
    }

    #[test]
    fn test_parse_utc_valid() {
        let moment = OffsetDateTime::parse("2023-11-01T12:00:00Z", &Rfc3339).unwrap();
        let parsed = parse_utc("2023-11-01T12:00:00Z").unwrap();
        assert_eq!(parsed, moment);
    }

    #[test]
    fn test_parse_utc_invalid_returns_fail_to_date_parse() {
        let err = parse_utc("not-a-date").unwrap_err();
        match err {
            Error::FailToDateParse(garbage) => assert_eq!(garbage, "not-a-date"),
        }
    }

    #[test]
    fn test_error_display_is_non_empty() {
        let err = Error::FailToDateParse("boom".to_string());
        let display = err.to_string();
        assert!(!display.is_empty());
        assert!(display.contains("boom"));
    }
}
