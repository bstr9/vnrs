//! Parquet-based database implementation for efficient columnar storage.
//!
//! Stores bar and tick data in Parquet files via Polars for high-performance
//! analytical queries. Order, trade, position, and event data use JSON sidecar
//! files (same pattern as FileDatabase).

#[cfg(feature = "alpha")]
use polars::prelude::*;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::constant::{Exchange, Interval};
use super::database::{BarOverview, BaseDatabase, EventRecord, TickOverview};
use super::object::{BarData, OrderData, PositionData, TickData, TradeData};

// ---------------------------------------------------------------------------
// Helper: Exchange / Interval from string
// ---------------------------------------------------------------------------

/// Parse an Exchange from its value() string.
fn exchange_from_str(s: &str) -> Option<Exchange> {
    match s {
        "CFFEX" => Some(Exchange::Cffex),
        "SHFE" => Some(Exchange::Shfe),
        "CZCE" => Some(Exchange::Czce),
        "DCE" => Some(Exchange::Dce),
        "INE" => Some(Exchange::Ine),
        "GFEX" => Some(Exchange::Gfex),
        "SSE" => Some(Exchange::Sse),
        "SZSE" => Some(Exchange::Szse),
        "BSE" => Some(Exchange::Bse),
        "SHHK" => Some(Exchange::Shhk),
        "SZHK" => Some(Exchange::Szhk),
        "SGE" => Some(Exchange::Sge),
        "WXE" => Some(Exchange::Wxe),
        "CFETS" => Some(Exchange::Cfets),
        "XBOND" => Some(Exchange::Xbond),
        "SMART" => Some(Exchange::Smart),
        "NYSE" => Some(Exchange::Nyse),
        "NASDAQ" => Some(Exchange::Nasdaq),
        "ARCA" => Some(Exchange::Arca),
        "EDGEA" => Some(Exchange::Edgea),
        "ISLAND" => Some(Exchange::Island),
        "BATS" => Some(Exchange::Bats),
        "IEX" => Some(Exchange::Iex),
        "AMEX" => Some(Exchange::Amex),
        "TSE" => Some(Exchange::Tse),
        "NYMEX" => Some(Exchange::Nymex),
        "COMEX" => Some(Exchange::Comex),
        "GLOBEX" => Some(Exchange::Globex),
        "IDEALPRO" => Some(Exchange::Idealpro),
        "CME" => Some(Exchange::Cme),
        "ICE" => Some(Exchange::Ice),
        "SEHK" => Some(Exchange::Sehk),
        "HKFE" => Some(Exchange::Hkfe),
        "SGX" => Some(Exchange::Sgx),
        "CBOT" => Some(Exchange::Cbot),
        "CBOE" => Some(Exchange::Cboe),
        "CFE" => Some(Exchange::Cfe),
        "DME" => Some(Exchange::Dme),
        "EUX" => Some(Exchange::Eurex),
        "APEX" => Some(Exchange::Apex),
        "LME" => Some(Exchange::Lme),
        "BMD" => Some(Exchange::Bmd),
        "TOCOM" => Some(Exchange::Tocom),
        "EUNX" => Some(Exchange::Eunx),
        "KRX" => Some(Exchange::Krx),
        "OTC" => Some(Exchange::Otc),
        "IBKRATS" => Some(Exchange::Ibkrats),
        "BINANCE" => Some(Exchange::Binance),
        "BINANCE_USDM" => Some(Exchange::BinanceUsdm),
        "BINANCE_COINM" => Some(Exchange::BinanceCoinm),
        "LOCAL" => Some(Exchange::Local),
        "GLOBAL" => Some(Exchange::Global),
        _ => None,
    }
}

/// Parse an Interval from its value() string.
fn interval_from_str(s: &str) -> Option<Interval> {
    match s {
        "1s" => Some(Interval::Second),
        "1m" => Some(Interval::Minute),
        "5m" => Some(Interval::Minute5),
        "15m" => Some(Interval::Minute15),
        "30m" => Some(Interval::Minute30),
        "1h" => Some(Interval::Hour),
        "4h" => Some(Interval::Hour4),
        "d" => Some(Interval::Daily),
        "w" => Some(Interval::Weekly),
        "tick" => Some(Interval::Tick),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ParquetDatabase
// ---------------------------------------------------------------------------

/// Parquet-based database for efficient columnar storage of bar and tick data.
///
/// File layout:
/// ```text
/// .rstrader/parquet_database/
///   bars/
///     BINANCE_BTCUSDT_1m.parquet
///   ticks/
///     BINANCE_BTCUSDT.parquet
///   orders/
///     BINANCE_SPOT.json
///   trades/
///     BINANCE_SPOT.json
///   positions/
///     BINANCE_SPOT.json
///   events/
///     events.json
/// ```
pub struct ParquetDatabase {
    base_dir: PathBuf,
    bar_overviews: std::sync::RwLock<Vec<BarOverview>>,
    tick_overviews: std::sync::RwLock<Vec<TickOverview>>,
}

impl ParquetDatabase {
    /// Create a new ParquetDatabase with the given base directory.
    pub fn new(base_dir: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&base_dir) {
            tracing::warn!("\u{521b}\u{5efa}Parquet\u{6570}\u{636e}\u{5e93}\u{76ee}\u{5f55}\u{5931}\u{8d25}: {}", e);
        }
        Self {
            base_dir,
            bar_overviews: std::sync::RwLock::new(Vec::new()),
            tick_overviews: std::sync::RwLock::new(Vec::new()),
        }
    }

    /// Create a ParquetDatabase using the default data directory.
    pub fn with_default_dir() -> Self {
        let base_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("trade_engine")
            .join("parquet_database");
        Self::new(base_dir)
    }

    // ----- Path helpers -----

    fn bar_path(&self, symbol: &str, exchange: Exchange, interval: Interval) -> PathBuf {
        self.base_dir
            .join("bars")
            .join(format!("{}_{}_{}.parquet", exchange.value(), symbol, interval.value()))
    }

    fn tick_path(&self, symbol: &str, exchange: Exchange) -> PathBuf {
        self.base_dir
            .join("ticks")
            .join(format!("{}_{}.parquet", exchange.value(), symbol))
    }

    fn order_file_path(&self, gateway_name: &str) -> PathBuf {
        self.base_dir.join("orders").join(format!("{}.json", gateway_name))
    }

    fn trade_file_path(&self, gateway_name: &str) -> PathBuf {
        self.base_dir.join("trades").join(format!("{}.json", gateway_name))
    }

    fn position_file_path(&self, gateway_name: &str) -> PathBuf {
        self.base_dir.join("positions").join(format!("{}.json", gateway_name))
    }

    fn event_file_path(&self) -> PathBuf {
        self.base_dir.join("events").join("events.json")
    }

    fn ensure_parent_dir(path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("\u{521b}\u{5efa}\u{76ee}\u{5f55}\u{5931}\u{8d25} {:?}: {}", parent, e))?;
        }
        Ok(())
    }
