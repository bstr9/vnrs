//! Database integration for historical data loading
//! 
//! Loads historical bar and tick data from database backends.
//! Supports PostgreSQL (via `database` feature) and SQLite (via `sqlite` feature).

use chrono::{DateTime, Utc};
#[cfg(feature = "database")]
use chrono::NaiveDateTime;
#[cfg(feature = "sqlite")]
use std::sync::Arc;
use crate::trader::{BarData, TickData, Exchange, Interval};
#[cfg(feature = "sqlite")]
use crate::trader::database::BaseDatabase;

#[cfg(feature = "database")]
use sqlx::{PgPool, Row};

/// Database loader for historical data
pub struct DatabaseLoader {
    #[cfg(feature = "database")]
    pool: Option<PgPool>,
    #[cfg(feature = "sqlite")]
    sqlite_db: Option<Arc<dyn BaseDatabase>>,
}

impl Default for DatabaseLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseLoader {
    /// Create new database loader
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "database")]
            pool: None,
            #[cfg(feature = "sqlite")]
            sqlite_db: None,
        }
    }

    /// Connect to PostgreSQL database
    #[cfg(feature = "database")]
    pub async fn connect(&mut self, database_url: &str) -> Result<(), String> {
        match PgPool::connect(database_url).await {
            Ok(pool) => {
                self.pool = Some(pool);
                Ok(())
            }
            Err(e) => Err(format!("数据库连接失败: {}", e)),
        }
    }

    #[cfg(not(feature = "database"))]
    pub async fn connect(&mut self, _database_url: &str) -> Result<(), String> {
        Err("数据库功能未启用，请使用 --features database 编译".to_string())
    }

    /// Set the SQLite database for loading data
    ///
    /// Only available when the `sqlite` feature is enabled.
    #[cfg(feature = "sqlite")]
    pub fn set_sqlite_database(&mut self, db: Arc<dyn BaseDatabase>) {
        self.sqlite_db = Some(db);
    }

    /// Load bar data from database
    ///
    /// Tries SQLite database first (if configured), then PostgreSQL.
    #[cfg(feature = "sqlite")]
    pub async fn load_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, String> {
        // Try SQLite first
        if let Some(db) = &self.sqlite_db {
            let bars = db.load_bar_data(symbol, exchange, interval, start, end).await.map_err(|e| e.to_string())?;
            if !bars.is_empty() {
                return Ok(bars);
            }
            return Err(format!("SQLite数据库中无Bar数据: {} {:?}", symbol, interval));
        }

        // Fall through to PostgreSQL if available
        #[cfg(feature = "database")]
        {
            self.load_bar_data_postgres(symbol, exchange, interval, start, end).await
        }
        #[cfg(not(feature = "database"))]
        {
            Err("数据库功能未启用，请配置SQLite或使用 --features database 编译".to_string())
        }
    }

    #[cfg(not(feature = "sqlite"))]
    pub async fn load_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, String> {
        #[cfg(feature = "database")]
        {
            self.load_bar_data_postgres(symbol, exchange, interval, start, end).await
        }
        #[cfg(not(feature = "database"))]
        {
            let _ = (symbol, exchange, interval, start, end);
            Err("数据库功能未启用".to_string())
        }
    }

    /// Load bar data from PostgreSQL (internal method)
    #[cfg(feature = "database")]
    async fn load_bar_data_postgres(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, String> {
        let pool = self.pool.as_ref()
            .ok_or("数据库未连接".to_string())?;

        let exchange_str = format!("{:?}", exchange);
        let interval_str = format!("{:?}", interval);

        let query = r#"
            SELECT 
                symbol, exchange, datetime, interval,
                open_price, high_price, low_price, close_price,
                volume, turnover, open_interest
            FROM dbbardata
            WHERE symbol = $1 
                AND exchange = $2
                AND interval = $3
                AND datetime >= $4
                AND datetime <= $5
            ORDER BY datetime ASC
        "#;

        let rows = sqlx::query(query)
            .bind(symbol)
            .bind(&exchange_str)
            .bind(&interval_str)
            .bind(start.naive_utc())
            .bind(end.naive_utc())
            .fetch_all(pool)
            .await
            .map_err(|e| format!("查询Bar数据失败: {}", e))?;

        let mut bars = Vec::new();
        for row in rows {
            let datetime: NaiveDateTime = row.try_get("datetime")
                .map_err(|e| format!("解析datetime失败: {}", e))?;
            
            let bar = BarData {
                gateway_name: "DATABASE".to_string(),
                symbol: row.try_get("symbol")
                    .map_err(|e| format!("解析symbol失败: {}", e))?,
                exchange,
                datetime: DateTime::from_naive_utc_and_offset(datetime, Utc),
                interval: Some(interval),
                volume: row.try_get("volume")
                    .map_err(|e| format!("解析volume失败: {}", e))?,
                turnover: row.try_get::<f64, _>("turnover")
                    .unwrap_or(0.0),
                open_interest: row.try_get::<f64, _>("open_interest")
                    .unwrap_or(0.0),
                open_price: row.try_get("open_price")
                    .map_err(|e| format!("解析open_price失败: {}", e))?,
                high_price: row.try_get("high_price")
                    .map_err(|e| format!("解析high_price失败: {}", e))?,
                low_price: row.try_get("low_price")
                    .map_err(|e| format!("解析low_price失败: {}", e))?,
                close_price: row.try_get("close_price")
                    .map_err(|e| format!("解析close_price失败: {}", e))?,
                extra: None,
            };
            bars.push(bar);
        }

        Ok(bars)
    }

    /// Load tick data from database
    ///
    /// Tries SQLite database first (if configured), then PostgreSQL.
    #[cfg(feature = "sqlite")]
    pub async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, String> {
        // Try SQLite first
        if let Some(db) = &self.sqlite_db {
            let ticks = db.load_tick_data(symbol, exchange, start, end).await.map_err(|e| e.to_string())?;
            if !ticks.is_empty() {
                return Ok(ticks);
            }
            return Err(format!("SQLite数据库中无Tick数据: {}", symbol));
        }

        // Fall through to PostgreSQL if available
        #[cfg(feature = "database")]
        {
            self.load_tick_data_postgres(symbol, exchange, start, end).await
        }
        #[cfg(not(feature = "database"))]
        {
            Err("数据库功能未启用，请配置SQLite或使用 --features database 编译".to_string())
        }
    }

    #[cfg(not(feature = "sqlite"))]
    pub async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, String> {
        #[cfg(feature = "database")]
        {
            self.load_tick_data_postgres(symbol, exchange, start, end).await
        }
        #[cfg(not(feature = "database"))]
        {
            let _ = (symbol, exchange, start, end);
            Err("数据库功能未启用".to_string())
        }
    }

    /// Load tick data from PostgreSQL (internal method)
    #[cfg(feature = "database")]
    async fn load_tick_data_postgres(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, String> {
        let pool = self.pool.as_ref()
            .ok_or("数据库未连接".to_string())?;

        let exchange_str = format!("{:?}", exchange);

        let query = r#"
            SELECT 
                symbol, exchange, datetime,
                last_price, last_volume,
                bid_price_1, bid_price_2, bid_price_3, bid_price_4, bid_price_5,
                bid_volume_1, bid_volume_2, bid_volume_3, bid_volume_4, bid_volume_5,
                ask_price_1, ask_price_2, ask_price_3, ask_price_4, ask_price_5,
                ask_volume_1, ask_volume_2, ask_volume_3, ask_volume_4, ask_volume_5,
                volume, turnover, open_interest
            FROM dbtickdata
            WHERE symbol = $1 
                AND exchange = $2
                AND datetime >= $3
                AND datetime <= $4
            ORDER BY datetime ASC
        "#;

        let rows = sqlx::query(query)
            .bind(symbol)
            .bind(&exchange_str)
            .bind(start.naive_utc())
            .bind(end.naive_utc())
            .fetch_all(pool)
            .await
            .map_err(|e| format!("查询Tick数据失败: {}", e))?;

        let mut ticks = Vec::new();
        for row in rows {
            let datetime: NaiveDateTime = row.try_get("datetime")
                .map_err(|e| format!("解析datetime失败: {}", e))?;
            
            let tick = TickData {
                gateway_name: "DATABASE".to_string(),
                symbol: row.try_get("symbol")
                    .map_err(|e| format!("解析symbol失败: {}", e))?,
                exchange,
                datetime: DateTime::from_naive_utc_and_offset(datetime, Utc),
                name: String::new(),
                volume: row.try_get("volume").unwrap_or(0.0),
                turnover: row.try_get("turnover").unwrap_or(0.0),
                open_interest: row.try_get("open_interest").unwrap_or(0.0),
                last_price: row.try_get("last_price")
                    .map_err(|e| format!("解析last_price失败: {}", e))?,
                last_volume: row.try_get::<f64, _>("last_volume").unwrap_or(0.0),
                limit_up: 0.0,
                limit_down: 0.0,
                open_price: row.try_get("open_price").unwrap_or(0.0),
                high_price: row.try_get("high_price").unwrap_or(0.0),
                low_price: row.try_get("low_price").unwrap_or(0.0),
                pre_close: row.try_get("pre_close").unwrap_or(0.0),
                bid_price_1: row.try_get("bid_price_1").unwrap_or(0.0),
                bid_price_2: row.try_get("bid_price_2").unwrap_or(0.0),
                bid_price_3: row.try_get("bid_price_3").unwrap_or(0.0),
                bid_price_4: row.try_get("bid_price_4").unwrap_or(0.0),
                bid_price_5: row.try_get("bid_price_5").unwrap_or(0.0),
                bid_volume_1: row.try_get("bid_volume_1").unwrap_or(0.0),
                bid_volume_2: row.try_get("bid_volume_2").unwrap_or(0.0),
                bid_volume_3: row.try_get("bid_volume_3").unwrap_or(0.0),
                bid_volume_4: row.try_get("bid_volume_4").unwrap_or(0.0),
                bid_volume_5: row.try_get("bid_volume_5").unwrap_or(0.0),
                ask_price_1: row.try_get("ask_price_1").unwrap_or(0.0),
                ask_price_2: row.try_get("ask_price_2").unwrap_or(0.0),
                ask_price_3: row.try_get("ask_price_3").unwrap_or(0.0),
                ask_price_4: row.try_get("ask_price_4").unwrap_or(0.0),
                ask_price_5: row.try_get("ask_price_5").unwrap_or(0.0),
                ask_volume_1: row.try_get("ask_volume_1").unwrap_or(0.0),
                ask_volume_2: row.try_get("ask_volume_2").unwrap_or(0.0),
                ask_volume_3: row.try_get("ask_volume_3").unwrap_or(0.0),
                ask_volume_4: row.try_get("ask_volume_4").unwrap_or(0.0),
                ask_volume_5: row.try_get("ask_volume_5").unwrap_or(0.0),
                localtime: None,
                extra: None,
            };
            ticks.push(tick);
        }

        Ok(ticks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trader::constant::{Exchange, Interval};
    use chrono::Utc;

    #[test]
    fn test_database_loader_new() {
        let loader = DatabaseLoader::new();
        // Should construct without panic; no pool field without database feature
        drop(loader);
    }

    #[test]
    fn test_database_loader_default() {
        let loader = DatabaseLoader::default();
        drop(loader);
    }

    #[tokio::test]
    async fn test_database_loader_connect_without_feature() {
        let mut loader = DatabaseLoader::new();
        let result = loader.connect("postgresql://invalid:5432/test").await;
        assert!(result.is_err());
        assert!(result.expect_err("should be error").contains("数据库功能未启用"));
    }

    #[tokio::test]
    async fn test_database_loader_load_bar_data_without_feature() {
        let loader = DatabaseLoader::new();
        let result = loader.load_bar_data(
            "BTCUSDT",
            Exchange::Binance,
            Interval::Minute,
            Utc::now(),
            Utc::now(),
        ).await;
        assert!(result.is_err());
        assert!(result.expect_err("should be error").contains("数据库功能未启用"));
    }

    #[tokio::test]
    async fn test_database_loader_load_tick_data_without_feature() {
        let loader = DatabaseLoader::new();
        let result = loader.load_tick_data(
            "BTCUSDT",
            Exchange::Binance,
            Utc::now(),
            Utc::now(),
        ).await;
        assert!(result.is_err());
        assert!(result.expect_err("should be error").contains("数据库功能未启用"));
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn test_database_loader_sqlite_tick_data() {
        use crate::trader::SqliteDatabase;

        let sqlite_db = SqliteDatabase::new_in_memory().expect("Failed to create SQLite database");
        let db = Arc::new(sqlite_db) as Arc<dyn crate::trader::database::BaseDatabase>;

        let mut loader = DatabaseLoader::new();
        loader.set_sqlite_database(db.clone());

        // Save tick data
        let now = Utc::now();
        let tick = TickData {
            gateway_name: "BINANCE_SPOT".to_string(),
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            datetime: now,
            last_price: 50000.0,
            last_volume: 1.0,
            volume: 100.0,
            turnover: 5000000.0,
            open_interest: 0.0,
            bid_price_1: 49999.0,
            bid_volume_1: 0.5,
            ask_price_1: 50001.0,
            ask_volume_1: 0.5,
            ..TickData::new("BINANCE_SPOT".to_string(), "BTCUSDT".to_string(), Exchange::Binance, now)
        };
        db.save_tick_data(vec![tick], false).await.expect("save should succeed");

        // Load tick data via DatabaseLoader
        let ticks = loader.load_tick_data(
            "BTCUSDT",
            Exchange::Binance,
            now - chrono::Duration::hours(1),
            now + chrono::Duration::hours(1),
        ).await.expect("load should succeed");

        assert_eq!(ticks.len(), 1);
        assert_eq!(ticks[0].symbol, "BTCUSDT");
        assert_eq!(ticks[0].last_price, 50000.0);
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn test_database_loader_sqlite_tick_data_empty() {
        use crate::trader::SqliteDatabase;

        let sqlite_db = SqliteDatabase::new_in_memory().expect("Failed to create SQLite database");
        let db = Arc::new(sqlite_db) as Arc<dyn crate::trader::database::BaseDatabase>;

        let mut loader = DatabaseLoader::new();
        loader.set_sqlite_database(db);

        // Load from empty database should fail
        let result = loader.load_tick_data(
            "BTCUSDT",
            Exchange::Binance,
            Utc::now() - chrono::Duration::hours(1),
            Utc::now() + chrono::Duration::hours(1),
        ).await;

        assert!(result.is_err());
        assert!(result.expect_err("should be error").contains("SQLite数据库中无Tick数据"));
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn test_database_loader_sqlite_bar_data() {
        use crate::trader::SqliteDatabase;

        let sqlite_db = SqliteDatabase::new_in_memory().expect("Failed to create SQLite database");
        let db = Arc::new(sqlite_db) as Arc<dyn crate::trader::database::BaseDatabase>;

        let mut loader = DatabaseLoader::new();
        loader.set_sqlite_database(db.clone());

        // Save bar data
        let now = Utc::now();
        let bar = BarData {
            gateway_name: "BINANCE_SPOT".to_string(),
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
        db.save_bar_data(vec![bar], false).await.expect("save should succeed");

        // Load bar data via DatabaseLoader
        let bars = loader.load_bar_data(
            "BTCUSDT",
            Exchange::Binance,
            Interval::Minute,
            now - chrono::Duration::hours(1),
            now + chrono::Duration::hours(1),
        ).await.expect("load should succeed");

        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].symbol, "BTCUSDT");
    }
}
