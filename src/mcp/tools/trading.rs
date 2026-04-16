//! MCP Trading Tools — 后端交易操作工具集
//!
//! 提供 connect / subscribe / send_order / cancel_order / query_history / list_contracts
//! 等 MCP Tool，通过 MainEngine 执行实际交易操作。

use rmcp::{
    ErrorData as McpError,
    model::*,
    tool, tool_router,
    handler::server::wrapper::Parameters,
    schemars,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::trader::{
    CancelRequest, Direction, Exchange, HistoryRequest, OrderRequest, OrderType, Offset,
    SubscribeRequest, MainEngine,
};
use super::super::types::UICommandSender;

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

// ---- 辅助函数 ----

/// 将字符串解析为 Exchange 枚举
#[allow(dead_code)]
fn parse_exchange(s: &str) -> Result<Exchange, McpError> {
    match s.to_uppercase().as_str() {
        "BINANCE" => Ok(Exchange::Binance),
        "BINANCE_USDM" => Ok(Exchange::BinanceUsdm),
        "BINANCE_COINM" => Ok(Exchange::BinanceCoinm),
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

// ---- TradingTools ----

/// 后端交易操作 MCP Tool 集合
#[allow(dead_code)]
pub struct TradingTools {
    engine: Arc<MainEngine>,
    ui_sender: UICommandSender,
}

impl TradingTools {
    /// 创建 TradingTools 实例
    pub fn new(engine: Arc<MainEngine>, ui_sender: UICommandSender) -> Self {
        Self { engine, ui_sender }
    }
}

#[tool_router]
impl TradingTools {
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
}
