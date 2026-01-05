//! Datafeed module for connecting to different data sources.

use async_trait::async_trait;

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
        assert!(!result.unwrap());
        
        let req = HistoryRequest::new(
            "BTCUSDT".to_string(),
            Exchange::Binance,
            Utc::now(),
        );
        
        let result = datafeed.query_bar_history(req).await;
        assert!(result.is_err());
    }
}
