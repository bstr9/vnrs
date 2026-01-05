//! Utility functions and data structures for alpha datasets

use chrono::{DateTime, NaiveDate, Utc};

/// Data segment enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Segment {
    Train,
    Valid,
    Test,
}

/// Convert string to datetime
pub fn to_datetime(arg: &str) -> DateTime<Utc> {
    if arg.contains('-') {
        // Format: YYYY-MM-DD
        let date = NaiveDate::parse_from_str(arg, "%Y-%m-%d").unwrap();
        date.and_hms_opt(0, 0, 0).unwrap().and_utc()
    } else {
        // Format: YYYYMMDD
        let date = NaiveDate::parse_from_str(arg, "%Y%m%d").unwrap();
        date.and_hms_opt(0, 0, 0).unwrap().and_utc()
    }
}