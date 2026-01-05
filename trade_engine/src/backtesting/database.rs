//! Database integration for historical data loading
//! 
//! Loads historical bar and tick data from PostgreSQL database

use chrono::{DateTime, Utc, NaiveDateTime};
use crate::trader::{BarData, TickData, Exchange, Interval};

#[cfg(feature = "database")]
use sqlx::{PgPool, Row};

/// Database loader for historical data
pub struct DatabaseLoader {
    #[cfg(feature = "database")]
    pool: Option<PgPool>,
}

impl DatabaseLoader {
    /// Create new database loader
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "database")]
            pool: None,
        }
    }

    /// Connect to database
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

    /// Load bar data from database
    #[cfg(feature = "database")]
    pub async fn load_bar_data(
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
                interval,
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

    #[cfg(not(feature = "database"))]
    pub async fn load_bar_data(
        &self,
        _symbol: &str,
        _exchange: Exchange,
        _interval: Interval,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, String> {
        Err("数据库功能未启用".to_string())
    }

    /// Load tick data from database
    #[cfg(feature = "database")]
    pub async fn load_tick_data(
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
                last_price: row.try_get("last_price")
                    .map_err(|e| format!("解析last_price失败: {}", e))?,
                last_volume: row.try_get::<f64, _>("last_volume").unwrap_or(0.0),
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
                volume: row.try_get("volume").unwrap_or(0.0),
                turnover: row.try_get("turnover").unwrap_or(0.0),
                open_interest: row.try_get("open_interest").unwrap_or(0.0),
                extra: None,
            };
            ticks.push(tick);
        }

        Ok(ticks)
    }

    #[cfg(not(feature = "database"))]
    pub async fn load_tick_data(
        &self,
        _symbol: &str,
        _exchange: Exchange,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, String> {
        Err("数据库功能未启用".to_string())
    }
}
