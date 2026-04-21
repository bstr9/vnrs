//! MCP Risk Tools — 风险管理工具集
//!
//! 提供 get_risk_metrics / set_stop_loss / set_take_profit / check_margin
//! / get_exposure 等 MCP Tool，通过 MainEngine 的风险管理模块计算风险指标。

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
pub struct SetStopLossParams {
    /// 标的符号（vt_symbol 格式）
    pub symbol: String,
    /// 方向：Long / Short
    pub direction: String,
    /// 止损价格
    pub stop_price: f64,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SetTakeProfitParams {
    /// 标的符号（vt_symbol 格式）
    pub symbol: String,
    /// 方向：Long / Short
    pub direction: String,
    /// 止盈价格
    pub take_profit_price: f64,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct CheckMarginParams {
    /// 标的符号（vt_symbol 格式）
    pub symbol: String,
    /// 拟开仓方向：Long / Short
    pub direction: String,
    /// 拟开仓数量
    pub volume: f64,
    /// 拟开仓价格
    pub price: f64,
}

// ---- RiskTools (data holder, no longer has #[tool_router]) ----

/// 风险管理数据容器（已迁移到 TradingMcpServer 的 #[tool_router] impl）
#[allow(dead_code)]
pub struct RiskTools {
    engine: Arc<MainEngine>,
}

#[allow(dead_code)]
impl RiskTools {
    /// 创建 RiskTools 实例
    pub fn new(engine: Arc<MainEngine>) -> Self {
        Self { engine }
    }
}

// ---- TradingMcpServer tool router for risk tools ----

#[tool_router(router = risk_router, vis = "pub")]
impl TradingMcpServer {
    #[tool(description = "Get portfolio risk metrics including exposure, PnL, and margin usage")]
    fn get_risk_metrics(&self) -> Result<CallToolResult, McpError> {
        let accounts = self.engine.get_all_accounts();
        let positions = self.engine.get_all_positions();
        let active_orders = self.engine.get_all_active_orders();

        let total_balance: f64 = accounts.iter().map(|a| a.balance).sum();
        let total_available: f64 = accounts.iter().map(|a| a.available()).sum();
        let total_frozen: f64 = accounts.iter().map(|a| a.frozen).sum();
        let total_pnl: f64 = positions.iter().map(|p| p.pnl).sum();
        let margin_usage_pct = if total_balance > 0.0 {
            (total_frozen / total_balance) * 100.0
        } else {
            0.0
        };

        // Calculate exposure
        let long_exposure: f64 = positions
            .iter()
            .filter(|p| p.volume > 0.0 && matches!(p.direction, crate::trader::Direction::Long))
            .map(|p| p.price * p.volume)
            .sum();
        let short_exposure: f64 = positions
            .iter()
            .filter(|p| p.volume > 0.0 && matches!(p.direction, crate::trader::Direction::Short))
            .map(|p| p.price * p.volume)
            .sum();
        let net_exposure = long_exposure - short_exposure;

        let result = serde_json::json!({
            "total_balance": total_balance,
            "total_available": total_available,
            "total_frozen": total_frozen,
            "margin_usage_pct": format!("{:.2}%", margin_usage_pct),
            "total_pnl": total_pnl,
            "long_exposure": long_exposure,
            "short_exposure": short_exposure,
            "net_exposure": net_exposure,
            "open_positions": positions.iter().filter(|p| p.volume > 0.0).count(),
            "active_orders": active_orders.len(),
        });
        let text = serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| "Risk metrics".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Set a stop-loss order for a position")]
    async fn set_stop_loss(
        &self,
        Parameters(params): Parameters<SetStopLossParams>,
    ) -> Result<CallToolResult, McpError> {
        let stop_engine = self.engine.stop_order_engine();

        // Parse vt_symbol to get symbol and exchange
        let parts: Vec<&str> = params.symbol.split('.').collect();
        if parts.len() != 2 {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Invalid symbol format: {}. Expected format: SYMBOL.EXCHANGE",
                params.symbol
            ))]));
        }

        let symbol = parts[0].to_string();
        let exchange = match parts[1].to_uppercase().as_str() {
            "BINANCE" => crate::trader::Exchange::Binance,
            "BINANCE_USDM" => crate::trader::Exchange::BinanceUsdm,
            "BINANCE_COINM" => crate::trader::Exchange::BinanceCoinm,
            "OKX" => crate::trader::Exchange::Okx,
            "BYBIT" => crate::trader::Exchange::Bybit,
            "LOCAL" => crate::trader::Exchange::Local,
            _ => crate::trader::Exchange::Local,
        };

        let direction = match params.direction.to_lowercase().as_str() {
            "long" | "buy" => crate::trader::Direction::Long,
            "short" | "sell" => crate::trader::Direction::Short,
            "net" => crate::trader::Direction::Net,
            _ => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Invalid direction: {}. Use Long, Short, or Net",
                    params.direction
                ))]));
            }
        };

        use crate::trader::stop_order::StopOrderRequest;
        
        // Get position to determine volume
        let _pos_key = format!("{}.{}", symbol, params.direction.to_lowercase());
        let positions = self.engine.get_all_positions();
        let position = positions.iter().find(|p| 
            p.vt_symbol() == params.symbol && 
            format!("{}", p.direction).to_lowercase() == params.direction.to_lowercase()
        );
        
        let volume = position.map(|p| p.volume).unwrap_or(0.0);
        if volume <= 0.0 {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No active {} position found for {} to set stop-loss.",
                params.direction, params.symbol
            ))]));
        }

        // For stop-loss, we need to close the opposite direction
        let stop_direction = match direction {
            crate::trader::Direction::Long => crate::trader::Direction::Short,
            crate::trader::Direction::Short => crate::trader::Direction::Long,
            crate::trader::Direction::Net => crate::trader::Direction::Net,
        };
        
        let req = StopOrderRequest::stop_market(
            &symbol, exchange, stop_direction,
            params.stop_price, volume, &params.gateway_name,
        );

        match stop_engine.add_stop_order(req) {
            Ok(order_id) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Stop-loss set for {} {}: stop_price={}, volume={}, order_id={}",
                params.direction, params.symbol, params.stop_price, volume, order_id
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to set stop-loss for {} {}: {}",
                params.direction, params.symbol, e
            ))])),
        }
    }

    #[tool(description = "Set a take-profit order for a position")]
    async fn set_take_profit(
        &self,
        Parameters(params): Parameters<SetTakeProfitParams>,
    ) -> Result<CallToolResult, McpError> {
        // Take-profit uses limit orders (StopLimit in the stop order engine)
        let stop_engine = self.engine.stop_order_engine();

        let parts: Vec<&str> = params.symbol.split('.').collect();
        if parts.len() != 2 {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Invalid symbol format: {}. Expected format: SYMBOL.EXCHANGE",
                params.symbol
            ))]));
        }

        let symbol = parts[0].to_string();
        let exchange = match parts[1].to_uppercase().as_str() {
            "BINANCE" => crate::trader::Exchange::Binance,
            "BINANCE_USDM" => crate::trader::Exchange::BinanceUsdm,
            "BINANCE_COINM" => crate::trader::Exchange::BinanceCoinm,
            "OKX" => crate::trader::Exchange::Okx,
            "BYBIT" => crate::trader::Exchange::Bybit,
            "LOCAL" => crate::trader::Exchange::Local,
            _ => crate::trader::Exchange::Local,
        };

        let direction = match params.direction.to_lowercase().as_str() {
            "long" | "buy" => crate::trader::Direction::Long,
            "short" | "sell" => crate::trader::Direction::Short,
            "net" => crate::trader::Direction::Net,
            _ => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Invalid direction: {}. Use Long, Short, or Net",
                    params.direction
                ))]));
            }
        };

        use crate::trader::stop_order::StopOrderRequest;

        // Get position to determine volume
        let positions = self.engine.get_all_positions();
        let position = positions.iter().find(|p| 
            p.vt_symbol() == params.symbol && 
            format!("{}", p.direction).to_lowercase() == params.direction.to_lowercase()
        );
        
        let volume = position.map(|p| p.volume).unwrap_or(0.0);
        if volume <= 0.0 {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "No active {} position found for {} to set take-profit.",
                params.direction, params.symbol
            ))]));
        }

        // For take-profit, close the opposite direction
        let tp_direction = match direction {
            crate::trader::Direction::Long => crate::trader::Direction::Short,
            crate::trader::Direction::Short => crate::trader::Direction::Long,
            crate::trader::Direction::Net => crate::trader::Direction::Net,
        };

        let req = StopOrderRequest::take_profit(
            &symbol, exchange, tp_direction,
            params.take_profit_price, volume, &params.gateway_name,
        );

        match stop_engine.add_stop_order(req) {
            Ok(order_id) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Take-profit set for {} {}: take_profit_price={}, volume={}, order_id={}",
                params.direction, params.symbol, params.take_profit_price, volume, order_id
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to set take-profit for {} {}: {}",
                params.direction, params.symbol, e
            ))])),
        }
    }

    #[tool(description = "Check margin adequacy for a potential new position")]
    fn check_margin(
        &self,
        Parameters(params): Parameters<CheckMarginParams>,
    ) -> Result<CallToolResult, McpError> {
        let accounts = self.engine.get_all_accounts();
        let total_available: f64 = accounts.iter().map(|a| a.available()).sum();

        // Estimate margin requirement (simplified: notional value * typical margin rate)
        let notional_value = params.price * params.volume;
        let typical_margin_rate = 0.1; // 10% typical margin rate for futures
        let estimated_margin = notional_value * typical_margin_rate;

        let margin_adequate = total_available >= estimated_margin;
        let result = serde_json::json!({
            "symbol": params.symbol,
            "direction": params.direction,
            "volume": params.volume,
            "price": params.price,
            "notional_value": notional_value,
            "estimated_margin": estimated_margin,
            "available_balance": total_available,
            "margin_adequate": margin_adequate,
            "margin_rate_used": format!("{:.0}%", typical_margin_rate * 100.0),
            "note": "This is an estimate using a typical margin rate. Actual margin requirements vary by exchange and instrument.",
        });
        let text = serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| "Margin check".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Get net exposure across all positions")]
    fn get_exposure(&self) -> Result<CallToolResult, McpError> {
        let positions = self.engine.get_all_positions();
        let active_positions: Vec<_> = positions.iter().filter(|p| p.volume > 0.0).collect();

        let mut long_exposure: f64 = 0.0;
        let mut short_exposure: f64 = 0.0;
        let mut exposure_by_symbol: Vec<serde_json::Value> = Vec::new();

        for pos in &active_positions {
            let exposure = pos.price * pos.volume;
            match pos.direction {
                crate::trader::Direction::Long => long_exposure += exposure,
                crate::trader::Direction::Short => short_exposure += exposure,
                crate::trader::Direction::Net => {} // Net doesn't contribute to directional exposure
            }
            exposure_by_symbol.push(serde_json::json!({
                "symbol": pos.vt_symbol(),
                "direction": format!("{}", pos.direction),
                "volume": pos.volume,
                "price": pos.price,
                "exposure": exposure,
                "pnl": pos.pnl,
            }));
        }

        let result = serde_json::json!({
            "total_long_exposure": long_exposure,
            "total_short_exposure": short_exposure,
            "net_exposure": long_exposure - short_exposure,
            "gross_exposure": long_exposure + short_exposure,
            "long_short_ratio": if short_exposure > 0.0 { long_exposure / short_exposure } else { f64::INFINITY },
            "positions": exposure_by_symbol,
        });
        let text = serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| "Exposure data".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
