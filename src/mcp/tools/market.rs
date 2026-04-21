//! MCP Market Data Tools — 行情数据查询工具集
//!
//! 提供 get_ticker / get_orderbook / get_candles / get_trades 等 MCP Tool，
//! 通过 MainEngine 的 OmsEngine 缓存读取实时行情数据。

use rmcp::{
    ErrorData as McpError,
    model::*,
    tool, tool_router,
    handler::server::wrapper::Parameters,
    schemars,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::trader::MainEngine;
use super::super::server::TradingMcpServer;

// ---- 参数结构体 ----

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SymbolParams {
    /// 标的符号（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub symbol: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct GetCandlesParams {
    /// 标的符号（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub symbol: String,
    /// 最大返回条数（默认 100）
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct GetTicker24hParams {
    /// 标的符号（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub symbol: String,
}

fn default_limit() -> usize {
    100
}

// ---- MarketTools (data holder, no longer has #[tool_router]) ----

/// 行情数据查询数据容器（已迁移到 TradingMcpServer 的 #[tool_router] impl）
#[allow(dead_code)]
pub struct MarketTools {
    engine: Arc<MainEngine>,
}

#[allow(dead_code)]
impl MarketTools {
    /// 创建 MarketTools 实例
    pub fn new(engine: Arc<MainEngine>) -> Self {
        Self { engine }
    }
}

// ---- TradingMcpServer tool router for market tools ----

#[tool_router(router = market_router, vis = "pub")]
impl TradingMcpServer {
    #[tool(description = "Get latest ticker data for a symbol")]
    fn get_ticker(
        &self,
        Parameters(params): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_tick(&params.symbol) {
            Some(tick) => {
                let summary = serde_json::json!({
                    "symbol": tick.vt_symbol(),
                    "last_price": tick.last_price,
                    "volume": tick.volume,
                    "turnover": tick.turnover,
                    "open_price": tick.open_price,
                    "high_price": tick.high_price,
                    "low_price": tick.low_price,
                    "pre_close": tick.pre_close,
                    "open_interest": tick.open_interest,
                    "bid_price_1": tick.bid_price_1,
                    "bid_volume_1": tick.bid_volume_1,
                    "ask_price_1": tick.ask_price_1,
                    "ask_volume_1": tick.ask_volume_1,
                    "datetime": tick.datetime.to_rfc3339(),
                });
                let text = serde_json::to_string_pretty(&summary)
                    .unwrap_or_else(|_| "Ticker data retrieved".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No tick data available for symbol: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get order book depth for a symbol (5-level from tick data)")]
    fn get_orderbook(
        &self,
        Parameters(params): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_tick(&params.symbol) {
            Some(tick) => {
                let orderbook = serde_json::json!({
                    "symbol": tick.vt_symbol(),
                    "datetime": tick.datetime.to_rfc3339(),
                    "bids": [
                        { "price": tick.bid_price_1, "volume": tick.bid_volume_1 },
                        { "price": tick.bid_price_2, "volume": tick.bid_volume_2 },
                        { "price": tick.bid_price_3, "volume": tick.bid_volume_3 },
                        { "price": tick.bid_price_4, "volume": tick.bid_volume_4 },
                        { "price": tick.bid_price_5, "volume": tick.bid_volume_5 },
                    ],
                    "asks": [
                        { "price": tick.ask_price_1, "volume": tick.ask_volume_1 },
                        { "price": tick.ask_price_2, "volume": tick.ask_volume_2 },
                        { "price": tick.ask_price_3, "volume": tick.ask_volume_3 },
                        { "price": tick.ask_price_4, "volume": tick.ask_volume_4 },
                        { "price": tick.ask_price_5, "volume": tick.ask_volume_5 },
                    ],
                });
                let text = serde_json::to_string_pretty(&orderbook)
                    .unwrap_or_else(|_| "Orderbook data retrieved".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No tick data available for order book: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get candlestick (bar) data for a symbol")]
    fn get_candles(
        &self,
        Parameters(params): Parameters<GetCandlesParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_bar(&params.symbol) {
            Some(bar) => {
                let summary = serde_json::json!({
                    "symbol": bar.vt_symbol(),
                    "interval": bar.interval.map(|i| i.value().to_string()).unwrap_or_default(),
                    "open_price": bar.open_price,
                    "high_price": bar.high_price,
                    "low_price": bar.low_price,
                    "close_price": bar.close_price,
                    "volume": bar.volume,
                    "turnover": bar.turnover,
                    "open_interest": bar.open_interest,
                    "datetime": bar.datetime.to_rfc3339(),
                });
                let text = serde_json::to_string_pretty(&summary)
                    .unwrap_or_else(|_| "Bar data retrieved".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No bar data available for symbol: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get recent trades for all symbols")]
    fn get_trades(&self) -> Result<CallToolResult, McpError> {
        let trades = self.engine.get_all_trades();
        if trades.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No trade data available".to_string(),
            )]));
        }
        let summary: Vec<serde_json::Value> = trades
            .iter()
            .map(|t| {
                serde_json::json!({
                    "vt_tradeid": t.vt_tradeid(),
                    "symbol": t.vt_symbol(),
                    "direction": format!("{:?}", t.direction),
                    "offset": format!("{}", t.offset),
                    "price": t.price,
                    "volume": t.volume,
                    "datetime": t.datetime.map(|d| d.to_rfc3339()).unwrap_or_default(),
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|_| format!("{} trades", summary.len()));
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Get funding rate for a symbol (returns from tick extra fields if available)")]
    fn get_funding_rate(
        &self,
        Parameters(params): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_tick(&params.symbol) {
            Some(tick) => {
                let funding_rate = tick.extra.as_ref()
                    .and_then(|e| e.get("funding_rate"))
                    .map(|v| v.as_str());
                match funding_rate {
                    Some(rate) => Ok(CallToolResult::success(vec![Content::text(format!(
                        "Funding rate for {}: {}",
                        params.symbol, rate
                    ))])),
                    None => Ok(CallToolResult::success(vec![Content::text(format!(
                        "Funding rate not available for {}. This data is only available for perpetual futures contracts with funding rate support.",
                        params.symbol
                    ))])),
                }
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No tick data available for symbol: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get mark price for a symbol (returns last price as mark price estimate)")]
    fn get_mark_price(
        &self,
        Parameters(params): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_tick(&params.symbol) {
            Some(tick) => {
                let mark_price = tick.extra.as_ref()
                    .and_then(|e| e.get("mark_price"))
                    .map(|v| v.as_str());
                let result = serde_json::json!({
                    "symbol": tick.vt_symbol(),
                    "mark_price": mark_price.unwrap_or("N/A"),
                    "last_price": tick.last_price,
                    "note": if mark_price.is_some() { "Exchange-provided mark price" } else { "Mark price not available from exchange; last_price shown as estimate" },
                });
                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "Mark price data".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No tick data available for symbol: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get index price for a symbol (returns from tick extra fields if available)")]
    fn get_index_price(
        &self,
        Parameters(params): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_tick(&params.symbol) {
            Some(tick) => {
                let index_price = tick.extra.as_ref()
                    .and_then(|e| e.get("index_price"))
                    .map(|v| v.as_str());
                match index_price {
                    Some(price) => Ok(CallToolResult::success(vec![Content::text(format!(
                        "Index price for {}: {}",
                        params.symbol, price
                    ))])),
                    None => Ok(CallToolResult::success(vec![Content::text(format!(
                        "Index price not available for {}. This data is only available for futures contracts with index price support.",
                        params.symbol
                    ))])),
                }
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No tick data available for symbol: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get liquidation data for a symbol (returns from tick extra fields if available)")]
    fn get_liquidations(
        &self,
        Parameters(params): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_tick(&params.symbol) {
            Some(tick) => {
                let liquidations = tick.extra.as_ref()
                    .and_then(|e| e.get("liquidations"))
                    .map(|v| v.as_str());
                match liquidations {
                    Some(data) => Ok(CallToolResult::success(vec![Content::text(format!(
                        "Liquidation data for {}: {}",
                        params.symbol, data
                    ))])),
                    None => Ok(CallToolResult::success(vec![Content::text(format!(
                        "Liquidation data not available for {}. This is only available for futures contracts with liquidation feed support.",
                        params.symbol
                    ))])),
                }
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No tick data available for symbol: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get open interest for a symbol")]
    fn get_open_interest(
        &self,
        Parameters(params): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_tick(&params.symbol) {
            Some(tick) => {
                let result = serde_json::json!({
                    "symbol": tick.vt_symbol(),
                    "open_interest": tick.open_interest,
                    "datetime": tick.datetime.to_rfc3339(),
                });
                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "Open interest data".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No tick data available for symbol: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get 24h ticker statistics for a symbol")]
    fn get_ticker_24h(
        &self,
        Parameters(params): Parameters<GetTicker24hParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_tick(&params.symbol) {
            Some(tick) => {
                let change = if tick.pre_close > 0.0 {
                    tick.last_price - tick.pre_close
                } else {
                    0.0
                };
                let change_pct = if tick.pre_close > 0.0 {
                    (change / tick.pre_close) * 100.0
                } else {
                    0.0
                };
                let result = serde_json::json!({
                    "symbol": tick.vt_symbol(),
                    "last_price": tick.last_price,
                    "open_price": tick.open_price,
                    "high_price": tick.high_price,
                    "low_price": tick.low_price,
                    "pre_close": tick.pre_close,
                    "volume": tick.volume,
                    "turnover": tick.turnover,
                    "change": change,
                    "change_pct": format!("{:.2}%", change_pct),
                    "open_interest": tick.open_interest,
                    "datetime": tick.datetime.to_rfc3339(),
                });
                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "24h ticker data".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No tick data available for symbol: {}",
                params.symbol
            ))])),
        }
    }
}
