//! MCP Account Tools — 账户与持仓查询工具集
//!
//! 提供 get_balance / get_positions / get_position / get_trade_history / get_fee_rate
//! / get_account_summary 等 MCP Tool，通过 MainEngine 的 OmsEngine 缓存读取账户数据。

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
pub struct GetPositionParams {
    /// 标的符号（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub symbol: String,
    /// 方向：Long / Short / Net
    #[serde(default)]
    pub direction: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct AccountIdParams {
    /// 账户 ID（vt_accountid 格式，如 binance.SPOT）
    pub account_id: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct GetTradeHistoryParams {
    /// 按标的符号过滤（可选）
    #[serde(default)]
    pub symbol: Option<String>,
    /// 最大返回条数（默认 100）
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

// ---- AccountTools (data holder, no longer has #[tool_router]) ----

/// 账户与持仓查询数据容器（已迁移到 TradingMcpServer 的 #[tool_router] impl）
#[allow(dead_code)]
pub struct AccountTools {
    engine: Arc<MainEngine>,
}

#[allow(dead_code)]
impl AccountTools {
    /// 创建 AccountTools 实例
    pub fn new(engine: Arc<MainEngine>) -> Self {
        Self { engine }
    }
}

// ---- TradingMcpServer tool router for account tools ----

#[tool_router(router = account_router, vis = "pub")]
impl TradingMcpServer {
    #[tool(description = "Get account balance for all connected accounts")]
    fn get_balance(&self) -> Result<CallToolResult, McpError> {
        let accounts = self.engine.get_all_accounts();
        if accounts.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No account data available. Make sure you are connected to a gateway.".to_string(),
            )]));
        }
        let summary: Vec<serde_json::Value> = accounts
            .iter()
            .map(|a| {
                serde_json::json!({
                    "vt_accountid": a.vt_accountid(),
                    "balance": a.balance,
                    "frozen": a.frozen,
                    "available": a.available(),
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|_| format!("{} accounts", summary.len()));
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Get all positions across all symbols")]
    fn get_positions(&self) -> Result<CallToolResult, McpError> {
        let positions = self.engine.get_all_positions();
        if positions.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No positions found.".to_string(),
            )]));
        }
        let summary: Vec<serde_json::Value> = positions
            .iter()
            .filter(|p| p.volume > 0.0)
            .map(|p| {
                serde_json::json!({
                    "vt_positionid": p.vt_positionid(),
                    "symbol": p.vt_symbol(),
                    "direction": format!("{}", p.direction),
                    "volume": p.volume,
                    "frozen": p.frozen,
                    "price": p.price,
                    "pnl": p.pnl,
                    "yd_volume": p.yd_volume,
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|_| format!("{} positions", summary.len()));
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Get position for a specific symbol and direction")]
    fn get_position(
        &self,
        Parameters(params): Parameters<GetPositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let positions = self.engine.get_all_positions();
        let filtered: Vec<_> = positions
            .iter()
            .filter(|p| {
                let symbol_match = p.vt_symbol() == params.symbol;
                let direction_match = match &params.direction {
                    Some(d) => format!("{}", p.direction).to_lowercase() == d.to_lowercase(),
                    None => true,
                };
                symbol_match && direction_match
            })
            .collect();

        if filtered.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No position found for symbol: {}",
                params.symbol
            ))]));
        }

        let summary: Vec<serde_json::Value> = filtered
            .iter()
            .map(|p| {
                serde_json::json!({
                    "vt_positionid": p.vt_positionid(),
                    "symbol": p.vt_symbol(),
                    "direction": format!("{}", p.direction),
                    "volume": p.volume,
                    "frozen": p.frozen,
                    "price": p.price,
                    "pnl": p.pnl,
                    "yd_volume": p.yd_volume,
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|_| "Position data".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Get trade history with optional symbol filter")]
    fn get_trade_history(
        &self,
        Parameters(params): Parameters<GetTradeHistoryParams>,
    ) -> Result<CallToolResult, McpError> {
        let trades = self.engine.get_all_trades();
        let filtered: Vec<_> = trades
            .iter()
            .filter(|t| {
                match &params.symbol {
                    Some(s) => t.vt_symbol() == *s,
                    None => true,
                }
            })
            .take(params.limit)
            .collect();

        if filtered.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                "No trade history found.".to_string(),
            )]));
        }

        let summary: Vec<serde_json::Value> = filtered
            .iter()
            .map(|t| {
                serde_json::json!({
                    "vt_tradeid": t.vt_tradeid(),
                    "vt_orderid": t.vt_orderid(),
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

    #[tool(description = "Get fee rate for a symbol (from contract data)")]
    fn get_fee_rate(
        &self,
        Parameters(params): Parameters<SymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_contract(&params.symbol) {
            Some(contract) => {
                let result = serde_json::json!({
                    "symbol": contract.vt_symbol(),
                    "name": contract.name,
                    "product": format!("{}", contract.product),
                    "size": contract.size,
                    "pricetick": contract.pricetick,
                    "min_volume": contract.min_volume,
                    "note": "Fee rate information is configured at the gateway level. Contact your exchange for current fee tiers.",
                });
                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "Contract data".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "No contract data found for symbol: {}",
                params.symbol
            ))])),
        }
    }

    #[tool(description = "Get account summary including balance, positions overview, and PnL")]
    fn get_account_summary(&self) -> Result<CallToolResult, McpError> {
        let accounts = self.engine.get_all_accounts();
        let positions = self.engine.get_all_positions();
        let active_orders = self.engine.get_all_active_orders();

        let total_balance: f64 = accounts.iter().map(|a| a.balance).sum();
        let total_available: f64 = accounts.iter().map(|a| a.available()).sum();
        let total_frozen: f64 = accounts.iter().map(|a| a.frozen).sum();
        let total_pnl: f64 = positions.iter().map(|p| p.pnl).sum();
        let open_positions = positions.iter().filter(|p| p.volume > 0.0).count();

        let result = serde_json::json!({
            "accounts": accounts.len(),
            "total_balance": total_balance,
            "total_available": total_available,
            "total_frozen": total_frozen,
            "total_pnl": total_pnl,
            "open_positions": open_positions,
            "active_orders": active_orders.len(),
        });
        let text = serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| "Account summary".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
