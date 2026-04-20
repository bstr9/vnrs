//! Datafeed module for connecting to different data sources.

use async_trait::async_trait;
use std::collections::HashMap;

use super::constant::{Exchange, Interval};
use super::object::{BarData, HistoryRequest, TickData};
use super::setting::SETTINGS;

/// Output function type for logging
pub type OutputFn = Box<dyn Fn(&str) + Send + Sync>;

/// Default output function that prints to stdout
pub fn default_output(msg: &str) {
    println!("{}", msg);
}

/// Abstract datafeed trait for connecting to different data sources
#[async_trait]
pub trait BaseDatafeed: Send + Sync {
    /// Initialize datafeed service connection
    async fn init(&self) -> Result<bool, String> {
        Ok(false)
    }

    /// Query history bar data
    async fn query_bar_history(&self, _req: HistoryRequest) -> Result<Vec<BarData>, String> {
        Err("查询K线数据失败：没有正确配置数据服务".to_string())
    }

    /// Query history tick data
    async fn query_tick_history(&self, _req: HistoryRequest) -> Result<Vec<TickData>, String> {
        Err("查询Tick数据失败：没有正确配置数据服务".to_string())
    }
}

/// Empty datafeed implementation for when no datafeed is configured
pub struct EmptyDatafeed;

impl EmptyDatafeed {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmptyDatafeed {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BaseDatafeed for EmptyDatafeed {
    async fn init(&self) -> Result<bool, String> {
        tracing::warn!("没有配置要使用的数据服务，请修改全局配置中的datafeed相关内容");
        Ok(false)
    }

    async fn query_bar_history(&self, _req: HistoryRequest) -> Result<Vec<BarData>, String> {
        Err("查询K线数据失败：没有正确配置数据服务".to_string())
    }

    async fn query_tick_history(&self, _req: HistoryRequest) -> Result<Vec<TickData>, String> {
        Err("查询Tick数据失败：没有正确配置数据服务".to_string())
    }
}

/// Binance datafeed implementation using REST API for historical data.
/// Supports both Spot and USDT-M Futures via the Binance klines API.
/// Does not require API keys for public historical data.
pub struct BinanceDatafeed {
    /// REST client for Binance API
    rest_client: crate::gateway::binance::BinanceRestClient,
    /// Whether using futures API
    futures: bool,
}

impl BinanceDatafeed {
    /// Create a new BinanceDatafeed for Spot
    pub fn new_spot() -> Self {
        Self {
            rest_client: crate::gateway::binance::BinanceRestClient::new().unwrap_or_default(),
            futures: false,
        }
    }

    /// Create a new BinanceDatafeed for USDT-M Futures
    pub fn new_futures() -> Self {
        Self {
            rest_client: crate::gateway::binance::BinanceRestClient::new().unwrap_or_default(),
            futures: true,
        }
    }

    /// Initialize the REST client with proxy and API settings from gateway config
    pub async fn init_with_config(&self, api_key: &str, api_secret: &str, proxy_host: &str, proxy_port: u16) {
        let host = if self.futures {
            crate::gateway::binance::USDT_REST_HOST
        } else {
            crate::gateway::binance::SPOT_REST_HOST
        };
        self.rest_client.init(api_key, api_secret, host, proxy_host, proxy_port).await
    }

    /// Get the appropriate klines endpoint based on market type
    fn klines_endpoint(&self) -> &'static str {
        if self.futures {
            "/fapi/v1/klines"
        } else {
            "/api/v3/klines"
        }
    }

    /// Get the appropriate exchange
    fn exchange(&self) -> Exchange {
        if self.futures {
            Exchange::BinanceUsdm
        } else {
            Exchange::Binance
        }
    }
}

#[async_trait]
impl BaseDatafeed for BinanceDatafeed {
    async fn init(&self) -> Result<bool, String> {
        tracing::info!(
            "BinanceDatafeed initialized ({})",
            if self.futures { "Futures" } else { "Spot" }
        );
        Ok(true)
    }

    async fn query_bar_history(&self, req: HistoryRequest) -> Result<Vec<BarData>, String> {
        use crate::gateway::binance::{INTERVAL_VT2BINANCE, get_interval_seconds, Security};

        let mut history = Vec::new();
        let limit = 1000;
        let mut start_time = req.start.timestamp() * 1000;
        let interval = req.interval.unwrap_or(Interval::Minute);
        let interval_str = INTERVAL_VT2BINANCE.get(&interval).unwrap_or(&"1m");
        let interval_ms = get_interval_seconds(interval) * 1000;

        loop {
            let mut params = HashMap::new();
            params.insert("symbol".to_string(), req.symbol.to_uppercase());
            params.insert("interval".to_string(), interval_str.to_string());
            params.insert("limit".to_string(), limit.to_string());
            params.insert("startTime".to_string(), start_time.to_string());
            if let Some(end) = req.end {
                params.insert("endTime".to_string(), (end.timestamp() * 1000).to_string());
            }

            let data = self.rest_client.get(
                self.klines_endpoint(),
                &params,
                Security::None,
            ).await?;

            let rows = match data.as_array() {
                Some(r) if !r.is_empty() => r,
                _ => break,
            };

            for row in rows {
                if let Some(arr) = row.as_array() {
                    let datetime = chrono::DateTime::from_timestamp_millis(arr[0].as_i64().unwrap_or(0))
                        .unwrap_or_else(chrono::Utc::now);
                    history.push(BarData {
                        symbol: req.symbol.clone(),
                        exchange: self.exchange(),
                        datetime,
                        interval: Some(interval),
                        volume: arr[5].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        turnover: arr[7].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        open_interest: if self.futures {
                            arr[9].as_str().unwrap_or("0").parse().unwrap_or(0.0)
                        } else {
                            0.0
                        },
                        open_price: arr[1].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        high_price: arr[2].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        low_price: arr[3].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        close_price: arr[4].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        gateway_name: if self.futures { "BINANCE_USDM" } else { "BINANCE_SPOT" }.to_string(),
                        extra: None,
                    });
                }
            }

            if rows.len() < limit {
                break;
            }
            if let Some(last) = rows.last().and_then(|r| r.as_array()) {
                start_time = last[0].as_i64().unwrap_or(0) + interval_ms;
            }
        }

        tracing::info!("BinanceDatafeed: 查询历史数据成功: {} 条", history.len());
        Ok(history)
    }

    async fn query_tick_history(&self, _req: HistoryRequest) -> Result<Vec<TickData>, String> {
        // Binance REST API doesn't provide tick-level historical data via klines
        Err("Binance REST API不支持Tick级别历史数据查询".to_string())
    }
}

/// Get the configured datafeed name
pub fn get_datafeed_name() -> String {
    SETTINGS.get_string("datafeed.name").unwrap_or_default()
}

/// Get datafeed username
pub fn get_datafeed_username() -> String {
    SETTINGS.get_string("datafeed.username").unwrap_or_default()
}

/// Get datafeed password
pub fn get_datafeed_password() -> String {
    SETTINGS.get_string("datafeed.password").unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::trader::constant::Exchange;

    #[tokio::test]
    async fn test_empty_datafeed() {
        let datafeed = EmptyDatafeed::new();
        
        let result = datafeed.init().await;
        assert!(!result.expect("empty datafeed init should return Ok(false)"));
        
        let req = HistoryRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        
        let result = datafeed.query_bar_history(req).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_binance_datafeed_creation() {
        let spot = BinanceDatafeed::new_spot();
        assert!(!spot.futures);

        let futures = BinanceDatafeed::new_futures();
        assert!(futures.futures);
    }

    #[tokio::test]
    async fn test_binance_datafeed_endpoints() {
        let spot = BinanceDatafeed::new_spot();
        assert_eq!(spot.klines_endpoint(), "/api/v3/klines");
        assert_eq!(spot.exchange(), Exchange::Binance);

        let futures = BinanceDatafeed::new_futures();
        assert_eq!(futures.klines_endpoint(), "/fapi/v1/klines");
        assert_eq!(futures.exchange(), Exchange::BinanceUsdm);
    }

    #[test]
    fn test_empty_datafeed_default() {
        let datafeed1 = EmptyDatafeed::new();
        let datafeed2 = EmptyDatafeed::default();
        // Both should construct without panic
        drop(datafeed1);
        drop(datafeed2);
    }

    #[tokio::test]
    async fn test_empty_datafeed_query_tick_history() {
        let datafeed = EmptyDatafeed::new();
        let req = HistoryRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        let result = datafeed.query_tick_history(req).await;
        assert!(result.is_err());
        assert!(result.expect_err("should be error").contains("没有正确配置数据服务"));
    }

    #[test]
    fn test_default_output_function() {
        // default_output should not panic
        default_output("test message");
    }

    #[tokio::test]
    async fn test_binance_datafeed_init() {
        let spot = BinanceDatafeed::new_spot();
        let result = spot.init().await;
        assert!(result.is_ok());
        assert!(result.expect("init should succeed"));

        let futures = BinanceDatafeed::new_futures();
        let result = futures.init().await;
        assert!(result.is_ok());
        assert!(result.expect("init should succeed"));
    }

    #[tokio::test]
    async fn test_binance_datafeed_query_tick_history_unsupported() {
        let datafeed = BinanceDatafeed::new_spot();
        let req = HistoryRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        let result = datafeed.query_tick_history(req).await;
        assert!(result.is_err());
        assert!(result.expect_err("should be error").contains("不支持Tick"));
    }
}
