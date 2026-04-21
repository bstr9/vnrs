//! MCP Server 主服务器实现
//!
//! TradingMcpServer 是 MCP 协议的核心入口，实现 ServerHandler trait，
//! 聚合 TradingTools + UITools + Resources，支持 stdio 和 HTTP/SSE 两种传输模式。
//! 支持 Sampling 能力：Server 可通过 `Peer::create_message()` 向 Client 请求 LLM 推理。

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
use crate::strategy::engine::StrategyEngine;
use super::types::{McpConfig, McpTransport, SamplingConfig, UICommandReceiver, UICommandSender, UIState};
#[allow(unused_imports)]
use super::tools::{TradingTools, UITools, MarketTools, AccountTools, StrategyTools, RiskTools, BacktestTools};

/// 交易系统 MCP Server
///
/// 聚合后端交易工具和前端 UI 工具，通过 MCP 协议暴露给 LLM 客户端。
/// 同时提供 Resources 接口查询实时交易数据和 UI 状态。
/// 支持 Sampling：工具可通过 `request_sampling()` 请求客户端 LLM 推理。
///
/// 支持两种传输模式：
/// - **STDIO**：适用于 Claude Desktop 等本地 MCP 客户端
/// - **HTTP/SSE**：适用于远程 Web 客户端，基于 rmcp 的 StreamableHttpService
#[allow(dead_code)]
pub struct TradingMcpServer {
    pub(crate) engine: Arc<MainEngine>,
    pub(crate) ui_sender: UICommandSender,
    pub(crate) ui_state: Arc<RwLock<UIState>>,
    pub(crate) sampling_config: SamplingConfig,
    tool_router: ToolRouter<Self>,
    /// Optional StrategyEngine for strategy tools
    pub(crate) strategy_engine: Option<Arc<StrategyEngine>>,
}

#[tool_router]
impl TradingMcpServer {
    /// 创建 TradingMcpServer 实例
    ///
    /// 返回 (server, UICommandReceiver)，Receiver 供 UI 线程消费命令。
    pub fn new(engine: Arc<MainEngine>) -> (Self, UICommandReceiver) {
        Self::with_sampling_config(engine, SamplingConfig::default())
    }

    /// 创建带自定义 Sampling 配置的 TradingMcpServer 实例
    pub fn with_sampling_config(
        engine: Arc<MainEngine>,
        sampling_config: SamplingConfig,
    ) -> (Self, UICommandReceiver) {
        let (tx, rx) = mpsc::unbounded_channel();
        let ui_state = Arc::new(RwLock::new(UIState::default()));

        let server = Self {
            engine,
            ui_sender: tx,
            ui_state: ui_state.clone(),
            sampling_config,
            tool_router: Self::tool_router(),
            strategy_engine: None,
        };

        (server, rx)
    }

    /// 获取 UI 状态的共享引用（供外部更新）
    pub fn ui_state(&self) -> Arc<RwLock<UIState>> {
        self.ui_state.clone()
    }

    /// 获取 Sampling 配置的引用
    pub fn sampling_config(&self) -> &SamplingConfig {
        &self.sampling_config
    }

    /// 向 Client 发起 LLM Sampling 请求
    pub async fn request_sampling(
        peer: &rmcp::Peer<RoleServer>,
        messages: Vec<SamplingMessage>,
        system_prompt: Option<String>,
        config: &SamplingConfig,
        tool_name: &str,
    ) -> Result<CreateMessageResult, McpError> {
        tracing::info!(
            tool_name = tool_name,
            message_count = messages.len(),
            max_tokens = config.max_tokens,
            temperature = config.temperature,
            system_prompt = system_prompt.as_deref().unwrap_or("<none>"),
            "MCP Sampling request initiated"
        );

        let mut params = CreateMessageRequestParams::new(messages, config.max_tokens)
            .with_temperature(config.temperature);

        if let Some(sp) = system_prompt {
            params = params.with_system_prompt(sp);
        }

        if let Some(ref model_pref) = config.model_preference {
            params = params.with_model_preferences(
                ModelPreferences::default().with_hints(vec![ModelHint::new(model_pref.clone())]),
            );
        }

        match peer.create_message(params).await {
            Ok(result) => {
                tracing::info!(
                    tool_name = tool_name,
                    model = %result.model,
                    stop_reason = result.stop_reason.as_deref().unwrap_or("<none>"),
                    "MCP Sampling request completed"
                );
                Ok(result)
            }
            Err(e) => {
                tracing::error!(
                    tool_name = tool_name,
                    error = %e,
                    "MCP Sampling request failed"
                );
                Err(McpError::internal_error(
                    format!("Sampling request failed: {}", e),
                    None,
                ))
            }
        }
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

    /// 以 HTTP/SSE 模式启动（适用于远程 Web 客户端）
    ///
    /// 使用 rmcp 的 `StreamableHttpService` + `LocalSessionManager`，
    /// 通过 axum 提供 HTTP 服务，MCP 端点挂载在 `/mcp` 路径下。
    ///
    /// # 参数
    /// - `addr`: 监听的 Socket 地址（如 `127.0.0.1:3000`）
    ///
    /// # 示例
    /// ```rust,no_run
    /// use std::net::SocketAddr;
    /// use std::sync::Arc;
    /// use trade_engine::trader::MainEngine;
    /// use trade_engine::mcp::TradingMcpServer;
    ///
    /// let engine = MainEngine::new();
    /// let (server, _) = TradingMcpServer::new(Arc::new(engine));
    /// let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
    /// tokio::spawn(async move {
    ///     server.serve_http(addr).await.unwrap();
    /// });
    /// ```
    pub async fn serve_http(self, addr: std::net::SocketAddr) -> Result<(), String> {
        use rmcp::transport::streamable_http_server::{
            StreamableHttpServerConfig,
            session::local::LocalSessionManager,
        };

        let engine = self.engine.clone();
        let ui_state = self.ui_state.clone();
        let sampling_config = self.sampling_config.clone();
        let strategy_engine = self.strategy_engine.clone();

        let service = rmcp::transport::StreamableHttpService::new(
            move || {
                let (tx, _rx) = mpsc::unbounded_channel();
                Ok(Self {
                    engine: engine.clone(),
                    ui_sender: tx,
                    ui_state: ui_state.clone(),
                    sampling_config: sampling_config.clone(),
                    strategy_engine: strategy_engine.clone(),
                    tool_router: Self::tool_router(),
                })
            },
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default(),
        );

        let app = axum::Router::new().nest_service("/mcp", service);

        tracing::info!("MCP HTTP/SSE server listening on http://{}", addr);
        tracing::info!("   MCP endpoint: http://{}/mcp", addr);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| format!("Failed to bind MCP HTTP server to {}: {}", addr, e))?;

        axum::serve(listener, app)
            .await
            .map_err(|e| format!("MCP HTTP server error: {}", e))?;

        Ok(())
    }

    /// 根据配置自动选择传输模式启动
    pub async fn serve_with_config(self, config: &McpConfig) -> Result<(), String> {
        match &config.transport {
            McpTransport::Stdio => self.serve_stdio().await,
            McpTransport::Http { port, host } => {
                let host_str = host.as_deref().unwrap_or("127.0.0.1");
                let addr: std::net::SocketAddr = format!("{}:{}", host_str, port)
                    .parse()
                    .map_err(|e| format!("Invalid MCP HTTP address: {}", e))?;
                self.serve_http(addr).await
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for TradingMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new(
            "vnrs-trading",
            env!("CARGO_PKG_VERSION"),
        ))
        .with_instructions(
            "Trading system MCP server with Sampling support. Use trading.* tools for backend operations (connect, subscribe, send_order, cancel_order, query_history, list_contracts, analyze_sentiment) and ui.* tools for frontend control (switch_symbol, switch_interval, add_indicator, remove_indicator, clear_indicators, navigate_to, show_notification). The analyze_sentiment tool uses MCP Sampling to request LLM-powered sentiment analysis. Resources: trading://ticks, trading://orders, trading://active_orders, trading://positions, trading://accounts, trading://trades, trading://contracts, ui://current_symbol, ui://chart_indicators, ui://active_tab. Prompts: pre_trade_check, risk_assessment, position_analysis, market_overview, strategy_review".to_string(),
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

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts = super::prompts::list_prompts();
        Ok(ListPromptsResult {
            prompts,
            next_cursor: None,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        super::prompts::get_prompt(request.name.as_str(), request.arguments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_transport_parse_stdio() {
        let transport = McpTransport::parse("stdio");
        assert_eq!(transport, McpTransport::Stdio);
    }

    #[test]
    fn test_mcp_transport_parse_http_default_port() {
        let transport = McpTransport::parse("http");
        assert!(matches!(transport, McpTransport::Http { port: 3000, host: None }));
    }

    #[test]
    fn test_mcp_transport_parse_http_custom_port() {
        let transport = McpTransport::parse("http:8080");
        assert!(matches!(transport, McpTransport::Http { port: 8080, host: None }));
    }

    #[test]
    fn test_mcp_transport_parse_http_with_host() {
        let transport = McpTransport::parse("http:0.0.0.0:9090");
        match transport {
            McpTransport::Http { port, host } => {
                assert_eq!(port, 9090);
                assert_eq!(host.as_deref(), Some("0.0.0.0"));
            }
            _ => panic!("Expected Http transport"),
        }
    }

    #[test]
    fn test_mcp_transport_default_is_stdio() {
        assert_eq!(McpTransport::default(), McpTransport::Stdio);
    }

    #[test]
    fn test_mcp_config_from_env_no_panic() {
        let config = McpConfig::from_env();
        assert!(matches!(config.transport, McpTransport::Stdio | McpTransport::Http { .. }));
    }

    #[test]
    fn test_mcp_config_http_convenience() {
        let config = McpConfig::http(8080);
        match config.transport {
            McpTransport::Http { port, host } => {
                assert_eq!(port, 8080);
                assert!(host.is_none());
            }
            _ => panic!("Expected Http transport"),
        }
    }

    #[test]
    fn test_mcp_config_stdio_convenience() {
        let config = McpConfig::stdio();
        assert_eq!(config.transport, McpTransport::Stdio);
    }

    #[test]
    fn test_mcp_transport_convenience_constructors() {
        let stdio = McpTransport::stdio();
        assert_eq!(stdio, McpTransport::Stdio);

        let http = McpTransport::http(3000);
        assert!(matches!(http, McpTransport::Http { port: 3000, host: None }));

        let http_with_host = McpTransport::http_with_host(9090, "0.0.0.0");
        match http_with_host {
            McpTransport::Http { port, host } => {
                assert_eq!(port, 9090);
                assert_eq!(host.as_deref(), Some("0.0.0.0"));
            }
            _ => panic!("Expected Http transport"),
        }
    }

    #[test]
    fn test_create_message_result_validation() {
        let result = CreateMessageResult::new(
            SamplingMessage::assistant_text("neutral sentiment"),
            "test-model".to_string(),
        );
        assert!(result.validate().is_ok());
        assert_eq!(result.model, "test-model");
    }

    #[test]
    fn test_create_message_result_wrong_role_validation() {
        let result = CreateMessageResult::new(
            SamplingMessage::user_text("this should fail"),
            "test-model".to_string(),
        );
        assert!(result.validate().is_err());
    }

    #[test]
    fn test_sampling_message_construction() {
        let msg = SamplingMessage::user_text("BTC is going up!");
        assert_eq!(msg.role, Role::User);

        let msg = SamplingMessage::assistant_text("Bullish sentiment detected");
        assert_eq!(msg.role, Role::Assistant);
    }
}
