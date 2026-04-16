//! MCP Server 主服务器实现
//!
//! TradingMcpServer 是 MCP 协议的核心入口，实现 ServerHandler trait，
//! 聚合 TradingTools + UITools + Resources，支持 stdio 模式启动。

use rmcp::{
    ErrorData as McpError,
    model::*,
    RoleServer, ServerHandler, ServiceExt,
    handler::server::router::tool::ToolRouter,
    service::RequestContext,
    tool_handler, tool_router,
};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

use crate::trader::MainEngine;
use super::types::{UICommandReceiver, UICommandSender, UIState};

/// 交易系统 MCP Server
///
/// 聚合后端交易工具和前端 UI 工具，通过 MCP 协议暴露给 LLM 客户端。
/// 同时提供 Resources 接口查询实时交易数据和 UI 状态。
#[allow(dead_code)]
pub struct TradingMcpServer {
    engine: Arc<MainEngine>,
    ui_sender: UICommandSender,
    ui_state: Arc<RwLock<UIState>>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl TradingMcpServer {
    /// 创建 TradingMcpServer 实例
    ///
    /// 返回 (server, UICommandReceiver)，Receiver 供 UI 线程消费命令。
    pub fn new(engine: Arc<MainEngine>) -> (Self, UICommandReceiver) {
        let (tx, rx) = mpsc::unbounded_channel();
        let ui_state = Arc::new(RwLock::new(UIState::default()));

        let server = Self {
            engine,
            ui_sender: tx,
            ui_state: ui_state.clone(),
            tool_router: Self::tool_router(),
        };

        (server, rx)
    }

    /// 获取 UI 状态的共享引用（供外部更新）
    pub fn ui_state(&self) -> Arc<RwLock<UIState>> {
        self.ui_state.clone()
    }

    /// 以 stdio 模式启动（适用于 Claude Desktop 等 MCP 客户端）
    pub async fn serve_stdio(self) -> Result<(), String> {
        let service = self
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| format!("MCP server serve error: {:?}", e))?;
        service.waiting().await.map_err(|e| format!("MCP server waiting error: {:?}", e))?;
        Ok(())
    }
}

#[tool_handler]
impl ServerHandler for TradingMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new(
            "vnrs-trading",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Trading system MCP server. Use trading.* tools for backend operations (connect, subscribe, send_order, cancel_order, query_history, list_contracts) and ui.* tools for frontend control (switch_symbol, switch_interval, add_indicator, remove_indicator, clear_indicators, navigate_to, show_notification). Resources: trading://ticks, trading://orders, trading://active_orders, trading://positions, trading://accounts, trading://trades, trading://contracts, ui://current_symbol, ui://chart_indicators, ui://active_tab".to_string(),
        )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resources = super::resources::list_resources(&self.ui_state);
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        super::resources::read_resource(request.uri.as_str(), &self.engine, &self.ui_state)
    }
}
