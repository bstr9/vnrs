//! SQLite database implementation for local-first data persistence.
//!
//! Provides a single-file SQLite database backend for storing bars, ticks,
//! orders, trades, positions, and events. Suitable for local development
//! and single-machine deployment.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};

use crate::error::DatabaseError;
use super::constant::{Direction, Exchange, Interval, Offset, OrderType, Status};
use super::database::{BaseDatabase, BarOverview, TickOverview, EventRecord};
use super::object::{BarData, TickData, OrderData, TradeData, PositionData, DepthData};

/// SQLite database implementation
///
/// Stores all data in a single SQLite file. Thread-safe via `Arc<Mutex<Connection>>`.
/// All operations are wrapped in `spawn_blocking` since rusqlite is synchronous.
/// The mutex is locked *inside* `spawn_blocking` to avoid `MutexGuard` not being `Send`.
pub struct SqliteDatabase {
    conn: Arc<Mutex<Connection>>,
    path: String,
}

impl SqliteDatabase {
    /// Create or open a SQLite database at the given path
    ///
    /// Creates the database file and all required tables if they don't exist.
    pub fn new(path: &str) -> Result<Self, String> {
        let conn = Connection::open(path)
            .map_err(|e| format!("Failed to open SQLite database at {}: {}", path, e))?;

        // Create tables
        Self::create_tables(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: path.to_string(),
        })
    }

    /// Create an in-memory SQLite database (useful for testing)
    pub fn new_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("Failed to create in-memory SQLite database: {}", e))?;

        Self::create_tables(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: ":memory:".to_string(),
        })
    }

    /// Get the database path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Parse an exchange string (from Debug format) back to Exchange enum
    fn parse_exchange(s: &str) -> Exchange {
        match s {
            "Cffex" => Exchange::Cffex,
            "Shfe" => Exchange::Shfe,
            "Czce" => Exchange::Czce,
            "Dce" => Exchange::Dce,
            "Ine" => Exchange::Ine,
            "Gfex" => Exchange::Gfex,
            "Sse" => Exchange::Sse,
            "Szse" => Exchange::Szse,
            "Bse" => Exchange::Bse,
            "Shhk" => Exchange::Shhk,
            "Szhk" => Exchange::Szhk,
            "Sge" => Exchange::Sge,
            "Wxe" => Exchange::Wxe,
            "Cfets" => Exchange::Cfets,
            "Xbond" => Exchange::Xbond,
            "Smart" => Exchange::Smart,
            "Nyse" => Exchange::Nyse,
            "Nasdaq" => Exchange::Nasdaq,
            "Arca" => Exchange::Arca,
            "Edgea" => Exchange::Edgea,
            "Island" => Exchange::Island,
            "Bats" => Exchange::Bats,
            "Iex" => Exchange::Iex,
            "Amex" => Exchange::Amex,
            "Tse" => Exchange::Tse,
            "Nymex" => Exchange::Nymex,
            "Comex" => Exchange::Comex,
            "Globex" => Exchange::Globex,
            "Idealpro" => Exchange::Idealpro,
            "Cme" => Exchange::Cme,
            "Ice" => Exchange::Ice,
            "Sehk" => Exchange::Sehk,
            "Hkfe" => Exchange::Hkfe,
            "Sgx" => Exchange::Sgx,
            "Cbot" => Exchange::Cbot,
            "Cboe" => Exchange::Cboe,
            "Cfe" => Exchange::Cfe,
            "Dme" => Exchange::Dme,
            "Eurex" => Exchange::Eurex,
            "Apex" => Exchange::Apex,
            "Lme" => Exchange::Lme,
            "Bmd" => Exchange::Bmd,
            "Tocom" => Exchange::Tocom,
            "Eunx" => Exchange::Eunx,
            "Krx" => Exchange::Krx,
            "Otc" => Exchange::Otc,
            "Ibkrats" => Exchange::Ibkrats,
            "Binance" => Exchange::Binance,
            "BinanceUsdm" => Exchange::BinanceUsdm,
            "BinanceCoinm" => Exchange::BinanceCoinm,
            "Okx" => Exchange::Okx,
            "Bybit" => Exchange::Bybit,
            "Local" => Exchange::Local,
            _ => Exchange::Global,
        }
    }

    /// Parse a direction string (from Debug format) back to Direction enum
    fn parse_direction(s: &str) -> Option<Direction> {
        match s {
            "Long" => Some(Direction::Long),
            "Short" => Some(Direction::Short),
            "Net" => Some(Direction::Net),
            _ => None,
        }
    }

    /// Parse an offset string (from Debug format) back to Offset enum
    fn parse_offset(s: &str) -> Offset {
        match s {
            "None" => Offset::None,
            "Open" => Offset::Open,
            "Close" => Offset::Close,
            "CloseToday" => Offset::CloseToday,
            "CloseYesterday" => Offset::CloseYesterday,
            _ => Offset::None,
        }
    }

    /// Parse an order type string (from Debug format) back to OrderType enum
    fn parse_order_type(s: &str) -> OrderType {
        match s {
            "Limit" => OrderType::Limit,
            "Market" => OrderType::Market,
            "Stop" => OrderType::Stop,
            "StopLimit" => OrderType::StopLimit,
            "Fak" => OrderType::Fak,
            "Fok" => OrderType::Fok,
            "Rfq" => OrderType::Rfq,
            "Etf" => OrderType::Etf,
            _ => OrderType::Limit,
        }
    }

    /// Parse a status string (from Debug format) back to Status enum
    fn parse_status(s: &str) -> Status {
        match s {
            "Submitting" => Status::Submitting,
            "NotTraded" => Status::NotTraded,
            "PartTraded" => Status::PartTraded,
            "AllTraded" => Status::AllTraded,
            "Cancelled" => Status::Cancelled,
            "Rejected" => Status::Rejected,
            _ => Status::Submitting,
        }
    }

    /// Convert a database row to OrderData
    fn row_to_order(row: &rusqlite::Row<'_>) -> rusqlite::Result<OrderData> {
        let exchange_str: String = row.get(2)?;
        let direction_str: String = row.get(3)?;
        let order_type_str: String = row.get(4)?;
        let offset_str: String = row.get(5)?;
        let status_str: String = row.get(9)?;
        let datetime_str: String = row.get(10)?;
        let post_only_val: i32 = row.get(13)?;
        let reduce_only_val: i32 = row.get(14)?;

        let datetime = if datetime_str.is_empty() {
            None
        } else {
            DateTime::parse_from_rfc3339(&datetime_str)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        };

        Ok(OrderData {
            orderid: row.get(0)?,
            symbol: row.get(1)?,
            exchange: Self::parse_exchange(&exchange_str),
            direction: Self::parse_direction(&direction_str),
            order_type: Self::parse_order_type(&order_type_str),
            offset: Self::parse_offset(&offset_str),
            price: row.get(6)?,
            volume: row.get(7)?,
            traded: row.get(8)?,
            status: Self::parse_status(&status_str),
            datetime,
            reference: row.get(11)?,
            gateway_name: row.get(12)?,
            post_only: post_only_val != 0,
            reduce_only: reduce_only_val != 0,
            expire_time: None,
            extra: None,
        })
    }

    /// Convert a database row to TradeData
    fn row_to_trade(row: &rusqlite::Row<'_>) -> rusqlite::Result<TradeData> {
        let exchange_str: String = row.get(3)?;
        let direction_str: String = row.get(4)?;
        let offset_str: String = row.get(5)?;
        let datetime_str: String = row.get(8)?;

        let datetime = if datetime_str.is_empty() {
            None
        } else {
            DateTime::parse_from_rfc3339(&datetime_str)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        };

        Ok(TradeData {
            tradeid: row.get(0)?,
            orderid: row.get(1)?,
            symbol: row.get(2)?,
            exchange: Self::parse_exchange(&exchange_str),
            direction: Self::parse_direction(&direction_str),
            offset: Self::parse_offset(&offset_str),
            price: row.get(6)?,
            volume: row.get(7)?,
            datetime,
            gateway_name: row.get(9)?,
            extra: None,
        })
    }

    /// Convert a database row to PositionData
    fn row_to_position(row: &rusqlite::Row<'_>) -> rusqlite::Result<PositionData> {
        let exchange_str: String = row.get(1)?;
        let direction_str: String = row.get(2)?;

        Ok(PositionData {
            symbol: row.get(0)?,
            exchange: Self::parse_exchange(&exchange_str),
            direction: Self::parse_direction(&direction_str).unwrap_or(Direction::Net),
            volume: row.get(3)?,
            frozen: row.get(4)?,
            price: row.get(5)?,
            pnl: row.get(6)?,
            yd_volume: row.get(7)?,
            gateway_name: row.get(8)?,
            extra: None,
        })
    }

    fn create_tables(conn: &Connection) -> Result<(), String> {
        // Bar data table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS dbbardata (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL,
                exchange TEXT NOT NULL,
                interval TEXT NOT NULL,
                datetime TEXT NOT NULL,
                open_price REAL NOT NULL,
                high_price REAL NOT NULL,
                low_price REAL NOT NULL,
                close_price REAL NOT NULL,
                volume REAL NOT NULL,
                turnover REAL NOT NULL DEFAULT 0,
                open_interest REAL NOT NULL DEFAULT 0,
                gateway_name TEXT NOT NULL DEFAULT '',
                UNIQUE(symbol, exchange, interval, datetime)
            )
            "#,
            [],
        ).map_err(|e| format!("Failed to create dbbardata table: {}", e))?;

        // Create index for faster queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_bar_datetime ON dbbardata(symbol, exchange, interval, datetime)",
            [],
        ).map_err(|e| format!("Failed to create bar datetime index: {}", e))?;

        // Tick data table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS dbtickdata (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL,
                exchange TEXT NOT NULL,
                datetime TEXT NOT NULL,
                last_price REAL NOT NULL,
                last_volume REAL NOT NULL DEFAULT 0,
                volume REAL NOT NULL DEFAULT 0,
                turnover REAL NOT NULL DEFAULT 0,
                open_interest REAL NOT NULL DEFAULT 0,
                bid_price_1 REAL NOT NULL DEFAULT 0,
                bid_volume_1 REAL NOT NULL DEFAULT 0,
                ask_price_1 REAL NOT NULL DEFAULT 0,
                ask_volume_1 REAL NOT NULL DEFAULT 0,
                gateway_name TEXT NOT NULL DEFAULT '',
                UNIQUE(symbol, exchange, datetime)
            )
            "#,
            [],
        ).map_err(|e| format!("Failed to create dbtickdata table: {}", e))?;

        // Create index for faster tick queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tick_datetime ON dbtickdata(symbol, exchange, datetime)",
            [],
        ).map_err(|e| format!("Failed to create tick datetime index: {}", e))?;

        // Order data table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS dborderdata (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                orderid TEXT NOT NULL,
                symbol TEXT NOT NULL,
                exchange TEXT NOT NULL,
                direction TEXT,
                order_type TEXT NOT NULL,
                offset TEXT NOT NULL,
                price REAL NOT NULL,
                volume REAL NOT NULL,
                traded REAL NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                datetime TEXT,
                reference TEXT NOT NULL DEFAULT '',
                gateway_name TEXT NOT NULL,
                UNIQUE(gateway_name, orderid)
            )
            "#,
            [],
        ).map_err(|e| format!("Failed to create dborderdata table: {}", e))?;

        // Trade data table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS dbtradedata (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                tradeid TEXT NOT NULL,
                orderid TEXT NOT NULL,
                symbol TEXT NOT NULL,
                exchange TEXT NOT NULL,
                direction TEXT,
                offset TEXT NOT NULL,
                price REAL NOT NULL,
                volume REAL NOT NULL,
                datetime TEXT,
                gateway_name TEXT NOT NULL,
                UNIQUE(gateway_name, tradeid)
            )
            "#,
            [],
        ).map_err(|e| format!("Failed to create dbtradedata table: {}", e))?;

        // Position data table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS dbpositiondata (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL,
                exchange TEXT NOT NULL,
                direction TEXT NOT NULL,
                volume REAL NOT NULL,
                frozen REAL NOT NULL DEFAULT 0,
                price REAL NOT NULL,
                pnl REAL NOT NULL DEFAULT 0,
                yd_volume REAL NOT NULL DEFAULT 0,
                gateway_name TEXT NOT NULL,
                UNIQUE(gateway_name, symbol, exchange, direction)
            )
            "#,
            [],
        ).map_err(|e| format!("Failed to create dbpositiondata table: {}", e))?;

        // Event records table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS dbeventdata (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_id INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                gateway_name TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                payload TEXT NOT NULL,
                UNIQUE(event_id, event_type, gateway_name)
            )
            "#,
            [],
        ).map_err(|e| format!("Failed to create dbeventdata table: {}", e))?;

        // Add post_only and reduce_only columns if missing (migration for existing databases)
        let _ = conn.execute("ALTER TABLE dborderdata ADD COLUMN post_only INTEGER NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE dborderdata ADD COLUMN reduce_only INTEGER NOT NULL DEFAULT 0", []);

        // Depth data table
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS dbdepthdata (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                symbol TEXT NOT NULL,
                exchange TEXT NOT NULL,
                datetime TEXT NOT NULL,
                bids_json TEXT NOT NULL DEFAULT '{}',
                asks_json TEXT NOT NULL DEFAULT '{}',
                gateway_name TEXT NOT NULL DEFAULT '',
                UNIQUE(symbol, exchange, datetime)
            )
            "#,
            [],
        ).map_err(|e| format!("Failed to create dbdepthdata table: {}", e))?;

        // Create index for faster depth queries
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_depth_datetime ON dbdepthdata(symbol, exchange, datetime)",
            [],
        ).map_err(|e| format!("Failed to create depth datetime index: {}", e))?;

        Ok(())
    }
}

#[async_trait]
impl BaseDatabase for SqliteDatabase {
    async fn save_bar_data(&self, bars: Vec<BarData>, _stream: bool) -> Result<bool, DatabaseError> {
        if bars.is_empty() {
            return Ok(true);
        }

        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let tx = conn.transaction()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin transaction: {}", e)))?;

            for bar in bars {
                let exchange_str = format!("{:?}", bar.exchange);
                let interval_str = bar.interval.map(|i| format!("{:?}", i)).unwrap_or_else(|| "Minute".to_string());
                let datetime_str = bar.datetime.to_rfc3339();

                tx.execute(
                    r#"
                    INSERT OR REPLACE INTO dbbardata 
                    (symbol, exchange, interval, datetime, open_price, high_price, low_price, close_price, 
                     volume, turnover, open_interest, gateway_name)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                    "#,
                    params![
                        bar.symbol,
                        exchange_str,
                        interval_str,
                        datetime_str,
                        bar.open_price,
                        bar.high_price,
                        bar.low_price,
                        bar.close_price,
                        bar.volume,
                        bar.turnover,
                        bar.open_interest,
                        bar.gateway_name,
                    ],
                ).map_err(|e| DatabaseError::InsertFailed { table: "dbbardata".to_string(), reason: e.to_string() })?;
            }

            tx.commit().map_err(|e| DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e)))?;
            Ok(true)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_tick_data(&self, ticks: Vec<TickData>, _stream: bool) -> Result<bool, DatabaseError> {
        if ticks.is_empty() {
            return Ok(true);
        }

        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let tx = conn.transaction()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin transaction: {}", e)))?;

            for tick in ticks {
                let exchange_str = format!("{:?}", tick.exchange);
                let datetime_str = tick.datetime.to_rfc3339();

                tx.execute(
                    r#"
                    INSERT OR REPLACE INTO dbtickdata 
                    (symbol, exchange, datetime, last_price, last_volume, volume, turnover, open_interest,
                     bid_price_1, bid_volume_1, ask_price_1, ask_volume_1, gateway_name)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                    "#,
                    params![
                        tick.symbol,
                        exchange_str,
                        datetime_str,
                        tick.last_price,
                        tick.last_volume,
                        tick.volume,
                        tick.turnover,
                        tick.open_interest,
                        tick.bid_price_1,
                        tick.bid_volume_1,
                        tick.ask_price_1,
                        tick.ask_volume_1,
                        tick.gateway_name,
                    ],
                ).map_err(|e| DatabaseError::InsertFailed { table: "dbtickdata".to_string(), reason: e.to_string() })?;
            }

            tx.commit().map_err(|e| DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e)))?;
            Ok(true)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, DatabaseError> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);
        let interval_str = format!("{:?}", interval);
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, interval, datetime, open_price, high_price, low_price, close_price,
                       volume, turnover, open_interest, gateway_name
                FROM dbbardata
                WHERE symbol = ?1 AND exchange = ?2 AND interval = ?3 
                      AND datetime >= ?4 AND datetime <= ?5
                ORDER BY datetime ASC
                "#
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e)))?;

            let bars = stmt.query_map(
                params![symbol, exchange_str, interval_str, start_str, end_str],
                |row| {
                    let datetime_str: String = row.get(3)?;
                    let datetime = DateTime::parse_from_rfc3339(&datetime_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());

                    let exchange_str: String = row.get(1)?;
                    let exchange = match exchange_str.as_str() {
                        "Binance" => Exchange::Binance,
                        "OKX" => Exchange::Okx,
                        "Bybit" => Exchange::Bybit,
                        _ => Exchange::Local,
                    };

                    let interval_str: String = row.get(2)?;
                    let interval = match interval_str.as_str() {
                        "Minute" => Interval::Minute,
                        "Minute5" => Interval::Minute5,
                        "Minute15" => Interval::Minute15,
                        "Hour" => Interval::Hour,
                        "Daily" => Interval::Daily,
                        _ => Interval::Minute,
                    };

                    Ok(BarData {
                        gateway_name: row.get(11)?,
                        symbol: row.get(0)?,
                        exchange,
                        datetime,
                        interval: Some(interval),
                        open_price: row.get(4)?,
                        high_price: row.get(5)?,
                        low_price: row.get(6)?,
                        close_price: row.get(7)?,
                        volume: row.get(8)?,
                        turnover: row.get(9)?,
                        open_interest: row.get(10)?,
                        extra: None,
                    })
                },
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query bars: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect bars: {}", e)))?;

            Ok(bars)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, DatabaseError> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, datetime, last_price, last_volume, volume, turnover, open_interest,
                       bid_price_1, bid_volume_1, ask_price_1, ask_volume_1, gateway_name
                FROM dbtickdata
                WHERE symbol = ?1 AND exchange = ?2 AND datetime >= ?3 AND datetime <= ?4
                ORDER BY datetime ASC
                "#
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e)))?;

            let ticks = stmt.query_map(
                params![symbol, exchange_str, start_str, end_str],
                |row| {
                    let datetime_str: String = row.get(2)?;
                    let datetime = DateTime::parse_from_rfc3339(&datetime_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());

                    let exchange_str: String = row.get(1)?;
                    let exchange = match exchange_str.as_str() {
                        "Binance" => Exchange::Binance,
                        "OKX" => Exchange::Okx,
                        "Bybit" => Exchange::Bybit,
                        _ => Exchange::Local,
                    };

                    Ok(TickData {
                        gateway_name: row.get(12)?,
                        symbol: row.get(0)?,
                        exchange,
                        datetime,
                        name: String::new(),
                        volume: row.get(5)?,
                        turnover: row.get(6)?,
                        open_interest: row.get(7)?,
                        last_price: row.get(3)?,
                        last_volume: row.get(4)?,
                        limit_up: 0.0,
                        limit_down: 0.0,
                        open_price: 0.0,
                        high_price: 0.0,
                        low_price: 0.0,
                        pre_close: 0.0,
                        bid_price_1: row.get(8)?,
                        bid_price_2: 0.0,
                        bid_price_3: 0.0,
                        bid_price_4: 0.0,
                        bid_price_5: 0.0,
                        ask_price_1: row.get(10)?,
                        ask_price_2: 0.0,
                        ask_price_3: 0.0,
                        ask_price_4: 0.0,
                        ask_price_5: 0.0,
                        bid_volume_1: row.get(9)?,
                        bid_volume_2: 0.0,
                        bid_volume_3: 0.0,
                        bid_volume_4: 0.0,
                        bid_volume_5: 0.0,
                        ask_volume_1: row.get(11)?,
                        ask_volume_2: 0.0,
                        ask_volume_3: 0.0,
                        ask_volume_4: 0.0,
                        ask_volume_5: 0.0,
                        localtime: None,
                        extra: None,
                    })
                },
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query ticks: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect ticks: {}", e)))?;

            Ok(ticks)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_depth_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<DepthData>, DatabaseError> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, datetime, bids_json, asks_json, gateway_name
                FROM dbdepthdata
                WHERE symbol = ?1 AND exchange = ?2 AND datetime >= ?3 AND datetime <= ?4
                ORDER BY datetime ASC
                "#
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e)))?;

            let depths = stmt.query_map(
                params![symbol, exchange_str, start_str, end_str],
                |row| {
                    let datetime_str: String = row.get(2)?;
                    let datetime = DateTime::parse_from_rfc3339(&datetime_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now());

                    let exchange_str: String = row.get(1)?;
                    let exchange = Self::parse_exchange(&exchange_str);

                    let bids_json: String = row.get(3)?;
                    let asks_json: String = row.get(4)?;

                    let bids: std::collections::BTreeMap<rust_decimal::Decimal, rust_decimal::Decimal> =
                        serde_json::from_str(&bids_json).unwrap_or_default();
                    let asks: std::collections::BTreeMap<rust_decimal::Decimal, rust_decimal::Decimal> =
                        serde_json::from_str(&asks_json).unwrap_or_default();

                    Ok(DepthData {
                        gateway_name: row.get(5)?,
                        symbol: row.get(0)?,
                        exchange,
                        datetime,
                        bids,
                        asks,
                        extra: None,
                    })
                },
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query depths: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect depths: {}", e)))?;

            Ok(depths)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn delete_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
    ) -> Result<i64, DatabaseError> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);
        let interval_str = format!("{:?}", interval);

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM dbbardata WHERE symbol = ?1 AND exchange = ?2 AND interval = ?3",
                params![symbol, exchange_str, interval_str],
                |row| row.get(0),
            ).unwrap_or(0);

            conn.execute(
                "DELETE FROM dbbardata WHERE symbol = ?1 AND exchange = ?2 AND interval = ?3",
                params![symbol, exchange_str, interval_str],
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete bars: {}", e)))?;

            Ok(count)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn delete_tick_data(&self, symbol: &str, exchange: Exchange) -> Result<i64, DatabaseError> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM dbtickdata WHERE symbol = ?1 AND exchange = ?2",
                params![symbol, exchange_str],
                |row| row.get(0),
            ).unwrap_or(0);

            conn.execute(
                "DELETE FROM dbtickdata WHERE symbol = ?1 AND exchange = ?2",
                params![symbol, exchange_str],
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to delete ticks: {}", e)))?;

            Ok(count)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn get_bar_overview(&self) -> Result<Vec<BarOverview>, DatabaseError> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, interval, COUNT(*) as count, MIN(datetime) as start, MAX(datetime) as end
                FROM dbbardata
                GROUP BY symbol, exchange, interval
                "#
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e)))?;

            let overviews = stmt.query_map([], |row| {
                let exchange_str: String = row.get(1)?;
                let exchange = match exchange_str.as_str() {
                    "Binance" => Some(Exchange::Binance),
                    "OKX" => Some(Exchange::Okx),
                    "Bybit" => Some(Exchange::Bybit),
                    _ => Some(Exchange::Local),
                };

                let interval_str: String = row.get(2)?;
                let interval = match interval_str.as_str() {
                    "Minute" => Some(Interval::Minute),
                    "Minute5" => Some(Interval::Minute5),
                    "Minute15" => Some(Interval::Minute15),
                    "Hour" => Some(Interval::Hour),
                    "Daily" => Some(Interval::Daily),
                    _ => Some(Interval::Minute),
                };

                let start_str: Option<String> = row.get(4)?;
                let end_str: Option<String> = row.get(5)?;

                let start = start_str.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
                });

                let end = end_str.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
                });

                Ok(BarOverview {
                    symbol: row.get(0)?,
                    exchange,
                    interval,
                    count: row.get(3)?,
                    start,
                    end,
                })
            }).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query bar overview: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect bar overview: {}", e)))?;

            Ok(overviews)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn get_tick_overview(&self) -> Result<Vec<TickOverview>, DatabaseError> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, COUNT(*) as count, MIN(datetime) as start, MAX(datetime) as end
                FROM dbtickdata
                GROUP BY symbol, exchange
                "#
            ).map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e)))?;

            let overviews = stmt.query_map([], |row| {
                let exchange_str: String = row.get(1)?;
                let exchange = match exchange_str.as_str() {
                    "Binance" => Some(Exchange::Binance),
                    "OKX" => Some(Exchange::Okx),
                    "Bybit" => Some(Exchange::Bybit),
                    _ => Some(Exchange::Local),
                };

                let start_str: Option<String> = row.get(3)?;
                let end_str: Option<String> = row.get(4)?;

                let start = start_str.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
                });

                let end = end_str.and_then(|s| {
                    DateTime::parse_from_rfc3339(&s).ok().map(|dt| dt.with_timezone(&Utc))
                });

                Ok(TickOverview {
                    symbol: row.get(0)?,
                    exchange,
                    count: row.get(2)?,
                    start,
                    end,
                })
            }).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query tick overview: {}", e)))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect tick overview: {}", e)))?;

            Ok(overviews)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_order_data(&self, orders: Vec<OrderData>) -> Result<bool, DatabaseError> {
        if orders.is_empty() {
            return Ok(true);
        }

        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let tx = conn.transaction()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin transaction: {}", e)))?;

            for order in orders {
                let exchange_str = format!("{:?}", order.exchange);
                let direction_str = order.direction.map(|d| format!("{:?}", d)).unwrap_or_default();
                let order_type_str = format!("{:?}", order.order_type);
                let offset_str = format!("{:?}", order.offset);
                let status_str = format!("{:?}", order.status);
                let datetime_str = order.datetime.map(|dt| dt.to_rfc3339()).unwrap_or_default();

                tx.execute(
                    "INSERT OR REPLACE INTO dborderdata (orderid, symbol, exchange, direction, order_type, offset, price, volume, traded, status, datetime, reference, gateway_name, post_only, reduce_only) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                    params![
                        order.orderid,
                        order.symbol,
                        exchange_str,
                        direction_str,
                        order_type_str,
                        offset_str,
                        order.price,
                        order.volume,
                        order.traded,
                        status_str,
                        datetime_str,
                        order.reference,
                        order.gateway_name,
                        order.post_only as i32,
                        order.reduce_only as i32,
                    ],
                ).map_err(|e| DatabaseError::InsertFailed { table: "dborderdata".to_string(), reason: e.to_string() })?;
            }

            tx.commit().map_err(|e| DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e)))?;
            Ok(true)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_trade_data(&self, trades: Vec<TradeData>) -> Result<bool, DatabaseError> {
        if trades.is_empty() {
            return Ok(true);
        }

        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let tx = conn.transaction()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin transaction: {}", e)))?;

            for trade in trades {
                let exchange_str = format!("{:?}", trade.exchange);
                let direction_str = trade.direction.map(|d| format!("{:?}", d)).unwrap_or_default();
                let offset_str = format!("{:?}", trade.offset);
                let datetime_str = trade.datetime.map(|dt| dt.to_rfc3339()).unwrap_or_default();

                tx.execute(
                    "INSERT OR REPLACE INTO dbtradedata (tradeid, orderid, symbol, exchange, direction, offset, price, volume, datetime, gateway_name) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![
                        trade.tradeid,
                        trade.orderid,
                        trade.symbol,
                        exchange_str,
                        direction_str,
                        offset_str,
                        trade.price,
                        trade.volume,
                        datetime_str,
                        trade.gateway_name,
                    ],
                ).map_err(|e| DatabaseError::InsertFailed { table: "dbtradedata".to_string(), reason: e.to_string() })?;
            }

            tx.commit().map_err(|e| DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e)))?;
            Ok(true)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_position_data(&self, positions: Vec<PositionData>) -> Result<bool, DatabaseError> {
        if positions.is_empty() {
            return Ok(true);
        }

        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let tx = conn.transaction()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to begin transaction: {}", e)))?;

            for position in positions {
                let exchange_str = format!("{:?}", position.exchange);
                let direction_str = format!("{:?}", position.direction);

                tx.execute(
                    "INSERT OR REPLACE INTO dbpositiondata (symbol, exchange, direction, volume, frozen, price, pnl, yd_volume, gateway_name) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                    params![
                        position.symbol,
                        exchange_str,
                        direction_str,
                        position.volume,
                        position.frozen,
                        position.price,
                        position.pnl,
                        position.yd_volume,
                        position.gateway_name,
                    ],
                ).map_err(|e| DatabaseError::InsertFailed { table: "dbpositiondata".to_string(), reason: e.to_string() })?;
            }

            tx.commit().map_err(|e| DatabaseError::QueryFailed(format!("Failed to commit transaction: {}", e)))?;
            Ok(true)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn save_event(&self, event: EventRecord) -> Result<bool, DatabaseError> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            conn.execute(
                r#"
                INSERT OR REPLACE INTO dbeventdata (event_id, event_type, gateway_name, timestamp, payload)
                VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    event.event_id,
                    event.event_type,
                    event.gateway_name,
                    event.timestamp.to_rfc3339(),
                    event.payload,
                ],
            ).map_err(|e| DatabaseError::InsertFailed { table: "dbeventdata".to_string(), reason: e.to_string() })?;

            Ok(true)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_orders(&self, gateway_name: Option<&str>) -> Result<Vec<OrderData>, DatabaseError> {
        let conn = self.conn.clone();
        let gateway_filter = gateway_name.map(|g| g.to_string());

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let sql = if gateway_filter.is_some() {
                "SELECT orderid, symbol, exchange, direction, order_type, offset, price, volume, traded, status, datetime, reference, gateway_name, post_only, reduce_only FROM dborderdata WHERE gateway_name = ?1"
            } else {
                "SELECT orderid, symbol, exchange, direction, order_type, offset, price, volume, traded, status, datetime, reference, gateway_name, post_only, reduce_only FROM dborderdata"
            };

            let mut stmt = conn.prepare(sql)
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e)))?;

            let orders = if let Some(gw) = &gateway_filter {
                stmt.query_map(params![gw], |row| {
                    Self::row_to_order(row)
                }).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query orders: {}", e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect orders: {}", e)))?
            } else {
                stmt.query_map([], |row| {
                    Self::row_to_order(row)
                }).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query orders: {}", e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect orders: {}", e)))?
            };

            Ok(orders)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_trades(&self, gateway_name: Option<&str>) -> Result<Vec<TradeData>, DatabaseError> {
        let conn = self.conn.clone();
        let gateway_filter = gateway_name.map(|g| g.to_string());

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let sql = if gateway_filter.is_some() {
                "SELECT tradeid, orderid, symbol, exchange, direction, offset, price, volume, datetime, gateway_name FROM dbtradedata WHERE gateway_name = ?1"
            } else {
                "SELECT tradeid, orderid, symbol, exchange, direction, offset, price, volume, datetime, gateway_name FROM dbtradedata"
            };

            let mut stmt = conn.prepare(sql)
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e)))?;

            let trades = if let Some(gw) = &gateway_filter {
                stmt.query_map(params![gw], |row| {
                    Self::row_to_trade(row)
                }).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query trades: {}", e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect trades: {}", e)))?
            } else {
                stmt.query_map([], |row| {
                    Self::row_to_trade(row)
                }).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query trades: {}", e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect trades: {}", e)))?
            };

            Ok(trades)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }

    async fn load_positions(&self, gateway_name: Option<&str>) -> Result<Vec<PositionData>, DatabaseError> {
        let conn = self.conn.clone();
        let gateway_filter = gateway_name.map(|g| g.to_string());

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| DatabaseError::Other(format!("Failed to acquire database lock: {}", e)))?;

            let sql = if gateway_filter.is_some() {
                "SELECT symbol, exchange, direction, volume, frozen, price, pnl, yd_volume, gateway_name FROM dbpositiondata WHERE gateway_name = ?1"
            } else {
                "SELECT symbol, exchange, direction, volume, frozen, price, pnl, yd_volume, gateway_name FROM dbpositiondata"
            };

            let mut stmt = conn.prepare(sql)
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to prepare statement: {}", e)))?;

            let positions = if let Some(gw) = &gateway_filter {
                stmt.query_map(params![gw], |row| {
                    Self::row_to_position(row)
                }).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query positions: {}", e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect positions: {}", e)))?
            } else {
                stmt.query_map([], |row| {
                    Self::row_to_position(row)
                }).map_err(|e| DatabaseError::QueryFailed(format!("Failed to query positions: {}", e)))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| DatabaseError::QueryFailed(format!("Failed to collect positions: {}", e)))?
            };

            Ok(positions)
        }).await.map_err(|e| DatabaseError::Other(format!("spawn_blocking error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::object::BarData;

    #[tokio::test]
    async fn test_sqlite_save_and_load_bars() {
        let db = SqliteDatabase::new_in_memory().expect("Failed to create database");

        let now = Utc::now();
        let bar1 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: now,
            interval: Some(Interval::Minute),
            open_price: 50000.0,
            high_price: 50100.0,
            low_price: 49900.0,
            close_price: 50050.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            extra: None,
        };
        let bar2 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: now + chrono::Duration::minutes(1),
            interval: Some(Interval::Minute),
            open_price: 50050.0,
            high_price: 50200.0,
            low_price: 50000.0,
            close_price: 50150.0,
            volume: 150.0,
            turnover: 7500000.0,
            open_interest: 0.0,
            extra: None,
        };

        db.save_bar_data(vec![bar1, bar2], false).await.expect("Failed to save bars");

        let loaded = db.load_bar_data(
            "BTCUSDT",
            Exchange::Binance,
            Interval::Minute,
            now - chrono::Duration::hours(1),
            now + chrono::Duration::hours(1),
        ).await.expect("Failed to load bars");

        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].symbol, "BTCUSDT");
        assert_eq!(loaded[0].open_price, 50000.0);
    }

    #[tokio::test]
    async fn test_sqlite_bar_overview() {
        let db = SqliteDatabase::new_in_memory().expect("Failed to create database");

        let now = Utc::now();
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "ETHUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: now,
            interval: Some(Interval::Hour),
            open_price: 3000.0,
            high_price: 3050.0,
            low_price: 2980.0,
            close_price: 3020.0,
            volume: 1000.0,
            turnover: 3000000.0,
            open_interest: 0.0,
            extra: None,
        };

        db.save_bar_data(vec![bar], false).await.expect("Failed to save bar");

        let overviews = db.get_bar_overview().await.expect("Failed to get overview");
        assert_eq!(overviews.len(), 1);
        assert_eq!(overviews[0].symbol, "ETHUSDT");
        assert_eq!(overviews[0].count, 1);
    }

    #[tokio::test]
    async fn test_sqlite_delete_bars() {
        let db = SqliteDatabase::new_in_memory().expect("Failed to create database");

        let now = Utc::now();
        let bar = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "XRPUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: now,
            interval: Some(Interval::Daily),
            open_price: 0.5,
            high_price: 0.52,
            low_price: 0.49,
            close_price: 0.51,
            volume: 1000000.0,
            turnover: 500000.0,
            open_interest: 0.0,
            extra: None,
        };

        db.save_bar_data(vec![bar], false).await.expect("Failed to save bar");

        let deleted = db.delete_bar_data("XRPUSDT", Exchange::Binance, Interval::Daily)
            .await.expect("Failed to delete");
        assert_eq!(deleted, 1);

        let overviews = db.get_bar_overview().await.expect("Failed to get overview");
        assert!(overviews.is_empty());
    }

    #[tokio::test]
    async fn test_sqlite_dedup() {
        let db = SqliteDatabase::new_in_memory().expect("Failed to create database");

        let now = Utc::now();
        let bar1 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "DOGEUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: now,
            interval: Some(Interval::Minute),
            open_price: 0.1,
            high_price: 0.11,
            low_price: 0.09,
            close_price: 0.105,
            volume: 1000.0,
            turnover: 100.0,
            open_interest: 0.0,
            extra: None,
        };
        let bar2 = BarData {
            gateway_name: "TEST".to_string(),
            symbol: "DOGEUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: now, // Same timestamp - should be deduplicated
            interval: Some(Interval::Minute),
            open_price: 0.12, // Different price
            high_price: 0.13,
            low_price: 0.11,
            close_price: 0.125,
            volume: 2000.0,
            turnover: 250.0,
            open_interest: 0.0,
            extra: None,
        };

        db.save_bar_data(vec![bar1], false).await.expect("Failed to save first bar");
        db.save_bar_data(vec![bar2], false).await.expect("Failed to save second bar"); // Should replace

        let loaded = db.load_bar_data(
            "DOGEUSDT",
            Exchange::Binance,
            Interval::Minute,
            now - chrono::Duration::hours(1),
            now + chrono::Duration::hours(1),
        ).await.expect("Failed to load bars");

        assert_eq!(loaded.len(), 1); // Only one bar due to dedup
        // The second bar should have replaced the first (INSERT OR REPLACE)
        assert_eq!(loaded[0].open_price, 0.12);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn test_save_and_load_order_data() {
        let db = SqliteDatabase::new_in_memory().unwrap();

        let now = Utc::now();
        let order = OrderData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "ORD001".to_string(),
            order_type: OrderType::Limit,
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            traded: 0.5,
            status: Status::PartTraded,
            datetime: Some(now),
            reference: "test_ref".to_string(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        db.save_order_data(vec![order.clone()]).await.unwrap();

        let loaded = db.load_orders(None).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].orderid, "ORD001");
        assert_eq!(loaded[0].symbol, "BTCUSDT");
        assert_eq!(loaded[0].exchange, Exchange::Binance);
        assert_eq!(loaded[0].direction, Some(Direction::Long));
        assert_eq!(loaded[0].order_type, OrderType::Limit);
        assert_eq!(loaded[0].offset, Offset::Open);
        assert_eq!(loaded[0].price, 50000.0);
        assert_eq!(loaded[0].volume, 1.0);
        assert_eq!(loaded[0].traded, 0.5);
        assert_eq!(loaded[0].status, Status::PartTraded);
        assert_eq!(loaded[0].reference, "test_ref");
        assert_eq!(loaded[0].gateway_name, "BINANCE");
        assert!(loaded[0].datetime.is_some());
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn test_save_and_load_trade_data() {
        let db = SqliteDatabase::new_in_memory().unwrap();

        let now = Utc::now();
        let trade = TradeData {
            gateway_name: "BINANCE".to_string(),
            symbol: "ETHUSDT".to_string(),
            exchange: Exchange::BinanceUsdm,
            orderid: "ORD002".to_string(),
            tradeid: "TRD001".to_string(),
            direction: Some(Direction::Short),
            offset: Offset::Close,
            price: 3000.0,
            volume: 2.0,
            datetime: Some(now),
            extra: None,
        };

        db.save_trade_data(vec![trade.clone()]).await.unwrap();

        let loaded = db.load_trades(None).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].tradeid, "TRD001");
        assert_eq!(loaded[0].orderid, "ORD002");
        assert_eq!(loaded[0].symbol, "ETHUSDT");
        assert_eq!(loaded[0].exchange, Exchange::BinanceUsdm);
        assert_eq!(loaded[0].direction, Some(Direction::Short));
        assert_eq!(loaded[0].offset, Offset::Close);
        assert_eq!(loaded[0].price, 3000.0);
        assert_eq!(loaded[0].volume, 2.0);
        assert_eq!(loaded[0].gateway_name, "BINANCE");
        assert!(loaded[0].datetime.is_some());
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn test_save_and_load_position_data() {
        let db = SqliteDatabase::new_in_memory().unwrap();

        let position = PositionData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            direction: Direction::Long,
            volume: 10.0,
            frozen: 2.0,
            price: 49000.0,
            pnl: 1000.0,
            yd_volume: 8.0,
            extra: None,
        };

        db.save_position_data(vec![position.clone()]).await.unwrap();

        let loaded = db.load_positions(None).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].symbol, "BTCUSDT");
        assert_eq!(loaded[0].exchange, Exchange::Binance);
        assert_eq!(loaded[0].direction, Direction::Long);
        assert_eq!(loaded[0].volume, 10.0);
        assert_eq!(loaded[0].frozen, 2.0);
        assert_eq!(loaded[0].price, 49000.0);
        assert_eq!(loaded[0].pnl, 1000.0);
        assert_eq!(loaded[0].yd_volume, 8.0);
        assert_eq!(loaded[0].gateway_name, "BINANCE");
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn test_save_order_data_with_post_only_reduce_only() {
        let db = SqliteDatabase::new_in_memory().unwrap();

        let order = OrderData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "ORD_POST".to_string(),
            order_type: OrderType::Limit,
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: None,
            reference: String::new(),
            post_only: true,
            reduce_only: true,
            expire_time: None,
            extra: None,
        };

        db.save_order_data(vec![order.clone()]).await.unwrap();

        let loaded = db.load_orders(None).await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].post_only);
        assert!(loaded[0].reduce_only);
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn test_load_orders_with_gateway_filter() {
        let db = SqliteDatabase::new_in_memory().unwrap();

        let order1 = OrderData {
            gateway_name: "BINANCE_SPOT".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "ORD_GW1".to_string(),
            order_type: OrderType::Limit,
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: None,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        let order2 = OrderData {
            gateway_name: "BINANCE_USDM".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::BinanceUsdm,
            orderid: "ORD_GW2".to_string(),
            order_type: OrderType::Market,
            direction: Some(Direction::Short),
            offset: Offset::Close,
            price: 50100.0,
            volume: 2.0,
            traded: 2.0,
            status: Status::AllTraded,
            datetime: None,
            reference: String::new(),
            post_only: false,
            reduce_only: true,
            expire_time: None,
            extra: None,
        };

        db.save_order_data(vec![order1, order2]).await.unwrap();

        // Load all
        let all = db.load_orders(None).await.unwrap();
        assert_eq!(all.len(), 2);

        // Load filtered
        let filtered = db.load_orders(Some("BINANCE_SPOT")).await.unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].orderid, "ORD_GW1");

        let filtered2 = db.load_orders(Some("BINANCE_USDM")).await.unwrap();
        assert_eq!(filtered2.len(), 1);
        assert_eq!(filtered2[0].orderid, "ORD_GW2");
    }

    #[tokio::test]
    #[allow(clippy::unwrap_used)]
    async fn test_save_order_data_upsert() {
        let db = SqliteDatabase::new_in_memory().unwrap();

        let order_v1 = OrderData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "ORD_UPSERT".to_string(),
            order_type: OrderType::Limit,
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 50000.0,
            volume: 1.0,
            traded: 0.0,
            status: Status::NotTraded,
            datetime: None,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        db.save_order_data(vec![order_v1]).await.unwrap();

        let order_v2 = OrderData {
            gateway_name: "BINANCE".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            orderid: "ORD_UPSERT".to_string(),
            order_type: OrderType::Limit,
            direction: Some(Direction::Long),
            offset: Offset::Open,
            price: 51000.0, // Updated price
            volume: 1.0,
            traded: 1.0, // Updated traded
            status: Status::AllTraded, // Updated status
            datetime: None,
            reference: String::new(),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            extra: None,
        };

        db.save_order_data(vec![order_v2]).await.unwrap();

        let loaded = db.load_orders(None).await.unwrap();
        // Should be exactly 1 (upserted, not duplicated)
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].price, 51000.0);
        assert_eq!(loaded[0].traded, 1.0);
        assert_eq!(loaded[0].status, Status::AllTraded);
    }
}
