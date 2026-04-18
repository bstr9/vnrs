//! Database module for storing and retrieving trading data.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::constant::{Exchange, Interval};
use super::object::{BarData, TickData};
use super::setting::SETTINGS;

/// Overview of bar data stored in database
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BarOverview {
    pub symbol: String,
    pub exchange: Option<Exchange>,
    pub interval: Option<Interval>,
    pub count: i64,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

/// Overview of tick data stored in database
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TickOverview {
    pub symbol: String,
    pub exchange: Option<Exchange>,
    pub count: i64,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
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
        // Dedup: collect existing keys for O(1) lookup
        let existing_keys: std::collections::HashSet<(String, Exchange, Option<Interval>, i64)> = data.iter()
            .map(|b| (b.symbol.clone(), b.exchange, b.interval, b.datetime.timestamp()))
            .collect();
        for bar in bars {
            let key = (bar.symbol.clone(), bar.exchange, bar.interval, bar.datetime.timestamp());
            if !existing_keys.contains(&key) {
                data.push(bar);
            }
        }
        // Limit to 1M bars
        if data.len() > 1_000_000 {
            let excess = data.len() - 1_000_000;
            data.drain(0..excess);
        }
        Ok(true)
    }

    async fn save_tick_data(&self, ticks: Vec<TickData>, _stream: bool) -> Result<bool, String> {
        let mut data = self.ticks.write().map_err(|e| e.to_string())?;
        // Dedup: collect existing keys for O(1) lookup
        let existing_keys: std::collections::HashSet<(String, Exchange, i64)> = data.iter()
            .map(|t| (t.symbol.clone(), t.exchange, t.datetime.timestamp()))
            .collect();
        for tick in ticks {
            let key = (tick.symbol.clone(), tick.exchange, tick.datetime.timestamp());
            if !existing_keys.contains(&key) {
                data.push(tick);
            }
        }
        // Limit to 1M ticks
        if data.len() > 1_000_000 {
            let excess = data.len() - 1_000_000;
            data.drain(0..excess);
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

/// Persistent file-based database using JSON storage.
/// Stores bar and tick data in separate JSON files organized by symbol/exchange/interval.
/// Suitable for single-machine deployment where a full database server is not available.
/// 
/// File layout:
/// ```text
/// .rstrader/database/
///   bars/
///     BINANCE_BTCUSDT_1m.json
///     BINANCE_BTCUSDT_1h.json
///   ticks/
///     BINANCE_BTCUSDT.json
/// ```
pub struct FileDatabase {
    /// Base directory for database files
    base_dir: std::path::PathBuf,
}

impl FileDatabase {
    /// Create a new FileDatabase with the given base directory
    pub fn new(base_dir: std::path::PathBuf) -> Self {
        Self { base_dir }
    }

    /// Create a FileDatabase using the default data directory
    pub fn with_default_dir() -> Self {
        let base_dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("trade_engine")
            .join("database");
        Self::new(base_dir)
    }

    /// Get the file path for bar data
    fn bar_file_path(&self, symbol: &str, exchange: Exchange, interval: Interval) -> std::path::PathBuf {
        self.base_dir.join("bars")
            .join(format!("{}_{}_{}.json", exchange, symbol, interval.value()))
    }

    /// Get the file path for tick data
    fn tick_file_path(&self, symbol: &str, exchange: Exchange) -> std::path::PathBuf {
        self.base_dir.join("ticks")
            .join(format!("{}_{}.json", exchange, symbol))
    }

    /// Ensure the parent directory exists for a file path
    fn ensure_parent_dir(path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory {:?}: {}", parent, e))?;
        }
        Ok(())
    }

    /// Load bars from a JSON file
    fn load_bars_from_file(path: &std::path::Path) -> Result<Vec<BarData>, String> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {:?}: {}", path, e))
    }

    /// Save bars to a JSON file
    fn save_bars_to_file(path: &std::path::Path, bars: &[BarData]) -> Result<(), String> {
        Self::ensure_parent_dir(path)?;
        let content = serde_json::to_string(bars)
            .map_err(|e| format!("Failed to serialize bars: {}", e))?;
        std::fs::write(path, content)
            .map_err(|e| format!("Failed to write {:?}: {}", path, e))
    }

    /// Load ticks from a JSON file
    fn load_ticks_from_file(path: &std::path::Path) -> Result<Vec<TickData>, String> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {:?}: {}", path, e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {:?}: {}", path, e))
    }

    /// Save ticks to a JSON file
    fn save_ticks_to_file(path: &std::path::Path, ticks: &[TickData]) -> Result<(), String> {
        Self::ensure_parent_dir(path)?;
        let content = serde_json::to_string(ticks)
            .map_err(|e| format!("Failed to serialize ticks: {}", e))?;
        std::fs::write(path, content)
            .map_err(|e| format!("Failed to write {:?}: {}", path, e))
    }
}

#[async_trait]
impl BaseDatabase for FileDatabase {
    async fn save_bar_data(&self, bars: Vec<BarData>, stream: bool) -> Result<bool, String> {
        // Group bars by (symbol, exchange, interval)
        let mut groups: std::collections::HashMap<(String, Exchange, Option<Interval>), Vec<BarData>> =
            std::collections::HashMap::new();
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
            let path = self.bar_file_path(&symbol, exchange, interval);

            // Load existing data and merge (dedup by timestamp)
            let mut existing = Self::load_bars_from_file(&path)?;
            let existing_keys: std::collections::HashSet<i64> = existing.iter()
                .map(|b| b.datetime.timestamp())
                .collect();

            for bar in &new_bars {
                if !existing_keys.contains(&bar.datetime.timestamp()) {
                    existing.push(bar.clone());
                }
            }

            // Sort by datetime
            existing.sort_by_key(|b| b.datetime);

            // Limit to 1M bars per file
            if existing.len() > 1_000_000 {
                existing.drain(0..existing.len() - 1_000_000);
            }

            Self::save_bars_to_file(&path, &existing)?;

            if stream {
                tracing::debug!("Streamed {} bars to {:?}", new_bars.len(), path);
            }
        }

        Ok(true)
    }

    async fn save_tick_data(&self, ticks: Vec<TickData>, stream: bool) -> Result<bool, String> {
        // Group ticks by (symbol, exchange)
        let mut groups: std::collections::HashMap<(String, Exchange), Vec<TickData>> =
            std::collections::HashMap::new();
        for tick in ticks {
            groups
                .entry((tick.symbol.clone(), tick.exchange))
                .or_default()
                .push(tick);
        }

        for ((symbol, exchange), new_ticks) in groups {
            let path = self.tick_file_path(&symbol, exchange);

            // Load existing data and merge (dedup by timestamp)
            let mut existing = Self::load_ticks_from_file(&path)?;
            let existing_keys: std::collections::HashSet<i64> = existing.iter()
                .map(|t| t.datetime.timestamp())
                .collect();

            for tick in &new_ticks {
                if !existing_keys.contains(&tick.datetime.timestamp()) {
                    existing.push(tick.clone());
                }
            }

            // Sort by datetime
            existing.sort_by_key(|t| t.datetime);

            // Limit to 1M ticks per file
            if existing.len() > 1_000_000 {
                existing.drain(0..existing.len() - 1_000_000);
            }

            Self::save_ticks_to_file(&path, &existing)?;

            if stream {
                tracing::debug!("Streamed {} ticks to {:?}", new_ticks.len(), path);
            }
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
        let path = self.bar_file_path(symbol, exchange, interval);
        let all_bars = Self::load_bars_from_file(&path)?;

        let result: Vec<BarData> = all_bars
            .into_iter()
            .filter(|bar| {
                bar.symbol == symbol
                    && bar.exchange == exchange
                    && bar.interval == Some(interval)
                    && bar.datetime >= start
                    && bar.datetime <= end
            })
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
        let path = self.tick_file_path(symbol, exchange);
        let all_ticks = Self::load_ticks_from_file(&path)?;

        let result: Vec<TickData> = all_ticks
            .into_iter()
            .filter(|tick| {
                tick.symbol == symbol
                    && tick.exchange == exchange
                    && tick.datetime >= start
                    && tick.datetime <= end
            })
            .collect();

        Ok(result)
    }

    async fn delete_bar_data(
        &self,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
    ) -> Result<i64, String> {
        let path = self.bar_file_path(symbol, exchange, interval);
        if !path.exists() {
            return Ok(0);
        }

        let existing = Self::load_bars_from_file(&path)?;
        let count = existing.len() as i64;

        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete {:?}: {}", path, e))?;

        Ok(count)
    }

    async fn delete_tick_data(&self, symbol: &str, exchange: Exchange) -> Result<i64, String> {
        let path = self.tick_file_path(symbol, exchange);
        if !path.exists() {
            return Ok(0);
        }

        let existing = Self::load_ticks_from_file(&path)?;
        let count = existing.len() as i64;

        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete {:?}: {}", path, e))?;

        Ok(count)
    }

    async fn get_bar_overview(&self) -> Result<Vec<BarOverview>, String> {
        let bars_dir = self.base_dir.join("bars");
        if !bars_dir.exists() {
            return Ok(Vec::new());
        }

        let mut overviews = Vec::new();
        let entries = std::fs::read_dir(&bars_dir)
            .map_err(|e| format!("Failed to read bars directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let bars = Self::load_bars_from_file(&path)?;
            if bars.is_empty() {
                continue;
            }

            // Parse filename: EXCHANGE_SYMBOL_INTERVAL.json
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let parts: Vec<&str> = filename.splitn(3, '_').collect();
            if parts.len() < 3 {
                continue;
            }

            let count = bars.len() as i64;
            let start = bars.iter().map(|b| b.datetime).min();
            let end = bars.iter().map(|b| b.datetime).max();

            // Get exchange and interval from the data itself (authoritative source)
            let exchange = bars.first().map(|b| b.exchange);
            let interval = bars.first().and_then(|b| b.interval);

            overviews.push(BarOverview {
                symbol: parts[1].to_string(),
                exchange,
                interval,
                count,
                start,
                end,
            });
        }

        Ok(overviews)
    }

    async fn get_tick_overview(&self) -> Result<Vec<TickOverview>, String> {
        let ticks_dir = self.base_dir.join("ticks");
        if !ticks_dir.exists() {
            return Ok(Vec::new());
        }

        let mut overviews = Vec::new();
        let entries = std::fs::read_dir(&ticks_dir)
            .map_err(|e| format!("Failed to read ticks directory: {}", e))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let ticks = Self::load_ticks_from_file(&path)?;
            if ticks.is_empty() {
                continue;
            }

            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let parts: Vec<&str> = filename.splitn(2, '_').collect();
            if parts.len() < 2 {
                continue;
            }

            let count = ticks.len() as i64;
            let start = ticks.iter().map(|t| t.datetime).min();
            let end = ticks.iter().map(|t| t.datetime).max();

            // Get exchange from the data itself (authoritative source)
            let exchange = ticks.first().map(|t| t.exchange);

            overviews.push(TickOverview {
                symbol: parts[1].to_string(),
                exchange,
                count,
                start,
                end,
            });
        }

        Ok(overviews)
    }
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
        
        db.save_bar_data(vec![bar], false).await.expect("save_bar_data should succeed");
        
        let overviews = db.get_bar_overview().await.expect("get_bar_overview should succeed");
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
        
        db.save_tick_data(vec![tick], false).await.expect("save_tick_data should succeed");
        
        let overviews = db.get_tick_overview().await.expect("get_tick_overview should succeed");
        assert_eq!(overviews.len(), 1);
    }

    #[tokio::test]
    async fn test_file_database_bars() {
        let temp_dir = std::env::temp_dir().join("trade_engine_test_filedb_bars");
        let _ = std::fs::remove_dir_all(&temp_dir); // Clean up any previous test run
        let db = FileDatabase::new(temp_dir.clone());
        
        let bar = BarData::new(
            "test".to_string(),
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        
        db.save_bar_data(vec![bar], false).await.expect("save_bar_data should succeed");
        
        // Load the bar back
        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now() + chrono::Duration::hours(1);
        let bars = db.load_bar_data("BTCUSDT", Exchange::Binance, Interval::Minute, start, end)
            .await.expect("load_bar_data should succeed");
        assert_eq!(bars.len(), 1);
        
        // Check overview
        let overviews = db.get_bar_overview().await.expect("get_bar_overview should succeed");
        assert_eq!(overviews.len(), 1);
        assert_eq!(overviews[0].count, 1);
        
        // Delete
        let deleted = db.delete_bar_data("BTCUSDT", Exchange::Binance, Interval::Minute)
            .await.expect("delete_bar_data should succeed");
        assert_eq!(deleted, 1);
        
        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_file_database_dedup() {
        let temp_dir = std::env::temp_dir().join("trade_engine_test_filedb_dedup");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let db = FileDatabase::new(temp_dir.clone());
        
        let now = Utc::now();
        let bar1 = BarData {
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
        let bar2 = BarData {
            close_price: 50060.0,
            ..bar1.clone()
        };
        
        db.save_bar_data(vec![bar1], false).await.expect("save should succeed");
        db.save_bar_data(vec![bar2], false).await.expect("save should succeed"); // Same timestamp = dedup
        
        let start = now - chrono::Duration::hours(1);
        let end = now + chrono::Duration::hours(1);
        let bars = db.load_bar_data("BTCUSDT", Exchange::Binance, Interval::Minute, start, end)
            .await.expect("load should succeed");
        // Second save should not duplicate since timestamp is the same
        assert_eq!(bars.len(), 1);
        
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
