//! Alpha BarData module for alpha research

use chrono::DateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlphaBarData {
    pub datetime: DateTime<chrono::Utc>,
    pub symbol: String,
    pub exchange: crate::trader::Exchange,
    pub interval: Option<crate::trader::Interval>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub turnover: f64,
    pub open_interest: f64,
    pub gateway_name: String,
}

impl AlphaBarData {
    pub fn vt_symbol(&self) -> String {
        format!("{}.{}", self.symbol, self.exchange)
    }
}