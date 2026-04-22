//! LMDB database implementation for low-latency data persistence.
//!
//! Provides an embedded key-value store backend using LMDB via the `heed` crate.
//! Suitable for high-throughput scenarios where minimal read/write latency is required.
//! All data is serialized with serde_json for cross-backend compatibility.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use heed::types::Bytes;
use heed::{Database, Env, EnvOpenOptions};
use std::collections::HashMap;
use std::ops::Bound;
use std::sync::Arc;

use crate::error::DatabaseError;
use super::constant::{Exchange, Interval};
use super::database::{BaseDatabase, BarOverview, EventRecord, TickOverview};
use super::object::{BarData, OrderData, PositionData, TickData, TradeData};

/// LMDB database implementation.
///
/// Uses separate named databases for each data type:
/// - `bars`: Bar data with composite key (exchange, symbol, interval, timestamp)
/// - `ticks`: Tick data with composite key (exchange, symbol, timestamp)
/// - `orders`: Order data keyed by vt_orderid
/// - `trades`: Trade data keyed by vt_tradeid
/// - `positions`: Position data keyed by vt_positionid
/// - `events`: Event records keyed by 8-byte big-endian event_id
///
/// All operations are wrapped in `spawn_blocking` since heed/LMDB is synchronous.
pub struct LmdbDatabase {
    env: Arc<Env>,
    path: String,
}

impl LmdbDatabase {
    /// Create or open an LMDB database at the given directory path.
    ///
    /// Creates the directory and all required named databases if they don't exist.
    pub fn new(path: &str) -> Result<Self, DatabaseError> {
        std::fs::create_dir_all(path)
            .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to create LMDB directory {}: {}", path, e)))?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(1024 * 1024 * 1024) // 1 GB
                .max_dbs(8)
                .open(path)
                .map_err(|e| DatabaseError::ConnectionFailed(format!("Failed to open LMDB environment at {}: {}", path, e)))?
        };

        // Create all named databases upfront
        let mut wtxn = env
            .write_txn()
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;

        env.create_database::<Bytes, Bytes>(&mut wtxn, Some("bars"))
            .map_err(|e| DatabaseError::Other(format!("Failed to create bars database: {}", e)))?;
        env.create_database::<Bytes, Bytes>(&mut wtxn, Some("ticks"))
            .map_err(|e| DatabaseError::Other(format!("Failed to create ticks database: {}", e)))?;
        env.create_database::<Bytes, Bytes>(&mut wtxn, Some("orders"))
            .map_err(|e| DatabaseError::Other(format!("Failed to create orders database: {}", e)))?;
        env.create_database::<Bytes, Bytes>(&mut wtxn, Some("trades"))
            .map_err(|e| DatabaseError::Other(format!("Failed to create trades database: {}", e)))?;
        env.create_database::<Bytes, Bytes>(&mut wtxn, Some("positions"))
            .map_err(|e| DatabaseError::Other(format!("Failed to create positions database: {}", e)))?;
        env.create_database::<Bytes, Bytes>(&mut wtxn, Some("events"))
            .map_err(|e| DatabaseError::Other(format!("Failed to create events database: {}", e)))?;

        wtxn.commit()
            .map_err(|e| DatabaseError::Other(format!("Failed to commit initial transaction: {}", e)))?;

        Ok(Self {
            env: Arc::new(env),
            path: path.to_string(),
        })
    }

    /// Get the database path
    pub fn path(&self) -> &str {
        &self.path
    }

    // ---- Key encoding helpers ----

    /// Encode a bar key: exchange\0symbol\0interval\0timestamp_be
    fn bar_key(exchange: &Exchange, symbol: &str, interval: &Interval, ts: i64) -> Vec<u8> {
        let mut key = Vec::with_capacity(64);
        key.extend_from_slice(exchange.value().as_bytes());
        key.push(0);
        key.extend_from_slice(symbol.as_bytes());
        key.push(0);
        key.extend_from_slice(interval.value().as_bytes());
        key.push(0);
        key.extend_from_slice(&ts.to_be_bytes());
        key
    }

    /// Encode a bar prefix for range scans: exchange\0symbol\0interval\0
    fn bar_prefix(exchange: &Exchange, symbol: &str, interval: &Interval) -> Vec<u8> {
        let mut prefix = Vec::with_capacity(48);
        prefix.extend_from_slice(exchange.value().as_bytes());
        prefix.push(0);
        prefix.extend_from_slice(symbol.as_bytes());
        prefix.push(0);
        prefix.extend_from_slice(interval.value().as_bytes());
        prefix.push(0);
        prefix
    }

    /// Encode a tick key: exchange\0symbol\0timestamp_be
    fn tick_key(exchange: &Exchange, symbol: &str, ts: i64) -> Vec<u8> {
        let mut key = Vec::with_capacity(48);
        key.extend_from_slice(exchange.value().as_bytes());
        key.push(0);
        key.extend_from_slice(symbol.as_bytes());
        key.push(0);
        key.extend_from_slice(&ts.to_be_bytes());
        key
    }

    /// Encode a tick prefix for range scans: exchange\0symbol\0
    fn tick_prefix(exchange: &Exchange, symbol: &str) -> Vec<u8> {
        let mut prefix = Vec::with_capacity(32);
        prefix.extend_from_slice(exchange.value().as_bytes());
        prefix.push(0);
        prefix.extend_from_slice(symbol.as_bytes());
        prefix.push(0);
        prefix
    }

    /// Encode an event key: 8-byte big-endian event_id
    fn event_key(event_id: u64) -> Vec<u8> {
        event_id.to_be_bytes().to_vec()
    }

    /// Compute the "prefix end" key for range scans.
    fn prefix_end(prefix: &[u8]) -> Vec<u8> {
        let mut end = prefix.to_vec();
        for i in (0..end.len()).rev() {
            if end[i] < 255 {
                end[i] += 1;
                end.truncate(i + 1);
                return end;
            }
        }
        vec![0xFF; prefix.len() + 1]
    }

    // ---- Deserialization helpers ----

    fn deserialize_bar(value: &[u8]) -> Result<BarData, DatabaseError> {
        serde_json::from_slice(value).map_err(|e| DatabaseError::Other(format!("Failed to deserialize BarData: {}", e)))
    }

    fn deserialize_tick(value: &[u8]) -> Result<TickData, DatabaseError> {
        serde_json::from_slice(value).map_err(|e| DatabaseError::Other(format!("Failed to deserialize TickData: {}", e)))
    }

    fn deserialize_order(value: &[u8]) -> Result<OrderData, DatabaseError> {
        serde_json::from_slice(value).map_err(|e| DatabaseError::Other(format!("Failed to deserialize OrderData: {}", e)))
    }

    fn deserialize_trade(value: &[u8]) -> Result<TradeData, DatabaseError> {
        serde_json::from_slice(value).map_err(|e| DatabaseError::Other(format!("Failed to deserialize TradeData: {}", e)))
    }

    fn deserialize_position(value: &[u8]) -> Result<PositionData, DatabaseError> {
        serde_json::from_slice(value).map_err(|e| DatabaseError::Other(format!("Failed to deserialize PositionData: {}", e)))
    }
}

#[async_trait]
impl BaseDatabase for LmdbDatabase {
    async fn save_bar_data(&self, bars: Vec<BarData>, _stream: bool) -> Result<bool, DatabaseError> {
        let env = self.env.clone();
        tokio::task::spawn_blocking(move || {
            let mut wtxn = env
                .write_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&wtxn, Some("bars"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open bars database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("bars database not found".to_string()))?;

            for bar in &bars {
                let interval = bar.interval.unwrap_or(Interval::Minute);
                let key = Self::bar_key(&bar.exchange, &bar.symbol, &interval, bar.datetime.timestamp());
                let value = serde_json::to_vec(bar)
                    .map_err(|e| DatabaseError::Other(format!("Failed to serialize BarData: {}", e)))?;
                db.put(&mut wtxn, &key, &value)
                    .map_err(|e| DatabaseError::InsertFailed { table: "bars".to_string(), reason: format!("Failed to put bar data: {}", e) })?;
            }

            wtxn.commit()
                .map_err(|e| DatabaseError::Other(format!("Failed to commit bar data: {}", e)))?;
            Ok(true)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_tick_data(&self, ticks: Vec<TickData>, _stream: bool) -> Result<bool, DatabaseError> {
        let env = self.env.clone();
        tokio::task::spawn_blocking(move || {
            let mut wtxn = env
                .write_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&wtxn, Some("ticks"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open ticks database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("ticks database not found".to_string()))?;

            for tick in &ticks {
                let key = Self::tick_key(&tick.exchange, &tick.symbol, tick.datetime.timestamp());
                let value = serde_json::to_vec(tick)
                    .map_err(|e| DatabaseError::Other(format!("Failed to serialize TickData: {}", e)))?;
                db.put(&mut wtxn, &key, &value)
                    .map_err(|e| DatabaseError::InsertFailed { table: "ticks".to_string(), reason: format!("Failed to put tick data: {}", e) })?;
            }

            wtxn.commit()
                .map_err(|e| DatabaseError::Other(format!("Failed to commit tick data: {}", e)))?;
            Ok(true)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, DatabaseError> {
        let env = self.env.clone();
        let symbol = symbol.to_string();
        let start_ts = start.timestamp();
        let end_ts = end.timestamp();

        tokio::task::spawn_blocking(move || {
            let rtxn = env
                .read_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin read transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&rtxn, Some("bars"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open bars database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("bars database not found".to_string()))?;

            let prefix = Self::bar_prefix(&exchange, &symbol, &interval);
            let range_start = {
                let mut s = prefix.clone();
                s.extend_from_slice(&start_ts.to_be_bytes());
                s
            };
            let range_end = Self::prefix_end(&prefix);

            let iter = db
                .range(&rtxn, &(Bound::Included(range_start.as_slice()), Bound::Excluded(range_end.as_slice())))
                .map_err(|e| DatabaseError::Other(format!("Failed to create bar range iterator: {}", e)))?;

            let mut result = Vec::new();
            for item in iter {
                let (key, value) = item.map_err(|e| DatabaseError::Other(format!("Failed to read bar entry: {}", e)))?;
                if let Some(pos) = key.iter().rposition(|&b| b == 0) {
                    let ts_bytes = &key[pos + 1..];
                    if ts_bytes.len() == 8 {
                        let ts = i64::from_be_bytes(ts_bytes.try_into().unwrap_or([0u8; 8]));
                        if ts > end_ts {
                            break;
                        }
                    }
                }
                let bar = Self::deserialize_bar(value)?;
                result.push(bar);
            }

            Ok(result)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, DatabaseError> {
        let env = self.env.clone();
        let symbol = symbol.to_string();
        let start_ts = start.timestamp();
        let end_ts = end.timestamp();

        tokio::task::spawn_blocking(move || {
            let rtxn = env
                .read_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin read transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&rtxn, Some("ticks"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open ticks database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("ticks database not found".to_string()))?;

            let prefix = Self::tick_prefix(&exchange, &symbol);
            let range_start = {
                let mut s = prefix.clone();
                s.extend_from_slice(&start_ts.to_be_bytes());
                s
            };
            let range_end = Self::prefix_end(&prefix);

            let iter = db
                .range(&rtxn, &(Bound::Included(range_start.as_slice()), Bound::Excluded(range_end.as_slice())))
                .map_err(|e| DatabaseError::Other(format!("Failed to create tick range iterator: {}", e)))?;

            let mut result = Vec::new();
            for item in iter {
                let (key, value) = item.map_err(|e| DatabaseError::Other(format!("Failed to read tick entry: {}", e)))?;
                if let Some(pos) = key.iter().rposition(|&b| b == 0) {
                    let ts_bytes = &key[pos + 1..];
                    if ts_bytes.len() == 8 {
                        let ts = i64::from_be_bytes(ts_bytes.try_into().unwrap_or([0u8; 8]));
                        if ts > end_ts {
                            break;
                        }
                    }
                }
                let tick = Self::deserialize_tick(value)?;
                result.push(tick);
            }

            Ok(result)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn delete_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
    ) -> Result<i64, DatabaseError> {
        let env = self.env.clone();
        let symbol = symbol.to_string();

        tokio::task::spawn_blocking(move || {
            let mut wtxn = env
                .write_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&wtxn, Some("bars"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open bars database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("bars database not found".to_string()))?;

            let prefix = Self::bar_prefix(&exchange, &symbol, &interval);
            let range_end = Self::prefix_end(&prefix);

            let deleted = db
                .delete_range(&mut wtxn, &(Bound::Included(prefix.as_slice()), Bound::Excluded(range_end.as_slice())))
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete bar data range: {}", e)))?;

            wtxn.commit()
                .map_err(|e| DatabaseError::Other(format!("Failed to commit delete: {}", e)))?;

            Ok(deleted as i64)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn delete_tick_data(&self, symbol: &str, exchange: Exchange) -> Result<i64, DatabaseError> {
        let env = self.env.clone();
        let symbol = symbol.to_string();

        tokio::task::spawn_blocking(move || {
            let mut wtxn = env
                .write_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&wtxn, Some("ticks"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open ticks database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("ticks database not found".to_string()))?;

            let prefix = Self::tick_prefix(&exchange, &symbol);
            let range_end = Self::prefix_end(&prefix);

            let deleted = db
                .delete_range(&mut wtxn, &(Bound::Included(prefix.as_slice()), Bound::Excluded(range_end.as_slice())))
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete tick data range: {}", e)))?;

            wtxn.commit()
                .map_err(|e| DatabaseError::Other(format!("Failed to commit delete: {}", e)))?;

            Ok(deleted as i64)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn get_bar_overview(&self) -> Result<Vec<BarOverview>, DatabaseError> {
        let env = self.env.clone();

        tokio::task::spawn_blocking(move || {
            let rtxn = env
                .read_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin read transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&rtxn, Some("bars"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open bars database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("bars database not found".to_string()))?;

            let iter = db
                .iter(&rtxn)
                .map_err(|e| DatabaseError::Other(format!("Failed to create bar iterator: {}", e)))?;

            let mut groups: HashMap<(String, Exchange, Interval), Vec<BarData>> = HashMap::new();

            for item in iter {
                let (_key, value) = item.map_err(|e| DatabaseError::Other(format!("Failed to read bar entry: {}", e)))?;
                let bar = Self::deserialize_bar(value)?;
                let interval = bar.interval.unwrap_or(Interval::Minute);
                let key = (bar.symbol.clone(), bar.exchange, interval);
                groups.entry(key).or_default().push(bar);
            }

            let mut overviews = Vec::new();
            for ((symbol, exchange, interval), bars) in groups {
                let count = bars.len() as i64;
                let start = bars.iter().map(|b| b.datetime).min();
                let end = bars.iter().map(|b| b.datetime).max();
                overviews.push(BarOverview {
                    symbol,
                    exchange: Some(exchange),
                    interval: Some(interval),
                    count,
                    start,
                    end,
                });
            }

            Ok(overviews)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn get_tick_overview(&self) -> Result<Vec<TickOverview>, DatabaseError> {
        let env = self.env.clone();

        tokio::task::spawn_blocking(move || {
            let rtxn = env
                .read_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin read transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&rtxn, Some("ticks"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open ticks database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("ticks database not found".to_string()))?;

            let iter = db
                .iter(&rtxn)
                .map_err(|e| DatabaseError::Other(format!("Failed to create tick iterator: {}", e)))?;

            let mut groups: HashMap<(String, Exchange), Vec<TickData>> = HashMap::new();

            for item in iter {
                let (_key, value) = item.map_err(|e| DatabaseError::Other(format!("Failed to read tick entry: {}", e)))?;
                let tick = Self::deserialize_tick(value)?;
                let key = (tick.symbol.clone(), tick.exchange);
                groups.entry(key).or_default().push(tick);
            }

            let mut overviews = Vec::new();
            for ((symbol, exchange), ticks) in groups {
                let count = ticks.len() as i64;
                let start = ticks.iter().map(|t| t.datetime).min();
                let end = ticks.iter().map(|t| t.datetime).max();
                overviews.push(TickOverview {
                    symbol,
                    exchange: Some(exchange),
                    count,
                    start,
                    end,
                });
            }

            Ok(overviews)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_order_data(&self, orders: Vec<OrderData>) -> Result<bool, DatabaseError> {
        let env = self.env.clone();
        tokio::task::spawn_blocking(move || {
            let mut wtxn = env
                .write_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&wtxn, Some("orders"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open orders database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("orders database not found".to_string()))?;

            for order in &orders {
                let key = order.vt_orderid();
                let value = serde_json::to_vec(order)
                    .map_err(|e| DatabaseError::Other(format!("Failed to serialize OrderData: {}", e)))?;
                db.put(&mut wtxn, key.as_bytes(), &value)
                    .map_err(|e| DatabaseError::InsertFailed { table: "orders".to_string(), reason: format!("Failed to put order data: {}", e) })?;
            }

            wtxn.commit()
                .map_err(|e| DatabaseError::Other(format!("Failed to commit order data: {}", e)))?;
            Ok(true)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_trade_data(&self, trades: Vec<TradeData>) -> Result<bool, DatabaseError> {
        let env = self.env.clone();
        tokio::task::spawn_blocking(move || {
            let mut wtxn = env
                .write_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&wtxn, Some("trades"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open trades database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("trades database not found".to_string()))?;

            for trade in &trades {
                let key = trade.vt_tradeid();
                let value = serde_json::to_vec(trade)
                    .map_err(|e| DatabaseError::Other(format!("Failed to serialize TradeData: {}", e)))?;
                db.put(&mut wtxn, key.as_bytes(), &value)
                    .map_err(|e| DatabaseError::InsertFailed { table: "trades".to_string(), reason: format!("Failed to put trade data: {}", e) })?;
            }

            wtxn.commit()
                .map_err(|e| DatabaseError::Other(format!("Failed to commit trade data: {}", e)))?;
            Ok(true)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_position_data(&self, positions: Vec<PositionData>) -> Result<bool, DatabaseError> {
        let env = self.env.clone();
        tokio::task::spawn_blocking(move || {
            let mut wtxn = env
                .write_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&wtxn, Some("positions"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open positions database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("positions database not found".to_string()))?;

            for position in &positions {
                let key = position.vt_positionid();
                let value = serde_json::to_vec(position)
                    .map_err(|e| DatabaseError::Other(format!("Failed to serialize PositionData: {}", e)))?;
                db.put(&mut wtxn, key.as_bytes(), &value)
                    .map_err(|e| DatabaseError::InsertFailed { table: "positions".to_string(), reason: format!("Failed to put position data: {}", e) })?;
            }

            wtxn.commit()
                .map_err(|e| DatabaseError::Other(format!("Failed to commit position data: {}", e)))?;
            Ok(true)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_event(&self, event: EventRecord) -> Result<bool, DatabaseError> {
        let env = self.env.clone();
        tokio::task::spawn_blocking(move || {
            let mut wtxn = env
                .write_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin write transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&wtxn, Some("events"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open events database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("events database not found".to_string()))?;

            let key = Self::event_key(event.event_id);
            let value = serde_json::to_vec(&event)
                .map_err(|e| DatabaseError::Other(format!("Failed to serialize EventRecord: {}", e)))?;
            db.put(&mut wtxn, &key, &value)
                .map_err(|e| DatabaseError::InsertFailed { table: "events".to_string(), reason: format!("Failed to put event data: {}", e) })?;

            wtxn.commit()
                .map_err(|e| DatabaseError::Other(format!("Failed to commit event data: {}", e)))?;
            Ok(true)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_orders(&self, gateway_name: Option<&str>) -> Result<Vec<OrderData>, DatabaseError> {
        let env = self.env.clone();
        let gateway_name = gateway_name.map(String::from);

        tokio::task::spawn_blocking(move || {
            let rtxn = env
                .read_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin read transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&rtxn, Some("orders"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open orders database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("orders database not found".to_string()))?;

            let iter = db
                .iter(&rtxn)
                .map_err(|e| DatabaseError::Other(format!("Failed to create orders iterator: {}", e)))?;

            let mut result = Vec::new();
            for item in iter {
                let (_key, value) = item.map_err(|e| DatabaseError::Other(format!("Failed to read order entry: {}", e)))?;
                let order = Self::deserialize_order(value)?;
                if gateway_name.as_ref().is_none_or(|gw| order.gateway_name == *gw) {
                    result.push(order);
                }
            }

            Ok(result)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_trades(&self, gateway_name: Option<&str>) -> Result<Vec<TradeData>, DatabaseError> {
        let env = self.env.clone();
        let gateway_name = gateway_name.map(String::from);

        tokio::task::spawn_blocking(move || {
            let rtxn = env
                .read_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin read transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&rtxn, Some("trades"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open trades database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("trades database not found".to_string()))?;

            let iter = db
                .iter(&rtxn)
                .map_err(|e| DatabaseError::Other(format!("Failed to create trades iterator: {}", e)))?;

            let mut result = Vec::new();
            for item in iter {
                let (_key, value) = item.map_err(|e| DatabaseError::Other(format!("Failed to read trade entry: {}", e)))?;
                let trade = Self::deserialize_trade(value)?;
                if gateway_name.as_ref().is_none_or(|gw| trade.gateway_name == *gw) {
                    result.push(trade);
                }
            }

            Ok(result)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_positions(&self, gateway_name: Option<&str>) -> Result<Vec<PositionData>, DatabaseError> {
        let env = self.env.clone();
        let gateway_name = gateway_name.map(String::from);

        tokio::task::spawn_blocking(move || {
            let rtxn = env
                .read_txn()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin read transaction: {}", e)))?;
            let db: Database<Bytes, Bytes> = env
                .open_database(&rtxn, Some("positions"))
                .map_err(|e| DatabaseError::Other(format!("Failed to open positions database: {}", e)))?
                .ok_or_else(|| DatabaseError::Other("positions database not found".to_string()))?;

            let iter = db
                .iter(&rtxn)
                .map_err(|e| DatabaseError::Other(format!("Failed to create positions iterator: {}", e)))?;

            let mut result = Vec::new();
            for item in iter {
                let (_key, value) = item.map_err(|e| DatabaseError::Other(format!("Failed to read position entry: {}", e)))?;
                let position = Self::deserialize_position(value)?;
                if gateway_name.as_ref().is_none_or(|gw| position.gateway_name == *gw) {
                    result.push(position);
                }
            }

            Ok(result)
        })
        .await
        .map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::{OrderType, Direction, Offset, Status};

    #[tokio::test]
    async fn test_lmdb_database_bars() {
        let temp_dir = std::env::temp_dir().join("trade_engine_test_lmdb_bars");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let db = LmdbDatabase::new(temp_dir.to_str().unwrap()).expect("Failed to create LmdbDatabase");

        let bar = BarData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );

        db.save_bar_data(vec![bar], false).await.expect("save_bar_data should succeed");

        let overviews = db.get_bar_overview().await.expect("get_bar_overview should succeed");
        assert_eq!(overviews.len(), 1);
        assert_eq!(overviews[0].symbol, "BTCUSDT");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lmdb_database_ticks() {
        let temp_dir = std::env::temp_dir().join("trade_engine_test_lmdb_ticks");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let db = LmdbDatabase::new(temp_dir.to_str().unwrap()).expect("Failed to create LmdbDatabase");

        let tick = TickData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );

        db.save_tick_data(vec![tick], false).await.expect("save_tick_data should succeed");

        let overviews = db.get_tick_overview().await.expect("get_tick_overview should succeed");
        assert_eq!(overviews.len(), 1);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lmdb_database_bar_load_and_delete() {
        let temp_dir = std::env::temp_dir().join("trade_engine_test_lmdb_bar_ops");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let db = LmdbDatabase::new(temp_dir.to_str().unwrap()).expect("Failed to create LmdbDatabase");

        let now = Utc::now();
        let bar = BarData {
            gateway_name: "test".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: now,
            interval: Some(Interval::Minute),
            volume: 100.0,
            turnover: 1000000.0,
            open_interest: 0.0,
            open_price: 50000.0,
            high_price: 50100.0,
            low_price: 49900.0,
            close_price: 50050.0,
            extra: None,
        };

        db.save_bar_data(vec![bar], false).await.expect("save should succeed");

        let start = now - chrono::Duration::hours(1);
        let end = now + chrono::Duration::hours(1);
        let bars = db.load_bar_data("BTCUSDT", Exchange::Binance, Interval::Minute, start, end)
            .await.expect("load should succeed");
        assert_eq!(bars.len(), 1);

        let deleted = db.delete_bar_data("BTCUSDT", Exchange::Binance, Interval::Minute)
            .await.expect("delete should succeed");
        assert_eq!(deleted, 1);

        let bars_after = db.load_bar_data("BTCUSDT", Exchange::Binance, Interval::Minute, start, end)
            .await.expect("load after delete should succeed");
        assert_eq!(bars_after.len(), 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lmdb_database_orders() {
        let temp_dir = std::env::temp_dir().join("trade_engine_test_lmdb_orders");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let db = LmdbDatabase::new(temp_dir.to_str().unwrap()).expect("Failed to create LmdbDatabase");

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
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        db.save_order_data(vec![order.clone()]).await.expect("save should succeed");

        let loaded = db.load_orders(None).await.expect("load should succeed");
        assert_eq!(loaded.len(), 1);

        let filtered = db.load_orders(Some("BINANCE_SPOT")).await.expect("load filtered should succeed");
        assert_eq!(filtered.len(), 1);

        let other = db.load_orders(Some("OTHER")).await.expect("load other should succeed");
        assert_eq!(other.len(), 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_lmdb_database_events() {
        let temp_dir = std::env::temp_dir().join("trade_engine_test_lmdb_events");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let db = LmdbDatabase::new(temp_dir.to_str().unwrap()).expect("Failed to create LmdbDatabase");

        let event = EventRecord::new(1, "eOrder".to_string(), "BINANCE_SPOT".to_string(), "{}".to_string());
        db.save_event(event).await.expect("save_event should succeed");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
