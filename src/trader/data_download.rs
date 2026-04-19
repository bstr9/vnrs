//! Data download manager for fetching historical market data from exchanges.
//!
//! Provides functionality to:
//! - Download historical klines (candlestick) data from Binance
//! - Paginate through large date ranges automatically
//! - Convert raw API data to BarData and persist to database
//! - Track download progress

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

use super::constant::{Exchange, Interval};
use super::engine::BaseEngine;
use super::gateway::GatewayEvent;
use super::object::BarData;

/// Configuration for data download operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadConfig {
    /// Maximum number of concurrent download tasks
    pub max_concurrency: usize,
    /// Maximum retries per request on failure
    pub max_retries: u8,
    /// Delay between paginated requests in milliseconds
    pub request_delay_ms: u64,
    /// Bars per page (Binance max is 1000)
    pub page_size: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 3,
            max_retries: 3,
            request_delay_ms: 200,
            page_size: 1000,
        }
    }
}

/// Progress tracking for a download operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    /// Symbol being downloaded
    pub symbol: String,
    /// Exchange
    pub exchange: Exchange,
    /// Interval
    pub interval: Interval,
    /// Number of bars downloaded so far
    pub bars_downloaded: usize,
    /// Estimated total bars (may be 0 if unknown)
    pub estimated_total: usize,
    /// Start time of the download range
    pub start: Option<DateTime<Utc>>,
    /// End time of the download range
    pub end: Option<DateTime<Utc>>,
    /// Whether the download is complete
    pub complete: bool,
    /// Error message if download failed
    pub error: Option<String>,
}

impl Default for DownloadProgress {
    fn default() -> Self {
        Self {
            symbol: String::new(),
            exchange: Exchange::Binance,
            interval: Interval::Minute,
            bars_downloaded: 0,
            estimated_total: 0,
            start: None,
            end: None,
            complete: false,
            error: None,
        }
    }
}

impl DownloadProgress {
    /// Calculate completion percentage
    pub fn percentage(&self) -> f64 {
        if self.estimated_total == 0 {
            return 0.0;
        }
        (self.bars_downloaded as f64 / self.estimated_total as f64) * 100.0
    }
}

/// Result of a download operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResult {
    /// Symbol downloaded
    pub symbol: String,
    /// Exchange
    pub exchange: Exchange,
    /// Interval
    pub interval: Interval,
    /// Number of bars downloaded
    pub bars_count: usize,
    /// Start time of downloaded data
    pub start: DateTime<Utc>,
    /// End time of downloaded data
    pub end: DateTime<Utc>,
}

/// Map Interval to Binance API interval string
fn interval_to_binance(interval: Interval) -> &'static str {
    match interval {
        Interval::Second => "1s",
        Interval::Minute => "1m",
        Interval::Minute5 => "5m",
        Interval::Minute15 => "15m",
        Interval::Minute30 => "30m",
        Interval::Hour => "1h",
        Interval::Hour4 => "4h",
        Interval::Daily => "1d",
        Interval::Weekly => "1w",
        Interval::Tick => "1m", // Fallback — tick not supported by klines API
    }
}

/// Get the klines API path for a given exchange
fn klines_path(exchange: Exchange) -> &'static str {
    match exchange {
        Exchange::BinanceUsdm => "/fapi/v1/klines",
        Exchange::BinanceCoinm => "/dapi/v1/klines",
        _ => "/api/v3/klines", // Default to Spot
    }
}

/// Data download manager engine
///
/// Manages downloading of historical market data from exchanges.
/// Implements BaseEngine for integration with the event system.
pub struct DataDownloadManager {
    /// Engine name
    name: String,
    /// Download configuration
    config: RwLock<DownloadConfig>,
    /// Active download progress by download key (symbol.exchange.interval)
    progress: RwLock<HashMap<String, DownloadProgress>>,
    /// Total bars downloaded across all operations
    total_downloaded: AtomicU64,
    /// Running flag
    running: AtomicBool,
}

impl DataDownloadManager {
    /// Create a new DataDownloadManager with default configuration
    pub fn new() -> Self {
        Self::with_config(DownloadConfig::default())
    }

    /// Create a new DataDownloadManager with custom configuration
    pub fn with_config(config: DownloadConfig) -> Self {
        Self {
            name: "DataDownloadManager".to_string(),
            config: RwLock::new(config),
            progress: RwLock::new(HashMap::new()),
            total_downloaded: AtomicU64::new(0),
            running: AtomicBool::new(false),
        }
    }

    /// Update download configuration
    pub fn update_config(&self, config: DownloadConfig) {
        let mut current = self.config.write().unwrap_or_else(|e| e.into_inner());
        *current = config;
        info!("[DataDownloadManager] Configuration updated");
    }

    /// Get current configuration
    pub fn get_config(&self) -> DownloadConfig {
        self.config.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Get progress for a specific download
    pub fn get_progress(&self, symbol: &str, exchange: Exchange, interval: Interval) -> Option<DownloadProgress> {
        let key = format!("{}.{}.{}", symbol, exchange.value(), interval.value());
        let progress = self.progress.read().unwrap_or_else(|e| e.into_inner());
        progress.get(&key).cloned()
    }

    /// Get all active download progress
    pub fn get_all_progress(&self) -> Vec<DownloadProgress> {
        let progress = self.progress.read().unwrap_or_else(|e| e.into_inner());
        progress.values().cloned().collect()
    }

    /// Get total bars downloaded
    pub fn get_total_downloaded(&self) -> u64 {
        self.total_downloaded.load(Ordering::Relaxed)
    }

    /// Cancel an active download by setting running flag to false.
    /// The download loop checks this flag on each iteration and will
    /// stop gracefully.
    pub fn cancel_download(&self) {
        self.running.store(false, Ordering::SeqCst);
        info!("[DataDownloadManager] Download cancellation requested");
    }

    /// Check if a download is currently running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Download historical klines from Binance REST API
    ///
    /// Automatically paginates through the date range, converting
    /// raw klines to BarData. Does NOT persist to database —
    /// the caller is responsible for saving via BaseDatabase.
    ///
    /// # Arguments
    /// * `rest_client` - BinanceRestClient for making API requests
    /// * `symbol` - Trading symbol (e.g., "BTCUSDT")
    /// * `exchange` - Exchange (Binance for Spot, BinanceUsdm for Futures)
    /// * `interval` - Bar interval
    /// * `start` - Start datetime
    /// * `end` - End datetime
    ///
    /// # Returns
    /// Vector of BarData or error string
    pub async fn download_klines(
        &self,
        rest_client: &crate::gateway::binance::BinanceRestClient,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<BarData>, String> {
        let key = format!("{}.{}.{}", symbol, exchange.value(), interval.value());
        let config = self.config.read().unwrap_or_else(|e| e.into_inner()).clone();

        // Initialize progress tracking
        {
            let mut progress = self.progress.write().unwrap_or_else(|e| e.into_inner());
            progress.insert(key.clone(), DownloadProgress {
                symbol: symbol.to_string(),
                exchange,
                interval,
                bars_downloaded: 0,
                estimated_total: 0,
                start: Some(start),
                end: Some(end),
                complete: false,
                error: None,
            });
        }

        info!(
            "[DataDownloadManager] Starting kline download: {} {} {} from {} to {}",
            symbol, exchange.value(), interval.value(),
            start.format("%Y-%m-%d %H:%M"), end.format("%Y-%m-%d %H:%M")
        );

        let path = klines_path(exchange);
        let binance_interval = interval_to_binance(interval);
        let gateway_name = match exchange {
            Exchange::BinanceUsdm => "BINANCE_USDT",
            _ => "BINANCE_SPOT",
        };

        let mut all_bars: Vec<BarData> = Vec::new();
        let mut current_start = start.timestamp_millis();
        let end_ms = end.timestamp_millis();
        let mut retry_count: u8 = 0;

        // Mark running state
        self.running.store(true, Ordering::SeqCst);

        while current_start < end_ms {
            // Check if download was cancelled
            if !self.running.load(Ordering::SeqCst) {
                info!("[DataDownloadManager] Download cancelled for {}", symbol);
                // Mark progress as complete with cancellation note
                let mut progress = self.progress.write().unwrap_or_else(|e| e.into_inner());
                if let Some(p) = progress.get_mut(&key) {
                    p.complete = true;
                    p.error = Some("Download cancelled".to_string());
                }
                return Err(format!("Download cancelled for {}", symbol));
            }

            let mut params = HashMap::new();
            params.insert("symbol".to_string(), symbol.to_uppercase());
            params.insert("interval".to_string(), binance_interval.to_string());
            params.insert("startTime".to_string(), current_start.to_string());
            params.insert("endTime".to_string(), end_ms.to_string());
            params.insert("limit".to_string(), config.page_size.to_string());

            match rest_client.get(path, &params, crate::gateway::binance::Security::None).await {
                Ok(response) => {
                    retry_count = 0; // Reset on success

                    let klines = response.as_array()
                        .ok_or_else(|| "Invalid klines response: expected array".to_string())?;

                    if klines.is_empty() {
                        info!("[DataDownloadManager] No more data returned, download complete");
                        break;
                    }

                    let page_count = klines.len();
                    for kline in klines {
                        let arr = kline.as_array()
                            .ok_or_else(|| "Invalid kline entry: expected array".to_string())?;

                        if arr.len() < 12 {
                            continue;
                        }

                        let open_time_ms = arr[0].as_i64().unwrap_or(0);
                        let open_price: f64 = arr[1].as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let high_price: f64 = arr[2].as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let low_price: f64 = arr[3].as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let close_price: f64 = arr[4].as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let volume: f64 = arr[5].as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);
                        let turnover: f64 = arr[7].as_str()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0.0);

                        let datetime = chrono::DateTime::from_timestamp_millis(open_time_ms)
                            .unwrap_or_else(Utc::now);

                        let bar = BarData {
                            gateway_name: gateway_name.to_string(),
                            symbol: symbol.to_string(),
                            exchange,
                            datetime,
                            interval: Some(interval),
                            volume,
                            turnover,
                            open_interest: 0.0,
                            open_price,
                            high_price,
                            low_price,
                            close_price,
                            extra: None,
                        };

                        all_bars.push(bar);
                    }

                    // Update progress
                    {
                        let mut progress = self.progress.write().unwrap_or_else(|e| e.into_inner());
                        if let Some(p) = progress.get_mut(&key) {
                            p.bars_downloaded += page_count;
                        }
                    }
                    self.total_downloaded.fetch_add(page_count as u64, Ordering::Relaxed);

                    // Move to next page: use close_time + 1ms from the last kline
                    if let Some(last) = klines.last() {
                        if let Some(last_arr) = last.as_array() {
                            // close_time is at index 6
                            let close_time_ms = last_arr[6].as_i64().unwrap_or(0);
                            current_start = close_time_ms + 1;
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }

                    // Rate limiting delay
                    if config.request_delay_ms > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(config.request_delay_ms)).await;
                    }

                    info!(
                        "[DataDownloadManager] Downloaded {} bars for {} (total: {})",
                        page_count, symbol, all_bars.len()
                    );
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count > config.max_retries {
                        let err_msg = format!("Download failed after {} retries: {}", config.max_retries, e);
                        error!("[DataDownloadManager] {}", err_msg);

                        // Update progress with error
                        let mut progress = self.progress.write().unwrap_or_else(|e| e.into_inner());
                        if let Some(p) = progress.get_mut(&key) {
                            p.error = Some(err_msg.clone());
                            p.complete = true;
                        }

                        self.running.store(false, Ordering::SeqCst);
                        return Err(err_msg);
                    }
                    warn!(
                        "[DataDownloadManager] Request failed (attempt {}/{}): {}",
                        retry_count, config.max_retries, e
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        }

        // Mark progress as complete
        {
            let mut progress = self.progress.write().unwrap_or_else(|e| e.into_inner());
            if let Some(p) = progress.get_mut(&key) {
                p.complete = true;
                p.bars_downloaded = all_bars.len();
            }
        }

        // Clear running state
        self.running.store(false, Ordering::SeqCst);

        // Deduplicate bars by datetime (overlapping pages may produce duplicates)
        all_bars.sort_by_key(|b| b.datetime);
        all_bars.dedup_by_key(|b| b.datetime);

        info!(
            "[DataDownloadManager] Download complete: {} bars for {} {} {}",
            all_bars.len(), symbol, exchange.value(), interval.value()
        );

        Ok(all_bars)
    }

    /// Convert raw kline JSON array to BarData
    ///
    /// Utility method for converting individual kline responses.
    pub fn kline_to_bar(
        kline: &serde_json::Value,
        symbol: &str,
        exchange: Exchange,
        interval: Interval,
        gateway_name: &str,
    ) -> Option<BarData> {
        let arr = kline.as_array()?;
        if arr.len() < 12 {
            return None;
        }

        let open_time_ms = arr[0].as_i64()?;
        let open_price: f64 = arr[1].as_str()?.parse().ok()?;
        let high_price: f64 = arr[2].as_str()?.parse().ok()?;
        let low_price: f64 = arr[3].as_str()?.parse().ok()?;
        let close_price: f64 = arr[4].as_str()?.parse().ok()?;
        let volume: f64 = arr[5].as_str()?.parse().ok()?;
        let turnover: f64 = arr[7].as_str()?.parse().ok()?;

        let datetime = chrono::DateTime::from_timestamp_millis(open_time_ms)?;

        Some(BarData {
            gateway_name: gateway_name.to_string(),
            symbol: symbol.to_string(),
            exchange,
            datetime,
            interval: Some(interval),
            volume,
            turnover,
            open_interest: 0.0,
            open_price,
            high_price,
            low_price,
            close_price,
            extra: None,
        })
    }
}

impl Default for DataDownloadManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BaseEngine for DataDownloadManager {
    fn engine_name(&self) -> &str {
        &self.name
    }

    fn close(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn process_event(&self, _event_type: &str, _event: &GatewayEvent) {
        // DataDownloadManager doesn't need to process real-time events
        // It's driven by explicit download requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_config_default() {
        let config = DownloadConfig::default();
        assert_eq!(config.max_concurrency, 3);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.page_size, 1000);
    }

    #[test]
    fn test_download_progress_percentage() {
        let progress = DownloadProgress {
            symbol: "BTCUSDT".to_string(),
            exchange: Exchange::Binance,
            interval: Interval::Minute,
            bars_downloaded: 500,
            estimated_total: 1000,
            start: None,
            end: None,
            complete: false,
            error: None,
        };
        assert!((progress.percentage() - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_download_progress_zero_total() {
        let progress = DownloadProgress::default();
        assert_eq!(progress.percentage(), 0.0);
    }

    #[test]
    fn test_interval_to_binance() {
        assert_eq!(interval_to_binance(Interval::Minute), "1m");
        assert_eq!(interval_to_binance(Interval::Hour), "1h");
        assert_eq!(interval_to_binance(Interval::Daily), "1d");
        assert_eq!(interval_to_binance(Interval::Minute5), "5m");
        assert_eq!(interval_to_binance(Interval::Hour4), "4h");
        assert_eq!(interval_to_binance(Interval::Weekly), "1w");
    }

    #[test]
    fn test_klines_path() {
        assert_eq!(klines_path(Exchange::Binance), "/api/v3/klines");
        assert_eq!(klines_path(Exchange::BinanceUsdm), "/fapi/v1/klines");
        assert_eq!(klines_path(Exchange::BinanceCoinm), "/dapi/v1/klines");
    }

    #[test]
    fn test_data_download_manager_new() {
        let manager = DataDownloadManager::new();
        assert_eq!(manager.engine_name(), "DataDownloadManager");
        assert_eq!(manager.get_total_downloaded(), 0);
    }

    #[test]
    fn test_data_download_manager_config() {
        let mut config = DownloadConfig::default();
        config.max_concurrency = 5;
        config.page_size = 500;
        let manager = DataDownloadManager::with_config(config);
        let retrieved = manager.get_config();
        assert_eq!(retrieved.max_concurrency, 5);
        assert_eq!(retrieved.page_size, 500);
    }

    #[test]
    fn test_kline_to_bar_valid() {
        let kline = serde_json::json!([
            1672531200000_i64,  // open_time
            "50000.00",         // open
            "51000.00",         // high
            "49000.00",         // low
            "50500.00",         // close
            "100.5",            // volume
            1672531259999_i64,  // close_time
            "5050000.0",        // quote_volume
            1000,               // trades
            "50.0",             // taker_buy_base
            "2500000.0",        // taker_buy_quote
            "0"                  // ignore
        ]);

        let bar = DataDownloadManager::kline_to_bar(
            &kline, "BTCUSDT", Exchange::Binance, Interval::Minute, "BINANCE_SPOT"
        );

        assert!(bar.is_some());
        let bar = bar.unwrap();
        assert_eq!(bar.symbol, "BTCUSDT");
        assert_eq!(bar.exchange, Exchange::Binance);
        assert!((bar.open_price - 50000.0).abs() < 0.01);
        assert!((bar.high_price - 51000.0).abs() < 0.01);
        assert!((bar.low_price - 49000.0).abs() < 0.01);
        assert!((bar.close_price - 50500.0).abs() < 0.01);
        assert!((bar.volume - 100.5).abs() < 0.01);
        assert_eq!(bar.interval, Some(Interval::Minute));
    }

    #[test]
    fn test_kline_to_bar_invalid() {
        let kline = serde_json::json!([1, "invalid"]);
        let bar = DataDownloadManager::kline_to_bar(
            &kline, "BTCUSDT", Exchange::Binance, Interval::Minute, "BINANCE_SPOT"
        );
        assert!(bar.is_none());

        // Not an array
        let kline = serde_json::json!("not an array");
        let bar = DataDownloadManager::kline_to_bar(
            &kline, "BTCUSDT", Exchange::Binance, Interval::Minute, "BINANCE_SPOT"
        );
        assert!(bar.is_none());
    }

    #[test]
    fn test_get_progress_empty() {
        let manager = DataDownloadManager::new();
        let progress = manager.get_progress("BTCUSDT", Exchange::Binance, Interval::Minute);
        assert!(progress.is_none());
    }

    #[test]
    fn test_get_all_progress_empty() {
        let manager = DataDownloadManager::new();
        let all = manager.get_all_progress();
        assert!(all.is_empty());
    }
}
