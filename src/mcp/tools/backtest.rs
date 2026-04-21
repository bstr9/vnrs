//! MCP Backtest Tools — 回测管理工具集
//!
//! 提供 run_backtest / get_backtest_result / list_backtests / compare_strategies
//! 等 MCP Tool，通过 BacktestingEngine 执行策略回测。
//!
//! NOTE: Backtest tools manage backtest runs in-memory. Results are stored
//! in a local cache keyed by backtest ID.

use rmcp::{
    ErrorData as McpError,
    model::*,
    tool, tool_router,
    handler::server::wrapper::Parameters,
    schemars,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::super::server::TradingMcpServer;

// ---- 参数结构体 ----

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RunBacktestParams {
    /// 标的符号（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub symbol: String,
    /// K 线周期：1m / 5m / 15m / 1h / 4h / 1d
    pub interval: String,
    /// 起始时间（ISO 8601）
    pub start: String,
    /// 结束时间（ISO 8601）
    pub end: String,
    /// 初始资金（默认 1,000,000）
    #[serde(default = "default_capital")]
    pub capital: f64,
    /// 佣金率（默认 0.0002）
    #[serde(default = "default_rate")]
    pub rate: f64,
    /// 滑点（默认 0）
    #[serde(default)]
    pub slippage: f64,
    /// 合约乘数（默认 1）
    #[serde(default = "default_size")]
    pub size: f64,
    /// 最小价格变动（默认 0.01）
    #[serde(default = "default_pricetick")]
    pub pricetick: f64,
    /// 策略名称（可选，用于标识）
    #[serde(default)]
    pub strategy_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct BacktestIdParams {
    /// 回测 ID
    pub backtest_id: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct CompareStrategiesParams {
    /// 要比较的回测 ID 列表（JSON 数组）
    pub backtest_ids: String,
}

fn default_capital() -> f64 { 1_000_000.0 }
fn default_rate() -> f64 { 0.0002 }
fn default_size() -> f64 { 1.0 }
fn default_pricetick() -> f64 { 0.01 }

// ---- Backtest Result Cache ----

/// Stored backtest result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BacktestEntry {
    /// Unique backtest ID
    pub id: String,
    /// Strategy name (if provided)
    pub strategy_name: Option<String>,
    /// Symbol
    pub symbol: String,
    /// Interval
    pub interval: String,
    /// Start time
    pub start: String,
    /// End time
    pub end: String,
    /// Capital
    pub capital: f64,
    /// Status: pending / running / completed / failed
    pub status: String,
    /// Result (if completed)
    pub result: Option<serde_json::Value>,
}

// ---- BacktestTools (data holder, no longer has #[tool_router]) ----

/// 回测管理数据容器（已迁移到 TradingMcpServer 的 #[tool_router] impl）
#[allow(dead_code)]
pub struct BacktestTools {
    results: Arc<RwLock<Vec<BacktestEntry>>>,
}

#[allow(dead_code)]
impl BacktestTools {
    /// 创建 BacktestTools 实例
    pub fn new() -> Self {
        Self {
            results: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for BacktestTools {
    fn default() -> Self {
        Self::new()
    }
}

// ---- TradingMcpServer tool router for backtest tools ----

#[tool_router(router = backtest_router, vis = "pub")]
impl TradingMcpServer {
    #[tool(description = "Run a backtest with specified parameters")]
    async fn run_backtest(
        &self,
        Parameters(params): Parameters<RunBacktestParams>,
    ) -> Result<CallToolResult, McpError> {
        // Generate a unique backtest ID
        let backtest_id = format!(
            "bt_{}_{}",
            params.symbol.replace('.', "_"),
            chrono::Utc::now().timestamp_millis()
        );

        // Create a new entry
        let entry = BacktestEntry {
            id: backtest_id.clone(),
            strategy_name: params.strategy_name.clone(),
            symbol: params.symbol.clone(),
            interval: params.interval.clone(),
            start: params.start.clone(),
            end: params.end.clone(),
            capital: params.capital,
            status: "pending".to_string(),
            result: None,
        };

        // Store the entry
        {
            let mut results = self.backtest_cache.write().await;
            results.push(entry);
        }

        // NOTE: Actual backtest execution would require creating a BacktestingEngine
        // with market data and strategy. This tool sets up the backtest configuration.
        // The actual run is deferred to the application layer.
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Backtest configured: id={}, symbol={}, interval={}, capital={}, rate={}, slippage={}\n\
             Status: pending\n\
             Note: Backtest execution requires a configured strategy and market data. \
             Use the backtesting engine API to load data and execute the strategy.",
            backtest_id, params.symbol, params.interval, params.capital, params.rate, params.slippage
        ))]))
    }

    #[tool(description = "Get results of a completed backtest")]
    async fn get_backtest_result(
        &self,
        Parameters(params): Parameters<BacktestIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let results = self.backtest_cache.read().await;
        let entry = results.iter().find(|e| e.id == params.backtest_id);

        match entry {
            Some(e) => {
                let summary = serde_json::json!({
                    "id": e.id,
                    "strategy_name": e.strategy_name,
                    "symbol": e.symbol,
                    "interval": e.interval,
                    "start": e.start,
                    "end": e.end,
                    "capital": e.capital,
                    "status": e.status,
                    "result": e.result,
                });
                let text = serde_json::to_string_pretty(&summary)
                    .unwrap_or_else(|_| "Backtest result".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "Backtest not found: {}",
                params.backtest_id
            ))])),
        }
    }

    #[tool(description = "List all backtest runs")]
    async fn list_backtests(&self) -> Result<CallToolResult, McpError> {
        let results = self.backtest_cache.read().await;
        if results.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No backtests found.".to_string(),
            )]));
        }

        let summary: Vec<serde_json::Value> = results
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "strategy_name": e.strategy_name,
                    "symbol": e.symbol,
                    "interval": e.interval,
                    "status": e.status,
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|_| format!("{} backtests", summary.len()));
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Compare results from multiple backtest runs")]
    async fn compare_strategies(
        &self,
        Parameters(params): Parameters<CompareStrategiesParams>,
    ) -> Result<CallToolResult, McpError> {
        let ids: Vec<String> = serde_json::from_str(&params.backtest_ids)
            .map_err(|e| McpError::invalid_params(format!("Invalid backtest_ids JSON: {}", e), None))?;

        let results = self.backtest_cache.read().await;
        let matched: Vec<&BacktestEntry> = results
            .iter()
            .filter(|e| ids.contains(&e.id))
            .collect();

        if matched.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No matching backtests found for the provided IDs.".to_string(),
            )]));
        }

        let comparison: Vec<serde_json::Value> = matched
            .iter()
            .map(|e| {
                serde_json::json!({
                    "id": e.id,
                    "strategy_name": e.strategy_name,
                    "symbol": e.symbol,
                    "interval": e.interval,
                    "capital": e.capital,
                    "status": e.status,
                    "result": e.result,
                })
            })
            .collect();

        let text = serde_json::to_string_pretty(&comparison)
            .unwrap_or_else(|_| "Strategy comparison".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
