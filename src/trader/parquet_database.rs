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
use std::sync::RwLock;

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
        "OKX" => Some(Exchange::Okx),
        "BYBIT" => Some(Exchange::Bybit),
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
/// ``text
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
/// ``
pub struct ParquetDatabase {
    base_dir: PathBuf,
    bar_overviews: RwLock<Vec<BarOverview>>,
    tick_overviews: RwLock<Vec<TickOverview>>,
}

impl ParquetDatabase {
    /// Create a new ParquetDatabase with the given base directory.
    pub fn new(base_dir: PathBuf) -> Self {
        if let Err(e) = std::fs::create_dir_all(&base_dir) {
            tracing::warn!("创建Parquet数据库目录失败: {}", e);
        }
        Self {
            base_dir,
            bar_overviews: RwLock::new(Vec::new()),
            tick_overviews: RwLock::new(Vec::new()),
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
                .map_err(|e| format!("创建目录失败 {:?}: {}", parent, e))?;
        }
        Ok(())
    }
    // ----- DataFrame converters (feature-gated) -----

    #[cfg(feature = "alpha")]
    fn bars_to_dataframe(bars: &[BarData]) -> Result<DataFrame, String> {
        let n = bars.len();
        let mut datetimes: Vec<i64> = Vec::with_capacity(n);
        let mut symbols: Vec<String> = Vec::with_capacity(n);
        let mut exchanges: Vec<String> = Vec::with_capacity(n);
        let mut intervals: Vec<String> = Vec::with_capacity(n);
        let mut open_prices: Vec<f64> = Vec::with_capacity(n);
        let mut high_prices: Vec<f64> = Vec::with_capacity(n);
        let mut low_prices: Vec<f64> = Vec::with_capacity(n);
        let mut close_prices: Vec<f64> = Vec::with_capacity(n);
        let mut volumes: Vec<f64> = Vec::with_capacity(n);
        let mut turnovers: Vec<f64> = Vec::with_capacity(n);
        let mut open_interests: Vec<f64> = Vec::with_capacity(n);
        let mut gateway_names: Vec<String> = Vec::with_capacity(n);

        for bar in bars {
            datetimes.push(bar.datetime.timestamp_millis());
            symbols.push(bar.symbol.clone());
            exchanges.push(bar.exchange.value().to_string());
            intervals.push(bar.interval.map(|i| i.value().to_string()).unwrap_or_default());
            open_prices.push(bar.open_price);
            high_prices.push(bar.high_price);
            low_prices.push(bar.low_price);
            close_prices.push(bar.close_price);
            volumes.push(bar.volume);
            turnovers.push(bar.turnover);
            open_interests.push(bar.open_interest);
            gateway_names.push(bar.gateway_name.clone());
        }

        DataFrame::new(vec![
            Column::new("datetime".into(), datetimes),
            Column::new("symbol".into(), symbols),
            Column::new("exchange".into(), exchanges),
            Column::new("interval".into(), intervals),
            Column::new("open_price".into(), open_prices),
            Column::new("high_price".into(), high_prices),
            Column::new("low_price".into(), low_prices),
            Column::new("close_price".into(), close_prices),
            Column::new("volume".into(), volumes),
            Column::new("turnover".into(), turnovers),
            Column::new("open_interest".into(), open_interests),
            Column::new("gateway_name".into(), gateway_names),
        ])
        .map_err(|e| format!("创建Bar DataFrame失败: {}", e))
    }

    #[cfg(feature = "alpha")]
    fn dataframe_to_bars(df: &DataFrame) -> Result<Vec<BarData>, String> {
        let height = df.height();
        if height == 0 { return Ok(Vec::new()); }

        let datetimes = df.column("datetime").map_err(|e| format!("{}", e))?.i64().map_err(|e| format!("{}", e))?;
        let symbols = df.column("symbol").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;
        let exchanges = df.column("exchange").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;
        let intervals = df.column("interval").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;
        let open_prices = df.column("open_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let high_prices = df.column("high_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let low_prices = df.column("low_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let close_prices = df.column("close_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let volumes = df.column("volume").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let turnovers = df.column("turnover").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let open_interests = df.column("open_interest").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let gateway_names = df.column("gateway_name").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;

        let mut bars = Vec::with_capacity(height);
        for i in 0..height {
            let dt_millis = datetimes.get(i).unwrap_or(0);
            let datetime = DateTime::from_timestamp_millis(dt_millis).unwrap_or_else(Utc::now);
            let exchange_str = exchanges.get(i).unwrap_or("BINANCE");
            let interval_str = intervals.get(i).unwrap_or("");

            bars.push(BarData {
                gateway_name: gateway_names.get(i).unwrap_or("").to_string(),
                symbol: symbols.get(i).unwrap_or("").to_string(),
                exchange: exchange_from_str(exchange_str).unwrap_or(Exchange::Binance),
                datetime,
                interval: if interval_str.is_empty() { None } else { interval_from_str(interval_str) },
                volume: volumes.get(i).unwrap_or(0.0),
                turnover: turnovers.get(i).unwrap_or(0.0),
                open_interest: open_interests.get(i).unwrap_or(0.0),
                open_price: open_prices.get(i).unwrap_or(0.0),
                high_price: high_prices.get(i).unwrap_or(0.0),
                low_price: low_prices.get(i).unwrap_or(0.0),
                close_price: close_prices.get(i).unwrap_or(0.0),
                extra: None,
            });
        }
        Ok(bars)
    }
    #[cfg(feature = "alpha")]
    fn ticks_to_dataframe(ticks: &[TickData]) -> Result<DataFrame, String> {
        let n = ticks.len();
        let mut datetimes = Vec::with_capacity(n);
        let mut symbols = Vec::with_capacity(n);
        let mut exchanges = Vec::with_capacity(n);
        let mut last_prices = Vec::with_capacity(n);
        let mut last_volumes = Vec::with_capacity(n);
        let mut volumes = Vec::with_capacity(n);
        let mut turnovers = Vec::with_capacity(n);
        let mut open_interests = Vec::with_capacity(n);
        let mut bid_price_1 = Vec::with_capacity(n);
        let mut ask_price_1 = Vec::with_capacity(n);
        let mut bid_volume_1 = Vec::with_capacity(n);
        let mut ask_volume_1 = Vec::with_capacity(n);
        let mut gateway_names = Vec::with_capacity(n);

        for t in ticks {
            datetimes.push(t.datetime.timestamp_millis());
            symbols.push(t.symbol.clone());
            exchanges.push(t.exchange.value().to_string());
            last_prices.push(t.last_price);
            last_volumes.push(t.last_volume);
            volumes.push(t.volume);
            turnovers.push(t.turnover);
            open_interests.push(t.open_interest);
            bid_price_1.push(t.bid_price_1);
            ask_price_1.push(t.ask_price_1);
            bid_volume_1.push(t.bid_volume_1);
            ask_volume_1.push(t.ask_volume_1);
            gateway_names.push(t.gateway_name.clone());
        }

        DataFrame::new(vec![
            Column::new("datetime".into(), datetimes),
            Column::new("symbol".into(), symbols),
            Column::new("exchange".into(), exchanges),
            Column::new("last_price".into(), last_prices),
            Column::new("last_volume".into(), last_volumes),
            Column::new("volume".into(), volumes),
            Column::new("turnover".into(), turnovers),
            Column::new("open_interest".into(), open_interests),
            Column::new("bid_price_1".into(), bid_price_1),
            Column::new("ask_price_1".into(), ask_price_1),
            Column::new("bid_volume_1".into(), bid_volume_1),
            Column::new("ask_volume_1".into(), ask_volume_1),
            Column::new("gateway_name".into(), gateway_names),
        ])
        .map_err(|e| format!("创建Tick DataFrame失败: {}", e))
    }

    #[cfg(feature = "alpha")]
    fn dataframe_to_ticks(df: &DataFrame) -> Result<Vec<TickData>, String> {
        let height = df.height();
        if height == 0 { return Ok(Vec::new()); }

        let datetimes = df.column("datetime").map_err(|e| format!("{}", e))?.i64().map_err(|e| format!("{}", e))?;
        let symbols = df.column("symbol").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;
        let exchanges = df.column("exchange").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;
        let last_prices = df.column("last_price").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let last_volumes = df.column("last_volume").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let volumes = df.column("volume").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let turnovers = df.column("turnover").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let open_interests = df.column("open_interest").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let bid_prices_1 = df.column("bid_price_1").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let ask_prices_1 = df.column("ask_price_1").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let bid_volumes_1 = df.column("bid_volume_1").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let ask_volumes_1 = df.column("ask_volume_1").map_err(|e| format!("{}", e))?.f64().map_err(|e| format!("{}", e))?;
        let gateway_names = df.column("gateway_name").map_err(|e| format!("{}", e))?.str().map_err(|e| format!("{}", e))?;

        let mut ticks = Vec::with_capacity(height);
        for i in 0..height {
            let dt_millis = datetimes.get(i).unwrap_or(0);
            let datetime = DateTime::from_timestamp_millis(dt_millis).unwrap_or_else(Utc::now);
            let exchange_str = exchanges.get(i).unwrap_or("BINANCE");

            let mut tick = TickData::new(
                gateway_names.get(i).unwrap_or("").to_string(),
                symbols.get(i).unwrap_or("").to_string(),
                exchange_from_str(exchange_str).unwrap_or(Exchange::Binance),
                datetime,
            );
            tick.last_price = last_prices.get(i).unwrap_or(0.0);
            tick.last_volume = last_volumes.get(i).unwrap_or(0.0);
            tick.volume = volumes.get(i).unwrap_or(0.0);
            tick.turnover = turnovers.get(i).unwrap_or(0.0);
            tick.open_interest = open_interests.get(i).unwrap_or(0.0);
            tick.bid_price_1 = bid_prices_1.get(i).unwrap_or(0.0);
            tick.ask_price_1 = ask_prices_1.get(i).unwrap_or(0.0);
            tick.bid_volume_1 = bid_volumes_1.get(i).unwrap_or(0.0);
            tick.ask_volume_1 = ask_volumes_1.get(i).unwrap_or(0.0);
            ticks.push(tick);
        }
        Ok(ticks)
    }
    // ----- JSON helpers for orders/trades/positions/events -----

    fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>, String> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取文件失败 {:?}: {}", path, e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("解析JSON失败 {:?}: {}", path, e))
    }

    fn save_json<T: serde::Serialize>(path: &Path, data: &[T]) -> Result<(), String> {
        Self::ensure_parent_dir(path)?;
        let content = serde_json::to_string_pretty(data)
            .map_err(|e| format!("序列化JSON失败: {}", e))?;
        std::fs::write(path, content)
            .map_err(|e| format!("写入文件失败 {:?}: {}", path, e))
    }

    /// Load all JSON files from a directory and merge into a single vector.
    fn load_all_from_dir<T: serde::de::DeserializeOwned>(
        dir: &Path,
    ) -> Result<Vec<T>, String> {
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut result = Vec::new();
        let entries = std::fs::read_dir(dir)
            .map_err(|e| format!("读取目录失败 {:?}: {}", dir, e))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录条目失败: {}", e))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .map_err(|e| format!("读取文件失败 {:?}: {}", path, e))?;
            let items: Vec<T> = serde_json::from_str(&content)
                .map_err(|e| format!("解析JSON失败 {:?}: {}", path, e))?;
            result.extend(items);
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// BaseDatabase Implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl BaseDatabase for ParquetDatabase {
    // ----- Bar data -----

    async fn save_bar_data(&self, bars: Vec<BarData>, _stream: bool) -> Result<bool, String> {
        if bars.is_empty() {
            return Ok(true);
        }

        #[cfg(feature = "alpha")]
        {
            // Group bars by (symbol, exchange, interval)
            let mut groups: HashMap<(String, Exchange, Option<Interval>), Vec<BarData>> = HashMap::new();
            for bar in bars {
                let interval_val = bar.interval.unwrap_or(Interval::Minute);
                groups
                    .entry((bar.symbol.clone(), bar.exchange, Some(interval_val)))
                    .or_default()
                    .push(bar);
            }

            for ((symbol, exchange, interval_opt), mut new_bars) in groups {
                let interval = interval_opt.unwrap_or(Interval::Minute);
                // Ensure each bar has the interval set consistently
                for bar in &mut new_bars {
                    bar.interval = Some(interval);
                }
                let path = self.bar_path(&symbol, exchange, interval);
                Self::ensure_parent_dir(&path)?;

                // Load existing data and merge (dedup by timestamp)
                let mut existing_bars = if path.exists() {
                    let file = std::fs::File::open(&path)
                        .map_err(|e| format!("打开Parquet文件失败: {}", e))?;
                    let df = JsonReader::new(file)
                        .finish()
                        .map_err(|e| format!("读取Parquet失败: {}", e))?;
                    Self::dataframe_to_bars(&df)?
                } else {
                    Vec::new()
                };

                let existing_keys: std::collections::HashSet<i64> = existing_bars.iter()
                    .map(|b| b.datetime.timestamp())
                    .collect();

                for bar in &new_bars {
                    if !existing_keys.contains(&bar.datetime.timestamp()) {
                        existing_bars.push(bar.clone());
                    }
                }

                // Sort by datetime
                existing_bars.sort_by_key(|b| b.datetime);

                // Limit to 1M bars per file
                if existing_bars.len() > 1_000_000 {
                    existing_bars.drain(0..existing_bars.len() - 1_000_000);
                }

                // Write back
                let df = Self::bars_to_dataframe(&existing_bars)?;
                let mut file = std::fs::File::create(&path)
                    .map_err(|e| format!("创建Parquet文件失败: {}", e))?;
                JsonWriter::new(&mut file)
                    .finish(&mut df.clone())
                    .map_err(|e| format!("写入Parquet失败: {}", e))?;

                tracing::debug!("保存 {} 条Bar数据到 {:?}", new_bars.len(), path);
            }
        }

        #[cfg(not(feature = "alpha"))]
        {
            tracing::warn!("ParquetDatabase 需要 'alpha' feature 才能保存Bar数据");
        }

        Ok(true)
    }
    async fn load_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, String> {
        #[cfg(feature = "alpha")]
        {
            let path = self.bar_path(symbol, exchange, interval);
            if !path.exists() {
                return Ok(Vec::new());
            }

            let file = std::fs::File::open(&path)
                .map_err(|e| format!("打开Parquet文件失败: {}", e))?;
            let df = JsonReader::new(file)
                .finish()
                .map_err(|e| format!("读取Parquet失败: {}", e))?;

            let mut bars = Self::dataframe_to_bars(&df)?;

            // Filter by time range and matching criteria
            bars.retain(|b| {
                b.symbol == symbol
                    && b.exchange == exchange
                    && b.interval == Some(interval)
                    && b.datetime >= start
                    && b.datetime <= end
            });

            tracing::debug!("从 {:?} 加载 {} 条Bar数据", path, bars.len());
            Ok(bars)
        }

        #[cfg(not(feature = "alpha"))]
        {
            let _ = (symbol, exchange, interval, start, end);
            Ok(Vec::new())
        }
    }

    async fn delete_bar_data(&self, symbol: &str, exchange: Exchange, interval: Interval) -> Result<i64, String> {
        let path = self.bar_path(symbol, exchange, interval);
        if !path.exists() {
            return Ok(0);
        }

        // Count records before deleting
        let count = {
            #[cfg(feature = "alpha")]
            {
                let file = std::fs::File::open(&path)
                    .map_err(|e| format!("打开Parquet文件失败: {}", e))?;
                let df = JsonReader::new(file)
                    .finish()
                    .map_err(|e| format!("读取Parquet失败: {}", e))?;
                let bars = Self::dataframe_to_bars(&df)?;
                bars.iter().filter(|b| {
                    b.symbol == symbol
                        && b.exchange == exchange
                        && b.interval == Some(interval)
                }).count() as i64
            }
            #[cfg(not(feature = "alpha"))]
            {
                0_i64
            }
        };

        std::fs::remove_file(&path)
            .map_err(|e| format!("删除文件失败 {:?}: {}", path, e))?;

        Ok(count)
    }
    async fn get_bar_overview(&self) -> Result<Vec<BarOverview>, String> {
        let bars_dir = self.base_dir.join("bars");
        if !bars_dir.exists() {
            return Ok(Vec::new());
        }

        let mut overviews = Vec::new();
        let entries = std::fs::read_dir(&bars_dir)
            .map_err(|e| format!("读取bars目录失败: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录条目失败: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("parquet") {
                continue;
            }

            #[cfg(feature = "alpha")]
            {
                let file = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                let df = match JsonReader::new(file).finish() {
                    Ok(df) => df,
                    Err(_) => continue,
                };
                let bars = match Self::dataframe_to_bars(&df) {
                    Ok(b) => b,
                    Err(_) => continue,
                };

                if bars.is_empty() {
                    continue;
                }

                let count = bars.len() as i64;
                let start = bars.iter().map(|b| b.datetime).min();
                let end = bars.iter().map(|b| b.datetime).max();

                // Get exchange and interval from the data itself (authoritative source)
                let exchange = bars.first().map(|b| b.exchange);
                let interval = bars.first().and_then(|b| b.interval);
                let symbol = bars.first().map(|b| b.symbol.clone()).unwrap_or_default();

                overviews.push(BarOverview {
                    symbol,
                    exchange,
                    interval,
                    count,
                    start,
                    end,
                });
            }

            #[cfg(not(feature = "alpha"))]
            {
                let _ = &path;
            }
        }

        // Cache the overviews
        {
            let mut cached = self.bar_overviews.write().map_err(|e| e.to_string())?;
            *cached = overviews.clone();
        }

        Ok(overviews)
    }

    // ----- Tick data -----

    async fn save_tick_data(&self, ticks: Vec<TickData>, _stream: bool) -> Result<bool, String> {
        if ticks.is_empty() {
            return Ok(true);
        }

        #[cfg(feature = "alpha")]
        {
            // Group ticks by (symbol, exchange)
            let mut groups: HashMap<(String, Exchange), Vec<TickData>> = HashMap::new();
            for tick in ticks {
                groups
                    .entry((tick.symbol.clone(), tick.exchange))
                    .or_default()
                    .push(tick);
            }

            for ((symbol, exchange), new_ticks) in groups {
                let path = self.tick_path(&symbol, exchange);
                Self::ensure_parent_dir(&path)?;

                // Load existing data and merge (dedup by timestamp)
                let mut existing_ticks = if path.exists() {
                    let file = std::fs::File::open(&path)
                        .map_err(|e| format!("打开Parquet文件失败: {}", e))?;
                    let df = JsonReader::new(file)
                        .finish()
                        .map_err(|e| format!("读取Parquet失败: {}", e))?;
                    Self::dataframe_to_ticks(&df)?
                } else {
                    Vec::new()
                };

                let existing_keys: std::collections::HashSet<i64> = existing_ticks.iter()
                    .map(|t| t.datetime.timestamp())
                    .collect();

                for tick in &new_ticks {
                    if !existing_keys.contains(&tick.datetime.timestamp()) {
                        existing_ticks.push(tick.clone());
                    }
                }

                existing_ticks.sort_by_key(|t| t.datetime);

                // Limit to 1M ticks per file
                if existing_ticks.len() > 1_000_000 {
                    existing_ticks.drain(0..existing_ticks.len() - 1_000_000);
                }

                let df = Self::ticks_to_dataframe(&existing_ticks)?;
                let mut file = std::fs::File::create(&path)
                    .map_err(|e| format!("创建Parquet文件失败: {}", e))?;
                JsonWriter::new(&mut file)
                    .finish(&mut df.clone())
                    .map_err(|e| format!("写入Parquet失败: {}", e))?;

                tracing::debug!("保存 {} 条Tick数据到 {:?}", new_ticks.len(), path);
            }
        }

        #[cfg(not(feature = "alpha"))]
        {
            tracing::warn!("ParquetDatabase 需要 'alpha' feature 才能保存Tick数据");
        }

        Ok(true)
    }
    async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, String> {
        #[cfg(feature = "alpha")]
        {
            let path = self.tick_path(symbol, exchange);
            if !path.exists() {
                return Ok(Vec::new());
            }

            let file = std::fs::File::open(&path)
                .map_err(|e| format!("打开Parquet文件失败: {}", e))?;
            let df = JsonReader::new(file)
                .finish()
                .map_err(|e| format!("读取Parquet失败: {}", e))?;

            let mut ticks = Self::dataframe_to_ticks(&df)?;

            // Filter by time range and matching criteria
            ticks.retain(|t| {
                t.symbol == symbol
                    && t.exchange == exchange
                    && t.datetime >= start
                    && t.datetime <= end
            });

            tracing::debug!("从 {:?} 加载 {} 条Tick数据", path, ticks.len());
            Ok(ticks)
        }

        #[cfg(not(feature = "alpha"))]
        {
            let _ = (symbol, exchange, start, end);
            Ok(Vec::new())
        }
    }

    async fn delete_tick_data(&self, symbol: &str, exchange: Exchange) -> Result<i64, String> {
        let path = self.tick_path(symbol, exchange);
        if !path.exists() {
            return Ok(0);
        }

        // Count records before deleting
        let count = {
            #[cfg(feature = "alpha")]
            {
                let file = std::fs::File::open(&path)
                    .map_err(|e| format!("打开Parquet文件失败: {}", e))?;
                let df = JsonReader::new(file)
                    .finish()
                    .map_err(|e| format!("读取Parquet失败: {}", e))?;
                let ticks = Self::dataframe_to_ticks(&df)?;
                ticks.iter().filter(|t| {
                    t.symbol == symbol && t.exchange == exchange
                }).count() as i64
            }
            #[cfg(not(feature = "alpha"))]
            {
                0_i64
            }
        };

        std::fs::remove_file(&path)
            .map_err(|e| format!("删除文件失败 {:?}: {}", path, e))?;

        Ok(count)
    }

    async fn get_tick_overview(&self) -> Result<Vec<TickOverview>, String> {
        let ticks_dir = self.base_dir.join("ticks");
        if !ticks_dir.exists() {
            return Ok(Vec::new());
        }

        let mut overviews = Vec::new();
        let entries = std::fs::read_dir(&ticks_dir)
            .map_err(|e| format!("读取ticks目录失败: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("读取目录条目失败: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("parquet") {
                continue;
            }

            #[cfg(feature = "alpha")]
            {
                let file = match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                let df = match JsonReader::new(file).finish() {
                    Ok(df) => df,
                    Err(_) => continue,
                };
                let ticks = match Self::dataframe_to_ticks(&df) {
                    Ok(t) => t,
                    Err(_) => continue,
                };

                if ticks.is_empty() {
                    continue;
                }

                let count = ticks.len() as i64;
                let start = ticks.iter().map(|t| t.datetime).min();
                let end = ticks.iter().map(|t| t.datetime).max();

                // Get exchange from the data itself (authoritative source)
                let exchange = ticks.first().map(|t| t.exchange);
                let symbol = ticks.first().map(|t| t.symbol.clone()).unwrap_or_default();

                overviews.push(TickOverview {
                    symbol,
                    exchange,
                    count,
                    start,
                    end,
                });
            }

            #[cfg(not(feature = "alpha"))]
            {
                let _ = &path;
            }
        }

        // Cache the overviews
        {
            let mut cached = self.tick_overviews.write().map_err(|e| e.to_string())?;
            *cached = overviews.clone();
        }

        Ok(overviews)
    }
    // ----- Order data -----

    async fn save_order_data(&self, orders: Vec<OrderData>) -> Result<bool, String> {
        if orders.is_empty() {
            return Ok(true);
        }
        // Group orders by gateway_name
        let mut groups: HashMap<String, Vec<OrderData>> = HashMap::new();
        for order in orders {
            groups.entry(order.gateway_name.clone()).or_default().push(order);
        }
        for (gw, new_orders) in groups {
            let path = self.order_file_path(&gw);
            let mut existing: Vec<OrderData> = Self::load_json(&path)?;
            let existing_keys: std::collections::HashSet<String> = existing.iter()
                .map(|o| o.vt_orderid())
                .collect();
            for order in &new_orders {
                if !existing_keys.contains(&order.vt_orderid()) {
                    existing.push(order.clone());
                }
            }
            if existing.len() > 100_000 {
                existing.drain(0..existing.len() - 100_000);
            }
            Self::save_json(&path, &existing)?;
        }
        Ok(true)
    }

    // ----- Trade data -----

    async fn save_trade_data(&self, trades: Vec<TradeData>) -> Result<bool, String> {
        if trades.is_empty() {
            return Ok(true);
        }
        // Group trades by gateway_name
        let mut groups: HashMap<String, Vec<TradeData>> = HashMap::new();
        for trade in trades {
            groups.entry(trade.gateway_name.clone()).or_default().push(trade);
        }
        for (gw, new_trades) in groups {
            let path = self.trade_file_path(&gw);
            let mut existing: Vec<TradeData> = Self::load_json(&path)?;
            let existing_keys: std::collections::HashSet<String> = existing.iter()
                .map(|t| t.vt_tradeid())
                .collect();
            for trade in &new_trades {
                if !existing_keys.contains(&trade.vt_tradeid()) {
                    existing.push(trade.clone());
                }
            }
            if existing.len() > 100_000 {
                existing.drain(0..existing.len() - 100_000);
            }
            Self::save_json(&path, &existing)?;
        }
        Ok(true)
    }

    // ----- Position data -----

    async fn save_position_data(&self, positions: Vec<PositionData>) -> Result<bool, String> {
        if positions.is_empty() {
            return Ok(true);
        }
        // Group positions by gateway_name
        let mut groups: HashMap<String, Vec<PositionData>> = HashMap::new();
        for position in positions {
            groups.entry(position.gateway_name.clone()).or_default().push(position);
        }
        for (gw, new_positions) in groups {
            let path = self.position_file_path(&gw);
            let mut existing: Vec<PositionData> = Self::load_json(&path)?;
            // Upsert: replace existing positions with same ID
            for position in &new_positions {
                let key = position.vt_positionid();
                existing.retain(|p| p.vt_positionid() != key);
                existing.push(position.clone());
            }
            Self::save_json(&path, &existing)?;
        }
        Ok(true)
    }

    // ----- Event data -----

    async fn save_event(&self, event: EventRecord) -> Result<bool, String> {
        let path = self.event_file_path();
        let mut events: Vec<EventRecord> = Self::load_json(&path)?;
        events.push(event);
        if events.len() > 100_000 {
            events.drain(0..events.len() - 100_000);
        }
        Self::save_json(&path, &events)?;
        Ok(true)
    }

    // ----- Load methods -----

    async fn load_orders(&self, gateway_name: Option<&str>) -> Result<Vec<OrderData>, String> {
        match gateway_name {
            Some(gw) => {
                let path = self.order_file_path(gw);
                Self::load_json(&path)
            }
            None => {
                let dir = self.base_dir.join("orders");
                Self::load_all_from_dir(&dir)
            }
        }
    }

    async fn load_trades(&self, gateway_name: Option<&str>) -> Result<Vec<TradeData>, String> {
        match gateway_name {
            Some(gw) => {
                let path = self.trade_file_path(gw);
                Self::load_json(&path)
            }
            None => {
                let dir = self.base_dir.join("trades");
                Self::load_all_from_dir(&dir)
            }
        }
    }

    async fn load_positions(&self, gateway_name: Option<&str>) -> Result<Vec<PositionData>, String> {
        match gateway_name {
            Some(gw) => {
                let path = self.position_file_path(gw);
                Self::load_json(&path)
            }
            None => {
                let dir = self.base_dir.join("positions");
                Self::load_all_from_dir(&dir)
            }
        }
    }
}
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parquet_database_new() {
        let dir = tempfile::tempdir().expect("tempdir should succeed");
        let db = ParquetDatabase::new(dir.path().to_path_buf());
        assert!(db.base_dir.exists());
    }

    #[tokio::test]
    async fn test_bar_roundtrip() {
        #[cfg(feature = "alpha")]
        {
            let dir = tempfile::tempdir().expect("tempdir should succeed");
            let db = ParquetDatabase::new(dir.path().to_path_buf());

            let now = Utc::now();
            let bar = BarData {
                gateway_name: "BINANCE_SPOT".to_string(),
                symbol: "BTCUSDT".to_string(),
                exchange: Exchange::Binance,
                datetime: now,
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 5000000.0,
                open_interest: 0.0,
                open_price: 50000.0,
                high_price: 51000.0,
                low_price: 49000.0,
                close_price: 50500.0,
                extra: None,
            };

            db.save_bar_data(vec![bar.clone()], false).await.expect("save_bar_data should succeed");
            let start = now - chrono::Duration::hours(1);
            let end = now + chrono::Duration::hours(1);
            let loaded = db.load_bar_data("BTCUSDT", Exchange::Binance, Interval::Minute, start, end)
                .await.expect("load_bar_data should succeed");
            assert_eq!(loaded.len(), 1);
            assert!((loaded[0].close_price - 50500.0).abs() < 0.01);
        }

        #[cfg(not(feature = "alpha"))]
        {
            println!("test_bar_roundtrip skipped (alpha feature not enabled)");
        }
    }

    #[tokio::test]
    async fn test_bar_overview() {
        #[cfg(feature = "alpha")]
        {
            let dir = tempfile::tempdir().expect("tempdir should succeed");
            let db = ParquetDatabase::new(dir.path().to_path_buf());

            let now = Utc::now();
            let bar = BarData {
                gateway_name: "BINANCE_SPOT".to_string(),
                symbol: "BTCUSDT".to_string(),
                exchange: Exchange::Binance,
                datetime: now,
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 5000000.0,
                open_interest: 0.0,
                open_price: 50000.0,
                high_price: 51000.0,
                low_price: 49000.0,
                close_price: 50500.0,
                extra: None,
            };

            db.save_bar_data(vec![bar], false).await.expect("save should succeed");
            let overviews = db.get_bar_overview().await.expect("get_bar_overview should succeed");
            assert_eq!(overviews.len(), 1);
            assert_eq!(overviews[0].count, 1);
        }

        #[cfg(not(feature = "alpha"))]
        {
            println!("test_bar_overview skipped (alpha feature not enabled)");
        }
    }

    #[tokio::test]
    async fn test_delete_bar_data() {
        #[cfg(feature = "alpha")]
        {
            let dir = tempfile::tempdir().expect("tempdir should succeed");
            let db = ParquetDatabase::new(dir.path().to_path_buf());

            let now = Utc::now();
            let bar = BarData {
                gateway_name: "BINANCE_SPOT".to_string(),
                symbol: "BTCUSDT".to_string(),
                exchange: Exchange::Binance,
                datetime: now,
                interval: Some(Interval::Minute),
                volume: 100.0,
                turnover: 5000000.0,
                open_interest: 0.0,
                open_price: 50000.0,
                high_price: 51000.0,
                low_price: 49000.0,
                close_price: 50500.0,
                extra: None,
            };

            db.save_bar_data(vec![bar], false).await.expect("save should succeed");
            let deleted = db.delete_bar_data("BTCUSDT", Exchange::Binance, Interval::Minute)
                .await.expect("delete should succeed");
            assert_eq!(deleted, 1);
        }

        #[cfg(not(feature = "alpha"))]
        {
            println!("test_delete_bar_data skipped (alpha feature not enabled)");
        }
    }

    #[test]
    fn test_exchange_from_str() {
        assert_eq!(exchange_from_str("BINANCE"), Some(Exchange::Binance));
        assert_eq!(exchange_from_str("BINANCE_USDM"), Some(Exchange::BinanceUsdm));
        assert_eq!(exchange_from_str("UNKNOWN"), None);
    }

    #[test]
    fn test_interval_from_str() {
        assert_eq!(interval_from_str("1m"), Some(Interval::Minute));
        assert_eq!(interval_from_str("1h"), Some(Interval::Hour));
        assert_eq!(interval_from_str("d"), Some(Interval::Daily));
        assert_eq!(interval_from_str("unknown"), None);
    }

    #[tokio::test]
    async fn test_save_and_load_orders() {
        use super::super::constant::{Direction, Offset, OrderType, Status};

        let dir = tempfile::tempdir().expect("tempdir should succeed");
        let db = ParquetDatabase::new(dir.path().to_path_buf());

        let order = OrderData {
            gateway_name: "BINANCE_SPOT".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "test_order_1".to_string(),
            order_type: OrderType::Limit,
            direction: Some(Direction::Long),
            offset: Offset::None,
            price: 50000.0,
            volume: 0.01,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: Some(Utc::now()),
            reference: String::new(),
            extra: None,
        };

        db.save_order_data(vec![order.clone()]).await.expect("save_order_data should succeed");

        let loaded = db.load_orders(Some("BINANCE_SPOT")).await.expect("load_orders should succeed");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].orderid, "test_order_1");

        let all_loaded = db.load_orders(None).await.expect("load_orders(None) should succeed");
        assert_eq!(all_loaded.len(), 1);
    }

    #[tokio::test]
    async fn test_save_and_load_trades() {
        use super::super::constant::Direction;

        let dir = tempfile::tempdir().expect("tempdir should succeed");
        let db = ParquetDatabase::new(dir.path().to_path_buf());

        let trade = TradeData {
            gateway_name: "BINANCE_SPOT".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "order_1".to_string(),
            tradeid: "trade_1".to_string(),
            direction: Some(Direction::Long),
            offset: super::super::constant::Offset::None,
            price: 50000.0,
            volume: 0.01,
            datetime: Some(Utc::now()),
            extra: None,
        };

        db.save_trade_data(vec![trade]).await.expect("save_trade_data should succeed");

        let loaded = db.load_trades(None).await.expect("load_trades should succeed");
        assert_eq!(loaded.len(), 1);
    }

    #[tokio::test]
    async fn test_save_and_load_positions() {
        use super::super::constant::Direction;

        let dir = tempfile::tempdir().expect("tempdir should succeed");
        let db = ParquetDatabase::new(dir.path().to_path_buf());

        let position = PositionData {
            gateway_name: "BINANCE_SPOT".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            volume: 0.1,
            frozen: 0.0,
            price: 50000.0,
            pnl: 0.0,
            yd_volume: 0.0,
            extra: None,
        };

        db.save_position_data(vec![position]).await.expect("save_position_data should succeed");

        let loaded = db.load_positions(None).await.expect("load_positions should succeed");
        assert_eq!(loaded.len(), 1);
    }

    #[tokio::test]
    async fn test_save_event() {
        let dir = tempfile::tempdir().expect("tempdir should succeed");
        let db = ParquetDatabase::new(dir.path().to_path_buf());

        let event = EventRecord::new(1, "eOrder".to_string(), "BINANCE_SPOT".to_string(), "{}".to_string());
        db.save_event(event).await.expect("save_event should succeed");

        // Verify by loading the JSON directly
        let path = db.event_file_path();
        let events: Vec<EventRecord> = ParquetDatabase::load_json(&path).expect("load should succeed");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "eOrder");
    }
}