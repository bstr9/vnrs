//! MCP Trading Tools — 后端交易操作工具集
//!
//! 提供 connect / subscribe / send_order / cancel_order / query_history / list_contracts
//! / analyze_sentiment 等 MCP Tool，通过 MainEngine 执行实际交易操作。
//! analyze_sentiment 使用 MCP Sampling 向 Client 请求 LLM 推理。

use rmcp::{
    ErrorData as McpError,
    model::*,
    tool, tool_router,
    handler::server::wrapper::Parameters,
    schemars,
    RoleServer,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::trader::{
    CancelRequest, Direction, Exchange, HistoryRequest, OrderRequest, OrderType, Offset,
    SubscribeRequest, MainEngine,
};
use super::super::types::{SamplingConfig, UICommandSender};
use super::super::server::TradingMcpServer;

// ---- 参数结构体（每个 tool 独立，派生 JsonSchema） ----

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ConnectParams {
    /// 网关名称，如 "binance_spot"、"binance_usdt"
    pub gateway_name: String,
    /// API Key
    pub api_key: String,
    /// API Secret
    pub api_secret: String,
    /// 是否使用测试网
    #[serde(default)]
    pub testnet: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SubscribeParams {
    /// 标的符号，如 "BTCUSDT"
    pub symbol: String,
    /// 交易所名称，如 "BINANCE"
    pub exchange: String,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SendOrderParams {
    /// 标的符号
    pub symbol: String,
    /// 交易所名称
    pub exchange: String,
    /// 方向：Long / Short / Net
    pub direction: String,
    /// 订单类型：Limit / Market / Stop / Fak / Fok
    pub order_type: String,
    /// 数量
    pub volume: f64,
    /// 价格（限价单必填）
    #[serde(default)]
    pub price: Option<f64>,
    /// 开平标志：None / Open / Close / CloseToday / CloseYesterday
    #[serde(default)]
    pub offset: Option<String>,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct CancelOrderParams {
    /// 订单 ID
    pub order_id: String,
    /// 标的符号
    pub symbol: String,
    /// 交易所名称
    pub exchange: String,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct QueryHistoryParams {
    /// 标的符号
    pub symbol: String,
    /// 交易所名称
    pub exchange: String,
    /// K 线周期：1m / 5m / 15m / 1h / 4h / 1d / 1w
    pub interval: String,
    /// 起始时间（ISO 8601）
    pub start: String,
    /// 结束时间（ISO 8601），可选
    #[serde(default)]
    pub end: Option<String>,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeSentimentParams {
    /// 待分析的文本内容（新闻、社交媒体、公告等）
    pub text: String,
    /// 分析上下文（可选，如相关标的符号）
    #[serde(default)]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SuggestStrategyParamsParams {
    /// Strategy identifier (e.g., strategy name or class name)
    pub strategy_id: String,
    /// Current strategy parameters as a JSON string
    pub current_params: String,
    /// Summary of recent strategy performance (e.g., PnL, win rate, drawdown)
    pub performance_summary: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct DisconnectParams {
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct UnsubscribeParams {
    /// 标的符号，如 "BTCUSDT"
    pub symbol: String,
    /// 交易所名称，如 "BINANCE"
    pub exchange: String,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ModifyOrderParams {
    /// 原 订单 ID
    pub order_id: String,
    /// 标的符号
    pub symbol: String,
    /// 交易所名称
    pub exchange: String,
    /// 新价格
    pub price: f64,
    /// 新数量
    pub volume: f64,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct BatchOrdersParams {
    /// 订单列表（JSON 数组，每个元素包含 symbol, exchange, direction, order_type, volume, price, offset, gateway_name）
    pub orders: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct ClosePositionParams {
    /// 标的符号（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub symbol: String,
    /// 方向：Long / Short / Net
    pub direction: String,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SetLeverageParams {
    /// 标的符号
    pub symbol: String,
    /// 交易所名称
    pub exchange: String,
    /// 杠杆倍数
    pub leverage: u32,
    /// 网关名称
    pub gateway_name: String,
}

#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct GetOrderStatusParams {
    /// 订单 ID（vt_orderid 格式，如 binance_spot.12345）
    pub order_id: String,
}

// ---- 辅助函数 ----

/// 将字符串解析为 Exchange 枚举
#[allow(dead_code)]
fn parse_exchange(s: &str) -> Result<Exchange, McpError> {
    match s.to_uppercase().as_str() {
        "BINANCE" => Ok(Exchange::Binance),
        "BINANCE_USDM" => Ok(Exchange::BinanceUsdm),
        "BINANCE_COINM" => Ok(Exchange::BinanceCoinm),
        "OKX" => Ok(Exchange::Okx),
        "BYBIT" => Ok(Exchange::Bybit),
        "LOCAL" => Ok(Exchange::Local),
        _ => Err(McpError::invalid_params(
            format!("Unknown exchange: {}", s),
            None,
        )),
    }
}

/// 将字符串解析为 Direction 枚举
#[allow(dead_code)]
fn parse_direction(s: &str) -> Result<Direction, McpError> {
    match s.to_lowercase().as_str() {
        "long" | "buy" => Ok(Direction::Long),
        "short" | "sell" => Ok(Direction::Short),
        "net" => Ok(Direction::Net),
        _ => Err(McpError::invalid_params(
            format!("Unknown direction: {}", s),
            None,
        )),
    }
}

/// 将字符串解析为 OrderType 枚举
#[allow(dead_code)]
fn parse_order_type(s: &str) -> Result<OrderType, McpError> {
    match s.to_lowercase().as_str() {
        "limit" => Ok(OrderType::Limit),
        "market" => Ok(OrderType::Market),
        "stop" => Ok(OrderType::Stop),
        "stop_limit" | "stoplimit" => Ok(OrderType::StopLimit),
        "fak" => Ok(OrderType::Fak),
        "fok" => Ok(OrderType::Fok),
        _ => Err(McpError::invalid_params(
            format!("Unknown order_type: {}", s),
            None,
        )),
    }
}

/// 将字符串解析为 Offset 枚举
#[allow(dead_code)]
fn parse_offset(s: &str) -> Result<Offset, McpError> {
    match s.to_lowercase().as_str() {
        "none" | "" => Ok(Offset::None),
        "open" => Ok(Offset::Open),
        "close" => Ok(Offset::Close),
        "closetoday" => Ok(Offset::CloseToday),
        "closeyesterday" => Ok(Offset::CloseYesterday),
        _ => Err(McpError::invalid_params(
            format!("Unknown offset: {}", s),
            None,
        )),
    }
}

/// 将字符串解析为 Interval 枚举
#[allow(dead_code)]
fn parse_interval(s: &str) -> Result<crate::trader::Interval, McpError> {
    use crate::trader::Interval;
    match s {
        "1s" => Ok(Interval::Second),
        "1m" => Ok(Interval::Minute),
        "15m" => Ok(Interval::Minute15),
        "1h" => Ok(Interval::Hour),
        "4h" => Ok(Interval::Hour4),
        "1d" | "d" => Ok(Interval::Daily),
        "1w" | "w" => Ok(Interval::Weekly),
        _ => Err(McpError::invalid_params(
            format!("Unknown interval: {}", s),
            None,
        )),
    }
}

// ---- TradingTools (data holder, no longer has #[tool_router]) ----

/// 后端交易操作数据容器（已迁移到 TradingMcpServer 的 #[tool_router] impl）
#[allow(dead_code)]
pub struct TradingTools {
    engine: Arc<MainEngine>,
    ui_sender: UICommandSender,
}

#[allow(dead_code)]
impl TradingTools {
    /// 创建 TradingTools 实例
    pub fn new(engine: Arc<MainEngine>, ui_sender: UICommandSender) -> Self {
        Self { engine, ui_sender }
    }
}

// ---- TradingMcpServer tool router for trading tools ----

#[tool_router(router = trading_router, vis = "pub")]
impl TradingMcpServer {
    #[tool(description = "Connect to an exchange gateway")]
    async fn connect(
        &self,
        Parameters(params): Parameters<ConnectParams>,
    ) -> Result<CallToolResult, McpError> {
        use crate::trader::gateway::GatewaySettingValue;
        use std::collections::HashMap;

        let mut settings: HashMap<String, GatewaySettingValue> = HashMap::new();
        settings.insert("api_key".to_string(), GatewaySettingValue::String(params.api_key));
        settings.insert("api_secret".to_string(), GatewaySettingValue::String(params.api_secret));
        if let Some(testnet) = params.testnet {
            settings.insert("testnet".to_string(), GatewaySettingValue::Bool(testnet));
        }

        match self.engine.connect(settings, &params.gateway_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Successfully connected to gateway: {}",
                params.gateway_name
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to connect to gateway {}: {}",
                params.gateway_name, e
            ))])),
        }
    }

    #[tool(description = "Subscribe to market data for a symbol")]
    async fn subscribe(
        &self,
        Parameters(params): Parameters<SubscribeParams>,
    ) -> Result<CallToolResult, McpError> {
        let exchange = parse_exchange(&params.exchange)?;
        let req = SubscribeRequest::new(params.symbol.clone(), exchange);

        match self.engine.subscribe(req, &params.gateway_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Subscribed to {}.{} via {}",
                params.symbol, params.exchange, params.gateway_name
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to subscribe: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Send a trade order")]
    async fn send_order(
        &self,
        Parameters(params): Parameters<SendOrderParams>,
    ) -> Result<CallToolResult, McpError> {
        let exchange = parse_exchange(&params.exchange)?;
        let direction = parse_direction(&params.direction)?;
        let order_type = parse_order_type(&params.order_type)?;
        let offset = match &params.offset {
            Some(s) => parse_offset(s)?,
            None => Offset::None,
        };

        let mut req = OrderRequest::new(
            params.symbol.clone(),
            exchange,
            direction,
            order_type,
            params.volume,
        );
        req.price = params.price.unwrap_or(0.0);
        req.offset = offset;

        match self.engine.send_order(req, &params.gateway_name).await {
            Ok(vt_orderid) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Order sent: {}",
                vt_orderid
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to send order: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Cancel an existing order")]
    async fn cancel_order(
        &self,
        Parameters(params): Parameters<CancelOrderParams>,
    ) -> Result<CallToolResult, McpError> {
        let exchange = parse_exchange(&params.exchange)?;
        let req = CancelRequest::new(params.order_id.clone(), params.symbol.clone(), exchange);

        match self.engine.cancel_order(req, &params.gateway_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Order {} cancelled",
                params.order_id
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to cancel order: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Query historical K-line data")]
    async fn query_history(
        &self,
        Parameters(params): Parameters<QueryHistoryParams>,
    ) -> Result<CallToolResult, McpError> {
        let exchange = parse_exchange(&params.exchange)?;
        let interval = parse_interval(&params.interval)?;

        let start = chrono::DateTime::parse_from_rfc3339(&params.start)
            .map(|dt| dt.to_utc())
            .map_err(|e| McpError::invalid_params(format!("Invalid start time: {}", e), None))?;

        let end = match &params.end {
            Some(s) => Some(
                chrono::DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.to_utc())
                    .map_err(|e| McpError::invalid_params(format!("Invalid end time: {}", e), None))?,
            ),
            None => None,
        };

        let req = HistoryRequest {
            symbol: params.symbol.clone(),
            exchange,
            start,
            end,
            interval: Some(interval),
        };

        match self.engine.query_history(req, &params.gateway_name).await {
            Ok(bars) => {
                let count = bars.len();
                let summary = serde_json::to_string_pretty(&bars)
                    .unwrap_or_else(|_| format!("{} bars retrieved", count));
                Ok(CallToolResult::success(vec![Content::text(summary)]))
            }
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to query history: {}",
                e
            ))])),
        }
    }

    #[tool(description = "List all available contracts")]
    fn list_contracts(&self) -> Result<CallToolResult, McpError> {
        let contracts = self.engine.get_all_contracts();
        let summary: Vec<serde_json::Value> = contracts
            .iter()
            .map(|c| {
                serde_json::json!({
                    "vt_symbol": c.vt_symbol(),
                    "name": c.name,
                    "product": format!("{}", c.product),
                    "size": c.size,
                    "pricetick": c.pricetick,
                })
            })
            .collect();
        let text = serde_json::to_string_pretty(&summary)
            .unwrap_or_else(|_| format!("{} contracts", summary.len()));
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Analyze sentiment of text using LLM via MCP Sampling. Useful for analyzing news, social media, or announcements for trading signals.")]
    async fn analyze_sentiment(
        &self,
        Parameters(params): Parameters<AnalyzeSentimentParams>,
        peer: rmcp::Peer<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let config = SamplingConfig::default();

        // 构造系统提示
        let system_prompt = if let Some(ref ctx) = params.context {
            format!(
                "You are a financial sentiment analyst. Analyze the following text and provide: \
                 1) Overall sentiment (bullish/bearish/neutral) \
                 2) Confidence level (0-100%) \
                 3) Key factors influencing the sentiment \
                 4) Potential trading implications. \
                 Context: {}",
                ctx
            )
        } else {
            "You are a financial sentiment analyst. Analyze the following text and provide: \
             1) Overall sentiment (bullish/bearish/neutral) \
             2) Confidence level (0-100%) \
             3) Key factors influencing the sentiment \
             4) Potential trading implications."
                .to_string()
        };

        // 构造消息
        let messages = vec![SamplingMessage::user_text(&params.text)];

        // 审计日志
        tracing::info!(
            tool_name = "analyze_sentiment",
            message_count = 1,
            max_tokens = config.max_tokens,
            temperature = config.temperature,
            text_preview = &params.text[..params.text.len().min(80)],
            "MCP Sampling request: analyze_sentiment"
        );

        // 构造 Sampling 请求参数
        let mut request_params = CreateMessageRequestParams::new(messages, config.max_tokens)
            .with_temperature(config.temperature)
            .with_system_prompt(&system_prompt);

        if let Some(ref model_pref) = config.model_preference {
            let hints = vec![ModelHint::new(model_pref.clone())];
            request_params = request_params.with_model_preferences(
                ModelPreferences::default().with_hints(hints),
            );
        }

        // 发起 Sampling 请求
        match peer.create_message(request_params).await {
            Ok(result) => {
                tracing::info!(
                    tool_name = "analyze_sentiment",
                    model = %result.model,
                    stop_reason = result.stop_reason.as_deref().unwrap_or("<none>"),
                    "MCP Sampling completed: analyze_sentiment"
                );

                // 提取文本响应
                let response_text = result
                    .message
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");

                if response_text.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text(
                        "Sentiment analysis returned no text content.".to_string(),
                    )]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Sentiment Analysis Result (model: {}):\n\n{}",
                        result.model, response_text
                    ))]))
                }
            }
            Err(e) => {
                tracing::error!(
                    tool_name = "analyze_sentiment",
                    error = %e,
                    "MCP Sampling failed: analyze_sentiment"
                );
                // 降级：如果 Sampling 不可用，返回模拟结果
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Sentiment analysis (simulated - LLM sampling unavailable): \
                     Text preview: \"{}...\" | \
                     Estimated sentiment: neutral | \
                     Confidence: 50% | \
                     Note: Connect to an MCP client with sampling support for real LLM analysis. Error: {}",
                    &params.text[..params.text.len().min(60)],
                    e
                ))]))
            }
        }
    }

    #[tool(description = "Suggest optimized strategy parameters using LLM via MCP Sampling. Provides AI-driven parameter tuning recommendations based on current parameters and performance data.")]
    async fn suggest_strategy_params(
        &self,
        Parameters(params): Parameters<SuggestStrategyParamsParams>,
        peer: rmcp::Peer<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let config = SamplingConfig::default();

        // Construct system prompt
        let system_prompt = "You are a quantitative trading strategy optimizer. \
             Given a strategy's current parameters and recent performance summary, \
             suggest optimized parameter values. For each parameter you suggest changing, \
             provide: \
             1) The parameter name \
             2) The current value \
             3) The suggested value \
             4) The rationale for the change \
             Also provide an overall confidence level (0-100%) for your suggestions.";

        // Construct the user message with strategy details
        let user_message = format!(
            "Strategy ID: {}\n\nCurrent Parameters:\n{}\n\nPerformance Summary:\n{}",
            params.strategy_id, params.current_params, params.performance_summary
        );

        // Audit log
        tracing::info!(
            tool_name = "suggest_strategy_params",
            strategy_id = %params.strategy_id,
            max_tokens = config.max_tokens,
            temperature = config.temperature,
            "MCP Sampling request: suggest_strategy_params"
        );

        // Build sampling request parameters
        let messages = vec![SamplingMessage::user_text(&user_message)];

        let mut request_params = CreateMessageRequestParams::new(messages, config.max_tokens)
            .with_temperature(config.temperature)
            .with_system_prompt(system_prompt);

        if let Some(ref model_pref) = config.model_preference {
            let hints = vec![ModelHint::new(model_pref.clone())];
            request_params = request_params.with_model_preferences(
                ModelPreferences::default().with_hints(hints),
            );
        }

        // Issue sampling request
        match peer.create_message(request_params).await {
            Ok(result) => {
                tracing::info!(
                    tool_name = "suggest_strategy_params",
                    model = %result.model,
                    stop_reason = result.stop_reason.as_deref().unwrap_or("<none>"),
                    "MCP Sampling completed: suggest_strategy_params"
                );

                // Extract text response
                let response_text = result
                    .message
                    .content
                    .iter()
                    .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");

                if response_text.is_empty() {
                    Ok(CallToolResult::success(vec![Content::text(
                        "Strategy parameter suggestion returned no text content.".to_string(),
                    )]))
                } else {
                    Ok(CallToolResult::success(vec![Content::text(format!(
                        "Strategy Parameter Suggestions for {} (model: {}):\n\n{}",
                        params.strategy_id, result.model, response_text
                    ))]))
                }
            }
            Err(e) => {
                tracing::error!(
                    tool_name = "suggest_strategy_params",
                    error = %e,
                    "MCP Sampling failed: suggest_strategy_params"
                );
                // Fallback: if sampling is unavailable, return a placeholder
                Ok(CallToolResult::success(vec![Content::text(format!(
                    "Strategy parameter suggestions (simulated - LLM sampling unavailable): \
                     Strategy: {} | \
                     Note: Connect to an MCP client with sampling support for real LLM-driven parameter optimization. Error: {}",
                    params.strategy_id, e
                ))]))
            }
        }
    }

    #[tool(description = "Disconnect from an exchange gateway")]
    async fn disconnect(
        &self,
        Parameters(params): Parameters<DisconnectParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.disconnect(&params.gateway_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Disconnected from gateway: {}",
                params.gateway_name
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to disconnect from gateway {}: {}",
                params.gateway_name, e
            ))])),
        }
    }

    #[tool(description = "Unsubscribe from market data for a symbol")]
    async fn unsubscribe(
        &self,
        Parameters(params): Parameters<UnsubscribeParams>,
    ) -> Result<CallToolResult, McpError> {
        let exchange = parse_exchange(&params.exchange)?;
        let req = SubscribeRequest::new(params.symbol.clone(), exchange);

        match self.engine.unsubscribe(req, &params.gateway_name).await {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Unsubscribed from {}.{} via {}",
                params.symbol, params.exchange, params.gateway_name
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to unsubscribe: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Modify an existing order (cancel + resend with new price/volume)")]
    async fn modify_order(
        &self,
        Parameters(params): Parameters<ModifyOrderParams>,
    ) -> Result<CallToolResult, McpError> {
        let exchange = parse_exchange(&params.exchange)?;

        // Cancel the original order first
        let cancel_req = CancelRequest::new(params.order_id.clone(), params.symbol.clone(), exchange);
        if let Err(e) = self.engine.cancel_order(cancel_req, &params.gateway_name).await {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to cancel original order {}: {}",
                params.order_id, e
            ))]));
        }

        // Find the original order to get direction and type
        let original = self.engine.get_order(&format!("{}.{}", params.gateway_name, params.order_id));
        let (direction, order_type, offset) = match original {
            Some(o) => {
                let dir = o.direction.unwrap_or(Direction::Long);
                (dir, o.order_type, o.offset)
            }
            None => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Original order {} cancelled, but cannot find original details to resend. Please place a new order manually.",
                    params.order_id
                ))]));
            }
        };

        // Place a new order with modified parameters
        let req = OrderRequest {
            symbol: params.symbol.clone(),
            exchange,
            direction,
            order_type,
            volume: params.volume,
            price: params.price,
            offset,
            reference: format!("modify_{}", params.order_id),
            post_only: false,
            reduce_only: false,
            expire_time: None,
            gateway_name: String::new(),
        };

        match self.engine.send_order(req, &params.gateway_name).await {
            Ok(vt_orderid) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Order modified: cancelled {}, new order {}",
                params.order_id, vt_orderid
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Original order {} cancelled, but failed to place new order: {}",
                params.order_id, e
            ))])),
        }
    }

    #[tool(description = "Send multiple orders in batch (JSON array of order params)")]
    async fn batch_orders(
        &self,
        Parameters(params): Parameters<BatchOrdersParams>,
    ) -> Result<CallToolResult, McpError> {
        let orders: Vec<serde_json::Value> = serde_json::from_str(&params.orders)
            .map_err(|e| McpError::invalid_params(format!("Invalid orders JSON: {}", e), None))?;

        let mut results = Vec::new();
        for order_json in orders {
            let symbol = order_json["symbol"].as_str().unwrap_or("").to_string();
            let exchange_str = order_json["exchange"].as_str().unwrap_or("").to_string();
            let direction_str = order_json["direction"].as_str().unwrap_or("long").to_string();
            let order_type_str = order_json["order_type"].as_str().unwrap_or("limit").to_string();
            let volume = order_json["volume"].as_f64().unwrap_or(0.0);
            let price = order_json["price"].as_f64().unwrap_or(0.0);
            let offset_str = order_json["offset"].as_str().map(|s| s.to_string());
            let gateway_name = order_json["gateway_name"].as_str().unwrap_or("").to_string();

            let exchange = match parse_exchange(&exchange_str) {
                Ok(e) => e,
                Err(e) => {
                    results.push(serde_json::json!({
                        "symbol": symbol,
                        "status": "error",
                        "error": format!("{}", e),
                    }));
                    continue;
                }
            };
            let direction = match parse_direction(&direction_str) {
                Ok(d) => d,
                Err(e) => {
                    results.push(serde_json::json!({
                        "symbol": symbol,
                        "status": "error",
                        "error": format!("{}", e),
                    }));
                    continue;
                }
            };
            let order_type = match parse_order_type(&order_type_str) {
                Ok(t) => t,
                Err(e) => {
                    results.push(serde_json::json!({
                        "symbol": symbol,
                        "status": "error",
                        "error": format!("{}", e),
                    }));
                    continue;
                }
            };
            let offset = match &offset_str {
                Some(s) => match parse_offset(s) {
                    Ok(o) => o,
                    Err(e) => {
                        results.push(serde_json::json!({
                            "symbol": symbol,
                            "status": "error",
                            "error": format!("{}", e),
                        }));
                        continue;
                    }
                },
                None => Offset::None,
            };

            let req = OrderRequest {
                symbol: symbol.clone(),
                exchange,
                direction,
                order_type,
                volume,
                price,
                offset,
                reference: "batch_order".to_string(),
                post_only: false,
                reduce_only: false,
                expire_time: None,
                gateway_name: String::new(),
            };

            match self.engine.send_order(req, &gateway_name).await {
                Ok(vt_orderid) => {
                    results.push(serde_json::json!({
                        "symbol": symbol,
                        "status": "sent",
                        "vt_orderid": vt_orderid,
                    }));
                }
                Err(e) => {
                    results.push(serde_json::json!({
                        "symbol": symbol,
                        "status": "error",
                        "error": e,
                    }));
                }
            }
        }

        let text = serde_json::to_string_pretty(&results)
            .unwrap_or_else(|_| format!("{} orders processed", results.len()));
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(description = "Close a position by generating a close order")]
    async fn close_position(
        &self,
        Parameters(params): Parameters<ClosePositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let direction = parse_direction(&params.direction)?;

        // Find the position
        let positions = self.engine.get_all_positions();
        let position = positions.iter().find(|p| {
            p.vt_symbol() == params.symbol && format!("{}", p.direction) == format!("{}", direction)
        });

        let position = match position {
            Some(p) => p,
            None => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "No {} position found for symbol: {}",
                    params.direction, params.symbol
                ))]));
            }
        };

        if position.volume <= 0.0 {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Position for {} {} is already closed (volume: {})",
                params.symbol, params.direction, position.volume
            ))]));
        }

        // Parse symbol to extract symbol and exchange
        let parts: Vec<&str> = params.symbol.split('.').collect();
        if parts.len() != 2 {
            return Ok(CallToolResult::success(vec![Content::text(format!(
                "Invalid symbol format: {}. Expected format: SYMBOL.EXCHANGE",
                params.symbol
            ))]));
        }

        let symbol = parts[0].to_string();
        let exchange = parse_exchange(parts[1])?;

        // Determine close direction and offset
        let (close_direction, close_offset) = match direction {
            Direction::Long => (Direction::Short, Offset::Close),
            Direction::Short => (Direction::Long, Offset::Close),
            Direction::Net => (Direction::Net, Offset::None),
        };

        let req = OrderRequest {
            symbol: symbol.clone(),
            exchange,
            direction: close_direction,
            order_type: OrderType::Market,
            volume: position.volume - position.frozen,
            price: 0.0,
            offset: close_offset,
            reference: "close_position".to_string(),
            post_only: false,
            reduce_only: true,
            expire_time: None,
            gateway_name: String::new(),
        };

        match self.engine.send_order(req, &params.gateway_name).await {
            Ok(vt_orderid) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Close position order sent: {} (closing {} {} @ {} vol={})",
                vt_orderid, params.direction, params.symbol, position.volume - position.frozen, params.gateway_name
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to close position: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Set leverage for a futures symbol (gateway-dependent)")]
    async fn set_leverage(
        &self,
        Parameters(params): Parameters<SetLeverageParams>,
    ) -> Result<CallToolResult, McpError> {
        // Leverage setting is gateway-specific and not all gateways support it
        // We attempt to use the gateway's custom functionality if available
        if let Some(_gateway) = self.engine.get_gateway(&params.gateway_name) {
            // Try to use the gateway's set_leverage method if available
            // Since BaseGateway doesn't have set_leverage, we return a note
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Leverage setting for {} on {}: {}x. Note: Leverage must be configured through the exchange API directly or gateway-specific methods. Current gateway may or may not support this operation.",
                params.symbol, params.gateway_name, params.leverage
            ))]))
        } else {
            Ok(CallToolResult::success(vec![Content::text(format!(
                "Gateway {} not found",
                params.gateway_name
            ))]))
        }
    }

    #[tool(description = "Get the current status of an order by its vt_orderid")]
    fn get_order_status(
        &self,
        Parameters(params): Parameters<GetOrderStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.engine.get_order(&params.order_id) {
            Some(order) => {
                let result = serde_json::json!({
                    "vt_orderid": order.vt_orderid(),
                    "symbol": order.vt_symbol(),
                    "direction": format!("{:?}", order.direction),
                    "order_type": format!("{:?}", order.order_type),
                    "offset": format!("{}", order.offset),
                    "price": order.price,
                    "volume": order.volume,
                    "traded": order.traded,
                    "status": format!("{:?}", order.status),
                    "reference": order.reference,
                    "datetime": order.datetime.map(|d| d.to_rfc3339()).unwrap_or_default(),
                });
                let text = serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| "Order status".to_string());
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            None => Ok(CallToolResult::success(vec![Content::text(format!(
                "Order not found: {}",
                params.order_id
            ))])),
        }
    }
}
