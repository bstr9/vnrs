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
pub fn to_datetime(arg: &str) -> Result<DateTime<Utc>, String> {
    if arg.contains('-') {
        // Format: YYYY-MM-DD
        let date = NaiveDate::parse_from_str(arg, "%Y-%m-%d")
            .map_err(|e| format!("Invalid date format '{}': {}", arg, e))?;
        let time = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| format!("Invalid time for date '{}'", arg))?;
        Ok(time.and_utc())
    } else {
        // Format: YYYYMMDD
        let date = NaiveDate::parse_from_str(arg, "%Y%m%d")
            .map_err(|e| format!("Invalid date format '{}': {}", arg, e))?;
        let time = date
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| format!("Invalid time for date '{}'", arg))?;
        Ok(time.and_utc())
    }
}
