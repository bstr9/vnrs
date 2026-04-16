//! Synchronized bar generator for multi-symbol strategies.
//!
//! Collects bars from multiple symbols and emits synchronized batches
//! when all registered symbols have data for the same timestamp.
//!
//! # Example
//!
//! ```ignore
//! use trade_engine::chart::SynchronizedBarGenerator;
//!
//! let mut gen = SynchronizedBarGenerator::new(vec![
//!     "BTCUSDT.BINANCE".to_string(),
//!     "ETHUSDT.BINANCE".to_string(),
//! ]);
//!
//! // Feed bars — nothing emitted until both symbols have the same timestamp
//! let btc_bar = /* ... */;
//! gen.on_bar("BTCUSDT.BINANCE", btc_bar); // returns None
//!
//! let eth_bar = /* ... */;
//! let sync = gen.on_bar("ETHUSDT.BINANCE", eth_bar); // returns Some(SynchronizedBars)
//! ```

use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::trader::object::BarData;

/// A synchronized batch of bars sharing the same timestamp.
///
/// Contains one `BarData` per registered symbol, all with identical `datetime`.
#[derive(Debug, Clone)]
pub struct SynchronizedBars {
    /// The shared timestamp of all bars in this batch.
    pub datetime: DateTime<Utc>,
    /// Bars keyed by vt_symbol.
    pub bars: HashMap<String, BarData>,
}

impl SynchronizedBars {
    /// Get a bar for a specific vt_symbol.
    pub fn get(&self, vt_symbol: &str) -> Option<&BarData> {
        self.bars.get(vt_symbol)
    }

    /// List all vt_symbols present in this synchronized batch.
    pub fn symbols(&self) -> Vec<&String> {
        self.bars.keys().collect()
    }
}

/// Generator that synchronizes bars across multiple symbols.
///
/// Bars arrive independently (possibly out of order) for each symbol.
/// The generator buffers pending bars by timestamp and emits a
/// `SynchronizedBars` once every registered symbol has contributed
/// a bar for the same timestamp.
///
/// When a later timestamp completes first, any earlier incomplete
/// timestamps are left pending — they will only emit once their
/// own set of symbols is complete.
pub struct SynchronizedBarGenerator {
    /// The set of vt_symbols that must all be present for a sync emit.
    vt_symbols: HashSet<String>,
    /// Buffered bars: timestamp -> (vt_symbol -> BarData).
    buffer: BTreeMap<DateTime<Utc>, HashMap<String, BarData>>,
}

impl SynchronizedBarGenerator {
    /// Create a new generator for the given list of vt_symbols.
    ///
    /// At least one symbol is required. Duplicate symbols are deduplicated.
    pub fn new(vt_symbols: Vec<String>) -> Self {
        Self {
            vt_symbols: vt_symbols.into_iter().collect(),
            buffer: BTreeMap::new(),
        }
    }

    /// Ingest a bar for the given vt_symbol.
    ///
    /// Returns `Some(SynchronizedBars)` when all registered symbols have
    /// a bar for the same timestamp as this incoming bar.
    /// Returns `None` if the timestamp is still incomplete.
    ///
    /// # Panics
    ///
    /// Panics if `vt_symbol` is not one of the registered symbols.
    pub fn on_bar(&mut self, vt_symbol: &str, bar: BarData) -> Option<SynchronizedBars> {
        assert!(
            self.vt_symbols.contains(vt_symbol),
            "unregistered vt_symbol: {vt_symbol}"
        );

        let dt = bar.datetime;
        let entry = self.buffer.entry(dt).or_default();
        entry.insert(vt_symbol.to_string(), bar);

        // Check whether this timestamp is now complete.
        if entry.len() == self.vt_symbols.len() {
            let bars = self.buffer.remove(&dt).unwrap();
            Some(SynchronizedBars { datetime: dt, bars })
        } else {
            None
        }
    }

    /// Number of incomplete timestamps currently waiting in the buffer.
    pub fn pending_count(&self) -> usize {
        self.buffer.len()
    }

    /// Clear all pending data.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::{Exchange, Interval};
    use chrono::TimeZone;

    fn make_bar(vt_symbol: &str, dt: DateTime<Utc>, close: f64) -> BarData {
        let parts: Vec<&str> = vt_symbol.split('.').collect();
        BarData {
            gateway_name: "test".to_string(),
            symbol: parts[0].to_string(),
            exchange: Exchange::Binance,
            datetime: dt,
            interval: Some(Interval::Minute),
            open_price: close,
            high_price: close,
            low_price: close,
            close_price: close,
            volume: 100.0,
            turnover: close * 100.0,
            open_interest: 0.0,
            extra: None,
        }
    }

    fn dt(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 1, 1, 10, minute, 0).unwrap()
    }

    #[test]
    fn test_single_symbol_passthrough() {
        let mut gen = SynchronizedBarGenerator::new(vec!["BTCUSDT.BINANCE".to_string()]);

        let bar = make_bar("BTCUSDT.BINANCE", dt(0), 42000.0);
        let result = gen.on_bar("BTCUSDT.BINANCE", bar.clone());

        assert!(result.is_some());
        let sync = result.unwrap();
        assert_eq!(sync.datetime, dt(0));
        assert_eq!(sync.bars.len(), 1);
        assert_eq!(sync.get("BTCUSDT.BINANCE").unwrap().close_price, 42000.0);
        assert_eq!(sync.symbols(), vec!["BTCUSDT.BINANCE"]);
        assert_eq!(gen.pending_count(), 0);
    }

    #[test]
    fn test_two_symbols_synchronized_emit() {
        let mut gen = SynchronizedBarGenerator::new(vec![
            "BTCUSDT.BINANCE".to_string(),
            "ETHUSDT.BINANCE".to_string(),
        ]);

        // First bar — incomplete
        let btc = make_bar("BTCUSDT.BINANCE", dt(0), 42000.0);
        let r1 = gen.on_bar("BTCUSDT.BINANCE", btc);
        assert!(r1.is_none());
        assert_eq!(gen.pending_count(), 1);

        // Second bar — completes the timestamp
        let eth = make_bar("ETHUSDT.BINANCE", dt(0), 2200.0);
        let r2 = gen.on_bar("ETHUSDT.BINANCE", eth);
        assert!(r2.is_some());

        let sync = r2.unwrap();
        assert_eq!(sync.datetime, dt(0));
        assert_eq!(sync.bars.len(), 2);
        assert_eq!(sync.get("BTCUSDT.BINANCE").unwrap().close_price, 42000.0);
        assert_eq!(sync.get("ETHUSDT.BINANCE").unwrap().close_price, 2200.0);
        assert_eq!(gen.pending_count(), 0);
    }

    #[test]
    fn test_out_of_order_bar_handling() {
        let mut gen = SynchronizedBarGenerator::new(vec![
            "BTCUSDT.BINANCE".to_string(),
            "ETHUSDT.BINANCE".to_string(),
        ]);

        // ETH for minute 1 arrives first (later timestamp)
        let eth_1 = make_bar("ETHUSDT.BINANCE", dt(1), 2205.0);
        let r1 = gen.on_bar("ETHUSDT.BINANCE", eth_1);
        assert!(r1.is_none());
        assert_eq!(gen.pending_count(), 1);

        // BTC for minute 0 arrives next (earlier timestamp)
        let btc_0 = make_bar("BTCUSDT.BINANCE", dt(0), 42000.0);
        let r2 = gen.on_bar("BTCUSDT.BINANCE", btc_0);
        assert!(r2.is_none());
        assert_eq!(gen.pending_count(), 2);

        // BTC for minute 1 arrives — completes minute 1
        let btc_1 = make_bar("BTCUSDT.BINANCE", dt(1), 42100.0);
        let r3 = gen.on_bar("BTCUSDT.BINANCE", btc_1);
        assert!(r3.is_some());
        let sync = r3.unwrap();
        assert_eq!(sync.datetime, dt(1));
        assert_eq!(sync.bars.len(), 2);
        assert_eq!(gen.pending_count(), 1); // minute 0 still incomplete

        // ETH for minute 0 arrives — completes minute 0
        let eth_0 = make_bar("ETHUSDT.BINANCE", dt(0), 2200.0);
        let r4 = gen.on_bar("ETHUSDT.BINANCE", eth_0);
        assert!(r4.is_some());
        let sync = r4.unwrap();
        assert_eq!(sync.datetime, dt(0));
        assert_eq!(gen.pending_count(), 0);
    }

    #[test]
    fn test_incomplete_sync_not_all_symbols() {
        let mut gen = SynchronizedBarGenerator::new(vec![
            "BTCUSDT.BINANCE".to_string(),
            "ETHUSDT.BINANCE".to_string(),
            "SOLUSDT.BINANCE".to_string(),
        ]);

        // Only two of three symbols arrive
        let btc = make_bar("BTCUSDT.BINANCE", dt(0), 42000.0);
        assert!(gen.on_bar("BTCUSDT.BINANCE", btc).is_none());

        let eth = make_bar("ETHUSDT.BINANCE", dt(0), 2200.0);
        assert!(gen.on_bar("ETHUSDT.BINANCE", eth).is_none());

        assert_eq!(gen.pending_count(), 1);

        // Clear and verify buffer is empty
        gen.clear();
        assert_eq!(gen.pending_count(), 0);
    }

    #[test]
    fn test_multiple_timestamps_sequential() {
        let mut gen = SynchronizedBarGenerator::new(vec![
            "BTCUSDT.BINANCE".to_string(),
            "ETHUSDT.BINANCE".to_string(),
        ]);

        for minute in 0..3u32 {
            let btc = make_bar("BTCUSDT.BINANCE", dt(minute), 42000.0 + minute as f64);
            let eth = make_bar("ETHUSDT.BINANCE", dt(minute), 2200.0 + minute as f64);

            assert!(gen.on_bar("BTCUSDT.BINANCE", btc).is_none());
            let r = gen.on_bar("ETHUSDT.BINANCE", eth);
            assert!(r.is_some());
            assert_eq!(r.unwrap().datetime, dt(minute));
        }

        assert_eq!(gen.pending_count(), 0);
    }

    #[test]
    #[should_panic(expected = "unregistered vt_symbol")]
    fn test_unregistered_symbol_panics() {
        let mut gen = SynchronizedBarGenerator::new(vec!["BTCUSDT.BINANCE".to_string()]);
        let bar = make_bar("ETHUSDT.BINANCE", dt(0), 2200.0);
        gen.on_bar("ETHUSDT.BINANCE", bar);
    }

    #[test]
    fn test_duplicate_symbol_dedup() {
        let gen = SynchronizedBarGenerator::new(vec![
            "BTCUSDT.BINANCE".to_string(),
            "BTCUSDT.BINANCE".to_string(),
        ]);
        // Duplicates deduplicated — only need one bar per timestamp
        assert_eq!(gen.vt_symbols.len(), 1);
    }

    #[test]
    fn test_overwrite_bar_same_timestamp() {
        let mut gen = SynchronizedBarGenerator::new(vec!["BTCUSDT.BINANCE".to_string()]);

        let bar1 = make_bar("BTCUSDT.BINANCE", dt(0), 42000.0);
        let r1 = gen.on_bar("BTCUSDT.BINANCE", bar1);
        assert!(r1.is_some());
        assert_eq!(
            r1.unwrap().get("BTCUSDT.BINANCE").unwrap().close_price,
            42000.0
        );
        assert_eq!(gen.pending_count(), 0);

        // Second bar for same symbol at same timestamp — starts fresh buffer entry
        let bar2 = make_bar("BTCUSDT.BINANCE", dt(0), 43000.0);
        let r2 = gen.on_bar("BTCUSDT.BINANCE", bar2);
        assert!(r2.is_some());
        assert_eq!(
            r2.unwrap().get("BTCUSDT.BINANCE").unwrap().close_price,
            43000.0
        );
    }
}
