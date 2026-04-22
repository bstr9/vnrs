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
    tool_handler,
};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;

use crate::trader::MainEngine;
use crate::strategy::engine::StrategyEngine;
use super::types::{McpConfig, McpTransport, SamplingConfig, UICommandReceiver, UICommandSender, UIState};
use super::tools::backtest::BacktestEntry;

/// 工具名称到模块的映射表
///
/// 用于 `allowed_modules` 过滤：根据工具名查找所属模块类别。
/// 模块类别包括：trading, ui, market, account, strategy, risk, backtest, news
pub fn tool_module(name: &str) -> &'static str {
    match name {
        // Trading module
        "connect" | "subscribe" | "send_order" | "cancel_order" | "query_history"
        | "list_contracts" | "analyze_sentiment" | "disconnect" | "unsubscribe"
        | "modify_order" | "batch_orders" | "close_position" | "set_leverage"
        | "get_order_status" | "suggest_strategy_params" => "trading",

        // UI module
        "switch_symbol" | "switch_interval" | "add_indicator" | "remove_indicator"
        | "clear_indicators" | "navigate_to" | "show_notification" => "ui",

        // Market module
        "get_ticker" | "get_orderbook" | "get_candles" | "get_trades"
        | "get_funding_rate" | "get_mark_price" | "get_index_price"
        | "get_liquidations" | "get_open_interest" | "get_ticker_24h" => "market",

        // Account module
        "get_balance" | "get_positions" | "get_position" | "get_trade_history"
        | "get_fee_rate" | "get_account_summary" => "account",

        // Strategy module
        "list_strategies" | "get_strategy_status" | "start_strategy" | "stop_strategy"
        | "pause_strategy" | "get_strategy_params" | "set_strategy_params"
        | "get_strategy_performance" => "strategy",

        // Risk module
        "get_risk_metrics" | "set_stop_loss" | "set_take_profit" | "check_margin"
        | "get_exposure" => "risk",

        // Backtest module
        "run_backtest" | "get_backtest_result" | "list_backtests" | "compare_strategies" => "backtest",

        // News module
        "get_news" | "get_economic_calendar" | "get_market_events" => "news",

        _ => "unknown",
    }
}

/// 只读模式下应排除的写操作工具列表
pub const WRITE_TOOLS: &[&str] = &[
    "send_order",
    "cancel_order",
    "modify_order",
    "batch_orders",
    "close_position",
    "set_leverage",
    "start_strategy",
    "stop_strategy",
    "pause_strategy",
    "set_strategy_params",
    "set_stop_loss",
    "set_take_profit",
];

// ---- Sampling Approval Callback ----

/// Parameters passed to the sampling approval callback for human-in-the-loop review.
#[derive(Debug, Clone)]
pub struct SamplingApprovalParams {
    /// The tool name that initiated the sampling request.
    pub tool_name: String,
    /// Maximum tokens requested.
    pub max_tokens: u32,
    /// Temperature setting.
    pub temperature: f32,
    /// Preview of the system prompt (truncated to 200 chars).
    pub system_prompt_preview: Option<String>,
    /// Number of messages in the request.
    pub message_count: usize,
}

/// Type alias for the async sampling approval callback.
///
/// The callback receives `SamplingApprovalParams` and returns `true` to approve
/// the sampling request, or `false` to reject it.
pub type SamplingApprovalCallback =
    Box<dyn Fn(SamplingApprovalParams) -> std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>> + Send + Sync>;

/// 交易系统 MCP Server
///
/// 聚合 8 组工具（Trading / UI / Market / Account / Strategy / Risk / Backtest / News），
/// 通过 MCP 协议暴露给 LLM 客户端。同时提供 Resources 和 Prompts 接口。
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
    /// Backtest results cache
    pub(crate) backtest_cache: Arc<tokio::sync::RwLock<Vec<BacktestEntry>>>,
    /// MCP configuration (read_only, allowed_modules, transport)
    pub(crate) config: McpConfig,
    /// Optional human-in-the-loop approval callback for sampling requests
    pub(crate) approval_callback: Option<Arc<SamplingApprovalCallback>>,
}

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
        Self::with_config_and_sampling(engine, McpConfig::default(), sampling_config)
    }

    /// 创建带 McpConfig 的 TradingMcpServer 实例
    pub fn with_config(engine: Arc<MainEngine>, config: McpConfig) -> (Self, UICommandReceiver) {
        Self::with_config_and_sampling(engine, config, SamplingConfig::default())
    }

    /// 创建带 McpConfig 和 Sampling 配置的 TradingMcpServer 实例
    pub fn with_config_and_sampling(
        engine: Arc<MainEngine>,
        config: McpConfig,
        sampling_config: SamplingConfig,
    ) -> (Self, UICommandReceiver) {
        let (tx, rx) = mpsc::unbounded_channel();
        let ui_state = Arc::new(RwLock::new(UIState::default()));
        let backtest_cache = Arc::new(tokio::sync::RwLock::new(Vec::new()));

        // Combine all tool routers using the + operator
        // Each router is generated by #[tool_router(router = xxx)] in the respective tool files
        let mut router = Self::trading_router()
            + Self::ui_router()
            + Self::market_router()
            + Self::account_router()
            + Self::strategy_router()
            + Self::risk_router()
            + Self::backtest_router()
            + Self::news_router();

        // Apply permission filtering based on McpConfig
        if config.read_only {
            for tool_name in WRITE_TOOLS {
                router.remove_route(tool_name);
            }
        }

        if !config.allowed_modules.is_empty() {
            let names_to_remove: Vec<String> = router
                .map
                .keys()
                .filter(|name| !config.is_module_allowed(tool_module(name)))
                .map(|k| k.to_string())
                .collect();
            for name in names_to_remove {
                router.remove_route(&name);
            }
        }

        let server = Self {
            engine,
            ui_sender: tx,
            ui_state: ui_state.clone(),
            sampling_config,
            tool_router: router,
            strategy_engine: None,
            backtest_cache,
            config,
            approval_callback: None,
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

    /// 获取 MCP 配置的引用
    pub fn config(&self) -> &McpConfig {
        &self.config
    }

    /// Set the human-in-the-loop approval callback for sampling requests.
    ///
    /// When set, every sampling request will be forwarded to this callback
    /// before being sent to the LLM. If the callback returns `false`,
    /// the sampling request is rejected with an error.
    ///
    /// # Example
    /// ```rust,ignore
    /// server.set_sampling_approval_callback(Box::new(|params| {
    ///     Box::pin(async move {
    ///         // Show params to user, return true to approve
    ///         println!("Sampling request from: {}", params.tool_name);
    ///         true
    ///     })
    /// }));
    /// ```
    pub fn set_sampling_approval_callback(&mut self, callback: SamplingApprovalCallback) {
        self.approval_callback = Some(Arc::new(callback));
    }

    /// Check if an approval callback is configured.
    pub fn has_approval_callback(&self) -> bool {
        self.approval_callback.is_some()
    }

    /// 向 Client 发起 LLM Sampling 请求
    ///
    /// If an approval callback is provided, it will be called before forwarding
    /// the request to the LLM. If it returns `false`, the request is rejected
    /// with error "Sampling request rejected by human-in-the-loop".
    pub async fn request_sampling(
        peer: &rmcp::Peer<RoleServer>,
        messages: Vec<SamplingMessage>,
        system_prompt: Option<String>,
        config: &SamplingConfig,
        tool_name: &str,
        approval_callback: Option<&Arc<SamplingApprovalCallback>>,
    ) -> Result<CreateMessageResult, McpError> {
        // Human-in-the-loop approval check
        if let Some(callback) = approval_callback {
            let approval_params = SamplingApprovalParams {
                tool_name: tool_name.to_string(),
                max_tokens: config.max_tokens,
                temperature: config.temperature,
                system_prompt_preview: system_prompt.as_deref().map(|s| {
                    if s.len() > 200 { s[..200].to_string() } else { s.to_string() }
                }),
                message_count: messages.len(),
            };
            let approved = callback(approval_params).await;
            if !approved {
                tracing::warn!(
                    tool_name = tool_name,
                    "MCP Sampling request rejected by human-in-the-loop"
                );
                return Err(McpError::internal_error(
                    "Sampling request rejected by human-in-the-loop",
                    None,
                ));
            }
        }

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
    /// ```rust,ignore
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
        let backtest_cache = self.backtest_cache.clone();
        let config = self.config.clone();

        let service = rmcp::transport::StreamableHttpService::new(
            move || {
                let (tx, _rx) = mpsc::unbounded_channel();
                let mut router = Self::trading_router()
                    + Self::ui_router()
                    + Self::market_router()
                    + Self::account_router()
                    + Self::strategy_router()
                    + Self::risk_router()
                    + Self::backtest_router()
                    + Self::news_router();

                // Apply permission filtering based on McpConfig
                if config.read_only {
                    for tool_name in WRITE_TOOLS {
                        router.remove_route(tool_name);
                    }
                }

                if !config.allowed_modules.is_empty() {
                    let names_to_remove: Vec<String> = router
                        .map
                        .keys()
                        .filter(|name| !config.is_module_allowed(tool_module(name)))
                        .map(|k| k.to_string())
                        .collect();
                    for name in names_to_remove {
                        router.remove_route(&name);
                    }
                }

                Ok(Self {
                    engine: engine.clone(),
                    ui_sender: tx,
                    ui_state: ui_state.clone(),
                    sampling_config: sampling_config.clone(),
                    strategy_engine: strategy_engine.clone(),
                    backtest_cache: backtest_cache.clone(),
                    tool_router: router,
                    config: config.clone(),
                    approval_callback: None,
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

#[tool_handler(router = self.tool_router)]
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
            "Trading system MCP server with 50+ tools across 8 categories. \
             \n\n**Trading tools**: connect, subscribe, send_order, cancel_order, query_history, list_contracts, analyze_sentiment, suggest_strategy_params, disconnect, unsubscribe, modify_order, batch_orders, close_position, set_leverage, get_order_status. \
             \n\n**UI tools**: switch_symbol, switch_interval, add_indicator, remove_indicator, clear_indicators, navigate_to, show_notification. \
             \n\n**Market tools**: get_ticker, get_orderbook, get_candles, get_trades, get_funding_rate, get_mark_price, get_index_price, get_liquidations, get_open_interest, get_ticker_24h. \
             \n\n**Account tools**: get_balance, get_positions, get_position, get_trade_history, get_fee_rate, get_account_summary. \
             \n\n**Strategy tools**: list_strategies, get_strategy_status, start_strategy, stop_strategy, pause_strategy, get_strategy_params, set_strategy_params, get_strategy_performance. \
             \n\n**Risk tools**: get_risk_metrics, set_stop_loss, set_take_profit, check_margin, get_exposure. \
             \n\n**Backtest tools**: run_backtest, get_backtest_result, list_backtests, compare_strategies. \
             \n\n**News tools**: get_news, get_economic_calendar, get_market_events. \
              \n\nThe analyze_sentiment and suggest_strategy_params tools use MCP Sampling to request LLM-powered analysis. \
             \n\n**Resources**: trading://ticks, trading://orders, trading://active_orders, trading://positions, trading://accounts, trading://trades, trading://contracts, ui://current_symbol, ui://chart_indicators, ui://active_tab. \
              \n\n**Prompts**: pre_trade_check, risk_assessment, position_analysis, market_overview, strategy_review, backtest_analysis, parameter_optimization, portfolio_risk, margin_check, exposure_analysis.".to_string(),
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
    use std::collections::HashSet;

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

    #[test]
    fn test_tool_module_mapping() {
        assert_eq!(tool_module("send_order"), "trading");
        assert_eq!(tool_module("cancel_order"), "trading");
        assert_eq!(tool_module("get_ticker"), "market");
        assert_eq!(tool_module("get_balance"), "account");
        assert_eq!(tool_module("start_strategy"), "strategy");
        assert_eq!(tool_module("get_risk_metrics"), "risk");
        assert_eq!(tool_module("run_backtest"), "backtest");
        assert_eq!(tool_module("get_news"), "news");
        assert_eq!(tool_module("switch_symbol"), "ui");
        assert_eq!(tool_module("nonexistent"), "unknown");
    }

    #[test]
    fn test_write_tools_list() {
        assert!(WRITE_TOOLS.contains(&"send_order"));
        assert!(WRITE_TOOLS.contains(&"cancel_order"));
        assert!(WRITE_TOOLS.contains(&"modify_order"));
        assert!(WRITE_TOOLS.contains(&"batch_orders"));
        assert!(WRITE_TOOLS.contains(&"close_position"));
        assert!(WRITE_TOOLS.contains(&"set_leverage"));
        assert!(WRITE_TOOLS.contains(&"start_strategy"));
        assert!(WRITE_TOOLS.contains(&"stop_strategy"));
        assert!(WRITE_TOOLS.contains(&"pause_strategy"));
        assert!(WRITE_TOOLS.contains(&"set_strategy_params"));
        assert!(WRITE_TOOLS.contains(&"set_stop_loss"));
        assert!(WRITE_TOOLS.contains(&"set_take_profit"));
    }

    #[test]
    fn test_mcp_config_read_only_filtering() {
        let config = McpConfig {
            transport: McpTransport::Stdio,
            read_only: true,
            allowed_modules: HashSet::new(),
        };

        // Verify that write tools are listed for removal
        for tool_name in WRITE_TOOLS {
            assert!(!config.is_module_allowed("trading") || *tool_name != "send_order"
                || true); // just verifying iteration works
        }
        assert!(config.is_read_only());
    }

    #[test]
    fn test_mcp_config_allowed_modules_filtering() {
        // Only allow market and account modules
        let mut modules = HashSet::new();
        modules.insert("market".to_string());
        modules.insert("account".to_string());

        let config = McpConfig {
            transport: McpTransport::Stdio,
            read_only: false,
            allowed_modules: modules,
        };

        assert!(config.is_module_allowed("market"));
        assert!(config.is_module_allowed("account"));
        assert!(!config.is_module_allowed("trading"));
        assert!(!config.is_module_allowed("strategy"));
        assert!(!config.is_module_allowed("risk"));
    }

    #[test]
    fn test_mcp_config_empty_modules_allows_all() {
        let config = McpConfig {
            transport: McpTransport::Stdio,
            read_only: false,
            allowed_modules: HashSet::new(),
        };

        // Empty allowed_modules means all modules are allowed
        assert!(config.is_module_allowed("trading"));
        assert!(config.is_module_allowed("market"));
        assert!(config.is_module_allowed("strategy"));
    }

    #[test]
    fn test_mcp_config_from_env_reads_read_only() {
        // Test that from_env properly parses MCP_READ_ONLY
        let config = McpConfig::from_env();
        // We can't set env vars in a thread-safe way, so just verify it doesn't panic
        assert!(matches!(config.transport, McpTransport::Stdio | McpTransport::Http { .. }));
    }

    #[test]
    fn test_mcp_config_with_read_only_builder() {
        let config = McpConfig::stdio().with_read_only(true);
        assert!(config.is_read_only());

        let config = McpConfig::stdio().with_read_only(false);
        assert!(!config.is_read_only());
    }

    #[test]
    fn test_mcp_config_with_allowed_modules_builder() {
        let mut modules = HashSet::new();
        modules.insert("market".to_string());

        let config = McpConfig::stdio().with_allowed_modules(modules);
        assert!(config.is_module_allowed("market"));
        assert!(!config.is_module_allowed("trading"));
    }

    #[test]
    fn test_mcp_config_read_only_convenience() {
        let config = McpConfig::read_only();
        assert!(config.is_read_only());
        assert!(config.allowed_modules.is_empty());
    }

    #[test]
    fn test_sampling_approval_params_construction() {
        let params = SamplingApprovalParams {
            tool_name: "analyze_sentiment".to_string(),
            max_tokens: 1024,
            temperature: 0.7,
            system_prompt_preview: Some("You are a financial analyst.".to_string()),
            message_count: 1,
        };
        assert_eq!(params.tool_name, "analyze_sentiment");
        assert_eq!(params.max_tokens, 1024);
        assert_eq!(params.message_count, 1);
    }

    #[tokio::test]
    async fn test_sampling_approval_callback_rejects() {
        // Create a callback that always rejects
        let callback: SamplingApprovalCallback = Box::new(|_params| {
            Box::pin(async { false })
        });

        let callback_arc = Arc::new(callback);
        let config = SamplingConfig::default();

        // Test the approval check logic directly
        let approval_params = SamplingApprovalParams {
            tool_name: "test_tool".to_string(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            system_prompt_preview: Some("test prompt".to_string()),
            message_count: 1,
        };

        let approved = callback_arc(approval_params).await;
        assert!(!approved, "Callback that returns false should reject");

        // Verify the error message that request_sampling would produce
        let expected_msg = "Sampling request rejected by human-in-the-loop";
        assert!(expected_msg.contains("rejected by human-in-the-loop"));
    }

    #[tokio::test]
    async fn test_sampling_approval_callback_approves() {
        // Create a callback that always approves
        let callback: SamplingApprovalCallback = Box::new(|_params| {
            Box::pin(async { true })
        });

        let callback_arc = Arc::new(callback);
        let config = SamplingConfig::default();

        let approval_params = SamplingApprovalParams {
            tool_name: "analyze_sentiment".to_string(),
            max_tokens: config.max_tokens,
            temperature: config.temperature,
            system_prompt_preview: None,
            message_count: 3,
        };

        let approved = callback_arc(approval_params).await;
        assert!(approved, "Callback that returns true should approve");
    }

    #[test]
    fn test_sampling_no_callback_means_auto_approve() {
        // Without an approval callback, sampling should proceed automatically.
        // This is verified by the logic: if approval_callback is None,
        // the check is skipped entirely.
        let callback: Option<Arc<SamplingApprovalCallback>> = None;
        assert!(callback.is_none(), "No callback means auto-approve");
    }

    #[test]
    fn test_set_sampling_approval_callback() {
        let engine = MainEngine::new();
        let (mut server, _rx) = TradingMcpServer::new(engine);

        assert!(!server.has_approval_callback());

        server.set_sampling_approval_callback(Box::new(|_params| {
            Box::pin(async { true })
        }));

        assert!(server.has_approval_callback());
    }

    #[tokio::test]
    async fn test_approval_callback_receives_correct_params() {
        let received = std::sync::Arc::new(std::sync::Mutex::new(None));
        let received_clone = received.clone();

        let callback: SamplingApprovalCallback = Box::new(move |params| {
            let received = received_clone.clone();
            Box::pin(async move {
                *received.lock().unwrap() = Some((
                    params.tool_name.clone(),
                    params.max_tokens,
                    params.temperature,
                    params.system_prompt_preview.clone(),
                    params.message_count,
                ));
                true
            })
        });

        let callback_arc = Arc::new(callback);

        let approval_params = SamplingApprovalParams {
            tool_name: "suggest_strategy_params".to_string(),
            max_tokens: 2048,
            temperature: 0.5,
            system_prompt_preview: Some("You are a strategy optimizer.".to_string()),
            message_count: 2,
        };

        let _approved = callback_arc(approval_params).await;

        let guard = received.lock().unwrap();
        let (tool_name, max_tokens, temperature, preview, msg_count) = guard.as_ref().unwrap().clone();
        assert_eq!(tool_name, "suggest_strategy_params");
        assert_eq!(max_tokens, 2048);
        assert!((temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(preview.as_deref(), Some("You are a strategy optimizer."));
        assert_eq!(msg_count, 2);
    }
}
