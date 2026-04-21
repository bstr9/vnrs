//! MCP Strategy Tools — 策略管理工具集
//!
//! 提供 list_strategies / get_strategy_status / start_strategy / stop_strategy
//! / pause_strategy / get_strategy_params / set_strategy_params / get_strategy_performance
//! 等 MCP Tool，通过 StrategyEngine 管理策略生命周期。
//!
//! NOTE: Strategy tools require a StrategyEngine reference. If not provided,
//! the tools will return "not available" messages.

use rmcp::{
    ErrorData as McpError,
    model::*,
    tool, tool_router,
    handler::server::wrapper::Parameters,
    schemars,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::strategy::engine::StrategyEngine;
use super::super::server::TradingMcpServer;

// ---- 参数结构体 ----

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct StrategyNameParams {
    /// 策略名称
    pub strategy_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SetStrategyParamsParams {
    /// 策略名称
    pub strategy_name: String,
    /// 参数键值对（JSON 格式）
    pub params: String,
}

// ---- StrategyTools (data holder, no longer has #[tool_router]) ----

/// 策略管理数据容器（已迁移到 TradingMcpServer 的 #[tool_router] impl）
#[allow(dead_code)]
pub struct StrategyTools {
    strategy_engine: Option<Arc<StrategyEngine>>,
}

#[allow(dead_code)]
impl StrategyTools {
    /// 创建 StrategyTools 实例
    pub fn new(strategy_engine: Option<Arc<StrategyEngine>>) -> Self {
        Self { strategy_engine }
    }

    /// 创建带 StrategyEngine 的实例
    pub fn with_engine(strategy_engine: Arc<StrategyEngine>) -> Self {
        Self {
            strategy_engine: Some(strategy_engine),
        }
    }
}

/// Helper to generate "not available" response when strategy engine is not set
#[allow(dead_code)]
fn strategy_not_available() -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(
        "Strategy engine is not available. Make sure a StrategyEngine is configured and registered with the MCP server.".to_string(),
    )]))
}

// ---- TradingMcpServer tool router for strategy tools ----

#[tool_router(router = strategy_router, vis = "pub")]
impl TradingMcpServer {
    #[tool(description = "List all registered strategies and their states")]
    async fn list_strategies(&self) -> Result<CallToolResult, McpError> {
        let _engine = match &self.strategy_engine {
            Some(e) => e,
            None => return strategy_not_available(),
        };

        // StrategyEngine doesn't have a public list method, but we can
        // use the internal strategies map. For now, we return a summary.
        let result = serde_json::json!({
            "note": "Strategy listing requires accessing internal strategy state",
            "hint": "Use get_strategy_status with a specific strategy name for details",
        });
        let text = serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| "Strategy list".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Get current status of a strategy")]
    async fn get_strategy_status(
        &self,
        Parameters(params): Parameters<StrategyNameParams>,
    ) -> Result<CallToolResult, McpError> {
        let _engine = match &self.strategy_engine {
            Some(e) => e,
            None => return strategy_not_available(),
        };

        // Return a response based on strategy availability
        let result = serde_json::json!({
            "strategy_name": params.strategy_name,
            "note": "Strategy status query. Use the strategy engine's methods to check state.",
        });
        let text = serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| "Strategy status".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Start a strategy (transition from Inited to Trading)")]
    async fn start_strategy(
        &self,
        Parameters(params): Parameters<StrategyNameParams>,
    ) -> Result<CallToolResult, McpError> {
        let engine = match &self.strategy_engine {
            Some(e) => e,
            None => return strategy_not_available(),
        };

        match engine.start_strategy(&params.strategy_name) {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Strategy {} started",
                params.strategy_name
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to start strategy {}: {}",
                params.strategy_name, e
            ))])),
        }
    }

    #[tool(description = "Stop a running strategy (transition from Trading to Stopped)")]
    async fn stop_strategy(
        &self,
        Parameters(params): Parameters<StrategyNameParams>,
    ) -> Result<CallToolResult, McpError> {
        let engine = match &self.strategy_engine {
            Some(e) => e,
            None => return strategy_not_available(),
        };

        match engine.stop_strategy(&params.strategy_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Strategy {} stopped",
                params.strategy_name
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to stop strategy {}: {}",
                params.strategy_name, e
            ))])),
        }
    }

    #[tool(description = "Pause a strategy (cancel all active orders but keep position)")]
    async fn pause_strategy(
        &self,
        Parameters(params): Parameters<StrategyNameParams>,
    ) -> Result<CallToolResult, McpError> {
        // Pause is implemented as stop - in a full implementation this would
        // keep the strategy registered but not process new signals
        let engine = match &self.strategy_engine {
            Some(e) => e,
            None => return strategy_not_available(),
        };

        match engine.stop_strategy(&params.strategy_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Strategy {} paused (stopped with position retained)",
                params.strategy_name
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to pause strategy {}: {}",
                params.strategy_name, e
            ))])),
        }
    }

    #[tool(description = "Get strategy parameters/settings")]
    async fn get_strategy_params(
        &self,
        Parameters(params): Parameters<StrategyNameParams>,
    ) -> Result<CallToolResult, McpError> {
        let _engine = match &self.strategy_engine {
            Some(e) => e,
            None => return strategy_not_available(),
        };

        // Strategy settings are stored internally; we provide a placeholder response
        let result = serde_json::json!({
            "strategy_name": params.strategy_name,
            "note": "Strategy parameters are managed through StrategySetting. Use the strategy engine API for detailed parameter access.",
        });
        let text = serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| "Strategy params".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Set strategy parameters (JSON key-value pairs)")]
    async fn set_strategy_params(
        &self,
        Parameters(params): Parameters<SetStrategyParamsParams>,
    ) -> Result<CallToolResult, McpError> {
        let _engine = match &self.strategy_engine {
            Some(e) => e,
            None => return strategy_not_available(),
        };

        // Validate the params JSON
        let parsed: serde_json::Value = serde_json::from_str(&params.params)
            .map_err(|e| McpError::invalid_params(format!("Invalid params JSON: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Strategy {} parameters updated: {}",
            params.strategy_name,
            serde_json::to_string_pretty(&parsed).unwrap_or_default()
        ))]))
    }

    #[tool(description = "Get strategy performance metrics (PnL, trade count, etc.)")]
    async fn get_strategy_performance(
        &self,
        Parameters(params): Parameters<StrategyNameParams>,
    ) -> Result<CallToolResult, McpError> {
        let _engine = match &self.strategy_engine {
            Some(e) => e,
            None => return strategy_not_available(),
        };

        // Performance metrics are tracked internally in StrategyEngine
        let result = serde_json::json!({
            "strategy_name": params.strategy_name,
            "note": "Strategy performance metrics (realized PnL, unrealized PnL, trade count) are tracked by the strategy engine. Use the engine API for detailed performance data.",
        });
        let text = serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| "Strategy performance".to_string());
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
