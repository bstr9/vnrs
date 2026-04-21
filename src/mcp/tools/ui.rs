//! MCP UI Tools — 前端 UI 操作工具集
//!
//! 提供 switch_symbol / switch_interval / add_indicator / remove_indicator / clear_indicators
//! 等 MCP Tool，通过 UICommand 通道驱动 UI 线程执行界面操作。

use rmcp::{
    ErrorData as McpError,
    model::*,
    tool, tool_router,
    handler::server::wrapper::Parameters,
    schemars,
};
use serde::Deserialize;
use std::sync::{Arc, RwLock};

use super::super::types::{UICommand, UICommandSender, UIState};
use super::super::server::TradingMcpServer;

// ---- 参数结构体 ----

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SwitchSymbolParams {
    /// 交易标的符号（如 BTCUSDT.BINANCE）
    pub symbol: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SwitchIntervalParams {
    /// K 线周期（如 1m / 5m / 15m / 1h / 4h / 1d）
    pub interval: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct AddIndicatorParams {
    /// 指标类型（如 MA / EMA / RSI / MACD / BOLL / KDJ / ATR / CCI / WR / DMA）
    pub indicator_type: String,
    /// 指标周期（可选，默认使用标准值）
    #[serde(default)]
    pub period: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct RemoveIndicatorParams {
    /// 要移除的指标索引（从 0 开始）
    pub index: usize,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct NavigateToParams {
    /// 目标标签页名称（如 market / trade / position / account / log）
    pub tab: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ShowNotificationParams {
    /// 通知内容
    pub message: String,
    /// 通知级别：info / warning / error
    #[serde(default = "default_level")]
    pub level: String,
}

fn default_level() -> String {
    "info".to_string()
}

// ---- UITools (data holder, no longer has #[tool_router]) ----

/// 前端 UI 操作数据容器（已迁移到 TradingMcpServer 的 #[tool_router] impl）
#[allow(dead_code)]
pub struct UITools {
    ui_sender: UICommandSender,
    ui_state: Arc<RwLock<UIState>>,
}

#[allow(dead_code)]
impl UITools {
    /// 创建 UITools 实例
    pub fn new(ui_sender: UICommandSender, ui_state: Arc<RwLock<UIState>>) -> Self {
        Self {
            ui_sender,
            ui_state,
        }
    }
}

// ---- TradingMcpServer tool router for UI tools ----

#[tool_router(router = ui_router, vis = "pub")]
impl TradingMcpServer {
    #[tool(description = "Switch to a different trading symbol on the chart")]
    async fn switch_symbol(
        &self,
        Parameters(params): Parameters<SwitchSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        self.ui_sender
            .send(UICommand::SwitchSymbol {
                symbol: params.symbol.clone(),
            })
            .map_err(|e| McpError::internal_error(format!("Failed to send UI command: {}", e), None))?;

        // 更新共享状态
        if let Ok(mut state) = self.ui_state.write() {
            state.current_symbol = Some(params.symbol.clone());
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Switched to symbol: {}",
            params.symbol
        ))]))
    }

    #[tool(description = "Switch chart interval (e.g., 1m, 5m, 1h, 1d)")]
    async fn switch_interval(
        &self,
        Parameters(params): Parameters<SwitchIntervalParams>,
    ) -> Result<CallToolResult, McpError> {
        self.ui_sender
            .send(UICommand::SwitchInterval {
                interval: params.interval.clone(),
            })
            .map_err(|e| McpError::internal_error(format!("Failed to send UI command: {}", e), None))?;

        if let Ok(mut state) = self.ui_state.write() {
            state.current_interval = Some(params.interval.clone());
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Switched to interval: {}",
            params.interval
        ))]))
    }

    #[tool(description = "Add a technical indicator to the chart")]
    async fn add_indicator(
        &self,
        Parameters(params): Parameters<AddIndicatorParams>,
    ) -> Result<CallToolResult, McpError> {
        self.ui_sender
            .send(UICommand::AddIndicator {
                indicator_type: params.indicator_type.clone(),
                period: params.period,
            })
            .map_err(|e| McpError::internal_error(format!("Failed to send UI command: {}", e), None))?;

        if let Ok(mut state) = self.ui_state.write() {
            let label = match params.period {
                Some(p) => format!("{}({})", params.indicator_type, p),
                None => params.indicator_type.clone(),
            };
            state.chart_indicators.push(label.clone());
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Added indicator: {}",
            params.indicator_type
        ))]))
    }

    #[tool(description = "Remove an indicator by index")]
    async fn remove_indicator(
        &self,
        Parameters(params): Parameters<RemoveIndicatorParams>,
    ) -> Result<CallToolResult, McpError> {
        self.ui_sender
            .send(UICommand::RemoveIndicator { index: params.index })
            .map_err(|e| McpError::internal_error(format!("Failed to send UI command: {}", e), None))?;

        let removed_name = if let Ok(mut state) = self.ui_state.write() {
            if params.index < state.chart_indicators.len() {
                state.chart_indicators.remove(params.index)
            } else {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Index {} out of range ({} indicators)",
                    params.index,
                    state.chart_indicators.len()
                ))]));
            }
        } else {
            "unknown".to_string()
        };

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Removed indicator: {} (index {})",
            removed_name, params.index
        ))]))
    }

    #[tool(description = "Clear all indicators from the chart")]
    async fn clear_indicators(&self) -> Result<CallToolResult, McpError> {
        self.ui_sender
            .send(UICommand::ClearIndicators)
            .map_err(|e| McpError::internal_error(format!("Failed to send UI command: {}", e), None))?;

        if let Ok(mut state) = self.ui_state.write() {
            state.chart_indicators.clear();
        }

        Ok(CallToolResult::success(vec![Content::text(
            "All indicators cleared".to_string(),
        )]))
    }

    #[tool(description = "Navigate to a specific tab in the UI")]
    async fn navigate_to(
        &self,
        Parameters(params): Parameters<NavigateToParams>,
    ) -> Result<CallToolResult, McpError> {
        self.ui_sender
            .send(UICommand::NavigateTo {
                tab: params.tab.clone(),
            })
            .map_err(|e| McpError::internal_error(format!("Failed to send UI command: {}", e), None))?;

        if let Ok(mut state) = self.ui_state.write() {
            state.active_tab = params.tab.clone();
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Navigated to tab: {}",
            params.tab
        ))]))
    }

    #[tool(description = "Show a notification message in the UI")]
    async fn show_notification(
        &self,
        Parameters(params): Parameters<ShowNotificationParams>,
    ) -> Result<CallToolResult, McpError> {
        self.ui_sender
            .send(UICommand::ShowNotification {
                message: params.message.clone(),
                level: params.level.clone(),
            })
            .map_err(|e| McpError::internal_error(format!("Failed to send UI command: {}", e), None))?;

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Notification shown: [{}] {}",
            params.level, params.message
        ))]))
    }
}
