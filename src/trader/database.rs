//! Database module for storing and retrieving trading data.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::constant::{Exchange, Interval};
use super::object::{BarData, TickData};
use super::setting::SETTINGS;

/// Overview of bar data stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarOverview {
    pub symbol: String,
    pub exchange: Option<Exchange>,
    pub interval: Option<Interval>,
    pub count: i64,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

impl Default for BarOverview {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            exchange: None,
            interval: None,
            count: 0,
            start: None,
            end: None,
        }
    }
}

/// Overview of tick data stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickOverview {
    pub symbol: String,
    pub exchange: Option<Exchange>,
    pub count: i64,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

impl Default for TickOverview {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            exchange: None,
            count: 0,
            start: None,
            end: None,
        }
    }
}

/// Abstract database trait for connecting to different databases
#[async_trait]
pub trait BaseDatabase: Send + Sync {
    /// Save bar data into database
    async fn save_bar_data(&self, bars: Vec<BarData>, stream: bool) -> Result<bool, String>;

    /// Save tick data into database
    async fn save_tick_data(&self, ticks: Vec<TickData>, stream: bool) -> Result<bool, String>;

    /// Load bar data from database
    async fn load_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, String>;

    /// Load tick data from database
    async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, String>;

    /// Delete all bar data with given symbol + exchange + interval
    async fn delete_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
    ) -> Result<i64, String>;

    /// Delete all tick data with given symbol + exchange
    async fn delete_tick_data(&self, symbol: &str, exchange: Exchange) -> Result<i64, String>;

    /// Return bar data available in database
    async fn get_bar_overview(&self) -> Result<Vec<BarOverview>, String>;

    /// Return tick data available in database
    async fn get_tick_overview(&self) -> Result<Vec<TickOverview>, String>;
}

/// In-memory database implementation for testing
pub struct MemoryDatabase {
    bars: std::sync::RwLock<Vec<BarData>>,
    ticks: std::sync::RwLock<Vec<TickData>>,
}

impl MemoryDatabase {
    pub fn new() -> Self {
        Self {
            bars: std::sync::RwLock::new(Vec::new()),
            ticks: std::sync::RwLock::new(Vec::new()),
        }
    }
}

impl Default for MemoryDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseDatabase for MemoryDatabase {
    async fn save_bar_data(&self, bars: Vec<BarData>, _stream: bool) -> Result<bool, String> {
        let mut data = self.bars.write().map_err(|e| e.to_string())?;
        data.extend(bars);
        Ok(true)
    }

    async fn save_tick_data(&self, ticks: Vec<TickData>, _stream: bool) -> Result<bool, String> {
        let mut data = self.ticks.write().map_err(|e| e.to_string())?;
        data.extend(ticks);
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
        let data = self.bars.read().map_err(|e| e.to_string())?;
        let result: Vec<BarData> = data
            .iter()
            .filter(|bar| {
                bar.symbol == symbol
                    && bar.exchange == exchange
                    && bar.interval == Some(interval)
                    && bar.datetime >= start
                    && bar.datetime <= end
            })
            .cloned()
            .collect();
        Ok(result)
    }

    async fn load_tick_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TickData>, String> {
        let data = self.ticks.read().map_err(|e| e.to_string())?;
        let result: Vec<TickData> = data
            .iter()
            .filter(|tick| {
                tick.symbol == symbol
                    && tick.exchange == exchange
                    && tick.datetime >= start
                    && tick.datetime <= end
            })
            .cloned()
            .collect();
        Ok(result)
    }

    async fn delete_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
    ) -> Result<i64, String> {
        let mut data = self.bars.write().map_err(|e| e.to_string())?;
        let original_len = data.len();
        data.retain(|bar| {
            !(bar.symbol == symbol
                && bar.exchange == exchange
                && bar.interval == Some(interval))
        });
        Ok((original_len - data.len()) as i64)
    }

    async fn delete_tick_data(&self, symbol: &str, exchange: Exchange) -> Result<i64, String> {
        let mut data = self.ticks.write().map_err(|e| e.to_string())?;
        let original_len = data.len();
        data.retain(|tick| !(tick.symbol == symbol && tick.exchange == exchange));
        Ok((original_len - data.len()) as i64)
    }

    async fn get_bar_overview(&self) -> Result<Vec<BarOverview>, String> {
        let data = self.bars.read().map_err(|e| e.to_string())?;
        
        // Group by symbol, exchange, interval
        use std::collections::HashMap;
        let mut groups: HashMap<(String, Exchange, Option<Interval>), Vec<&BarData>> = HashMap::new();
        
        for bar in data.iter() {
            let key = (bar.symbol.clone(), bar.exchange, bar.interval);
            groups.entry(key).or_insert_with(Vec::new).push(bar);
        }
        
        let mut overviews = Vec::new();
        for ((symbol, exchange, interval), bars) in groups {
            let count = bars.len() as i64;
            let start = bars.iter().map(|b| b.datetime).min();
            let end = bars.iter().map(|b| b.datetime).max();
            
            overviews.push(BarOverview {
                symbol,
                exchange: Some(exchange),
                interval,
                count,
                start,
                end,
            });
        }
        
        Ok(overviews)
    }

    async fn get_tick_overview(&self) -> Result<Vec<TickOverview>, String> {
        let data = self.ticks.read().map_err(|e| e.to_string())?;
        
        // Group by symbol, exchange
        use std::collections::HashMap;
        let mut groups: HashMap<(String, Exchange), Vec<&TickData>> = HashMap::new();
        
        for tick in data.iter() {
            let key = (tick.symbol.clone(), tick.exchange);
            groups.entry(key).or_insert_with(Vec::new).push(tick);
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
    }
}

/// Get database timezone
pub fn get_database_timezone() -> String {
    SETTINGS.get_string("database.timezone").unwrap_or_else(|| "UTC".to_string())
}

/// Convert datetime to database timezone (remove timezone info)
pub fn convert_tz(dt: DateTime<Utc>) -> chrono::NaiveDateTime {
    dt.naive_utc()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_database_bars() {
        let db = MemoryDatabase::new();
        
        let bar = BarData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        
        db.save_bar_data(vec![bar], false).await.unwrap();
        
        let overviews = db.get_bar_overview().await.unwrap();
        assert_eq!(overviews.len(), 1);
    }

    #[tokio::test]
    async fn test_memory_database_ticks() {
        let db = MemoryDatabase::new();
        
        let tick = TickData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        
        db.save_tick_data(vec![tick], false).await.unwrap();
        
        let overviews = db.get_tick_overview().await.unwrap();
        assert_eq!(overviews.len(), 1);
    }
}
