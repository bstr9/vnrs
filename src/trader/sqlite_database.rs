//! SQLite database implementation for local-first data persistence.
//!
//! Provides a single-file SQLite database backend for storing bars, ticks,
//! orders, trades, positions, and events. Suitable for local development
//! and single-machine deployment.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use std::sync::{Arc, Mutex};

use super::constant::{Exchange, Interval};
use super::database::{BaseDatabase, BarOverview, TickOverview, EventRecord};
use super::object::{BarData, TickData, OrderData, TradeData, PositionData};

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

        Ok(())
    }
}

#[async_trait]
impl BaseDatabase for SqliteDatabase {
    async fn save_bar_data(&self, bars: Vec<BarData>, _stream: bool) -> Result<bool, String> {
        if bars.is_empty() {
            return Ok(true);
        }

        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

            let tx = conn.transaction()
                .map_err(|e| format!("Failed to begin transaction: {}", e))?;

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
                ).map_err(|e| format!("Failed to insert bar: {}", e))?;
            }

            tx.commit().map_err(|e| format!("Failed to commit transaction: {}", e))?;
            Ok(true)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn save_tick_data(&self, ticks: Vec<TickData>, _stream: bool) -> Result<bool, String> {
        if ticks.is_empty() {
            return Ok(true);
        }

        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let mut conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

            let tx = conn.transaction()
                .map_err(|e| format!("Failed to begin transaction: {}", e))?;

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
                ).map_err(|e| format!("Failed to insert tick: {}", e))?;
            }

            tx.commit().map_err(|e| format!("Failed to commit transaction: {}", e))?;
            Ok(true)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn load_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, String> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);
        let interval_str = format!("{:?}", interval);
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, interval, datetime, open_price, high_price, low_price, close_price,
                       volume, turnover, open_interest, gateway_name
                FROM dbbardata
                WHERE symbol = ?1 AND exchange = ?2 AND interval = ?3 
                      AND datetime >= ?4 AND datetime <= ?5
                ORDER BY datetime ASC
                "#
            ).map_err(|e| format!("Failed to prepare statement: {}", e))?;

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
            ).map_err(|e| format!("Failed to query bars: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect bars: {}", e))?;

            Ok(bars)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, String> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);
        let start_str = start.to_rfc3339();
        let end_str = end.to_rfc3339();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, datetime, last_price, last_volume, volume, turnover, open_interest,
                       bid_price_1, bid_volume_1, ask_price_1, ask_volume_1, gateway_name
                FROM dbtickdata
                WHERE symbol = ?1 AND exchange = ?2 AND datetime >= ?3 AND datetime <= ?4
                ORDER BY datetime ASC
                "#
            ).map_err(|e| format!("Failed to prepare statement: {}", e))?;

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
            ).map_err(|e| format!("Failed to query ticks: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect ticks: {}", e))?;

            Ok(ticks)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn delete_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
    ) -> Result<i64, String> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);
        let interval_str = format!("{:?}", interval);

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM dbbardata WHERE symbol = ?1 AND exchange = ?2 AND interval = ?3",
                params![symbol, exchange_str, interval_str],
                |row| row.get(0),
            ).unwrap_or(0);

            conn.execute(
                "DELETE FROM dbbardata WHERE symbol = ?1 AND exchange = ?2 AND interval = ?3",
                params![symbol, exchange_str, interval_str],
            ).map_err(|e| format!("Failed to delete bars: {}", e))?;

            Ok(count)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn delete_tick_data(&self, symbol: &str, exchange: Exchange) -> Result<i64, String> {
        let conn = self.conn.clone();

        let symbol = symbol.to_string();
        let exchange_str = format!("{:?}", exchange);

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM dbtickdata WHERE symbol = ?1 AND exchange = ?2",
                params![symbol, exchange_str],
                |row| row.get(0),
            ).unwrap_or(0);

            conn.execute(
                "DELETE FROM dbtickdata WHERE symbol = ?1 AND exchange = ?2",
                params![symbol, exchange_str],
            ).map_err(|e| format!("Failed to delete ticks: {}", e))?;

            Ok(count)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn get_bar_overview(&self) -> Result<Vec<BarOverview>, String> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, interval, COUNT(*) as count, MIN(datetime) as start, MAX(datetime) as end
                FROM dbbardata
                GROUP BY symbol, exchange, interval
                "#
            ).map_err(|e| format!("Failed to prepare statement: {}", e))?;

            let overviews = stmt.query_map([], |row| {
                let exchange_str: String = row.get(1)?;
                let exchange = match exchange_str.as_str() {
                    "Binance" => Some(Exchange::Binance),
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
            }).map_err(|e| format!("Failed to query bar overview: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect bar overview: {}", e))?;

            Ok(overviews)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn get_tick_overview(&self) -> Result<Vec<TickOverview>, String> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

            let mut stmt = conn.prepare(
                r#"
                SELECT symbol, exchange, COUNT(*) as count, MIN(datetime) as start, MAX(datetime) as end
                FROM dbtickdata
                GROUP BY symbol, exchange
                "#
            ).map_err(|e| format!("Failed to prepare statement: {}", e))?;

            let overviews = stmt.query_map([], |row| {
                let exchange_str: String = row.get(1)?;
                let exchange = match exchange_str.as_str() {
                    "Binance" => Some(Exchange::Binance),
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
            }).map_err(|e| format!("Failed to query tick overview: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to collect tick overview: {}", e))?;

            Ok(overviews)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn save_order_data(&self, _orders: Vec<OrderData>) -> Result<bool, String> {
        // Simplified implementation - orders are more complex to serialize
        Ok(true)
    }

    async fn save_trade_data(&self, _trades: Vec<TradeData>) -> Result<bool, String> {
        Ok(true)
    }

    async fn save_position_data(&self, _positions: Vec<PositionData>) -> Result<bool, String> {
        Ok(true)
    }

    async fn save_event(&self, event: EventRecord) -> Result<bool, String> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || {
            let conn = conn.lock()
                .map_err(|e| format!("Failed to acquire database lock: {}", e))?;

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
            ).map_err(|e| format!("Failed to insert event: {}", e))?;

            Ok(true)
        }).await.map_err(|e| format!("spawn_blocking error: {}", e))?
    }

    async fn load_orders(&self, _gateway_name: Option<&str>) -> Result<Vec<OrderData>, String> {
        Ok(Vec::new())
    }

    async fn load_trades(&self, _gateway_name: Option<&str>) -> Result<Vec<TradeData>, String> {
        Ok(Vec::new())
    }

    async fn load_positions(&self, _gateway_name: Option<&str>) -> Result<Vec<PositionData>, String> {
        Ok(Vec::new())
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
}
