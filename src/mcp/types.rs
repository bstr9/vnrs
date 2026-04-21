//! MCP 类型定义：UICommand 枚举、UIState 状态、通道类型、McpConfig 配置
//!
//! 定义 MCP Server 与 UI 线程之间的通信协议，
//! 包括 UI 命令（MCP→UI）和 UI 状态共享（UI→MCP）。

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tokio::sync::mpsc;

/// MCP 传输模式配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum McpTransport {
    /// STDIO 模式（适用于 Claude Desktop 等本地 MCP 客户端）
    #[default]
    Stdio,
    /// HTTP/SSE 模式（适用于远程 Web 客户端）
    Http {
        /// 监听端口
        port: u16,
        /// 监听地址（默认 127.0.0.1）
        host: Option<String>,
    },
}

impl McpTransport {
    /// 创建默认的 STDIO 配置
    pub fn stdio() -> Self {
        Self::Stdio
    }

    /// 创建 HTTP 配置
    pub fn http(port: u16) -> Self {
        Self::Http {
            port,
            host: None,
        }
    }

    /// 创建 HTTP 配置（带自定义 host）
    pub fn http_with_host(port: u16, host: impl Into<String>) -> Self {
        Self::Http {
            port,
            host: Some(host.into()),
        }
    }

    /// 从环境变量解析传输模式
    ///
    /// 支持的环境变量：
    /// - `MCP_MODE=stdio` → STDIO 模式
    /// - `MCP_MODE=http` → HTTP 模式（默认端口 3000）
    /// - `MCP_MODE=http:8080` → HTTP 模式（端口 8080）
    /// - `MCP_MODE=http:0.0.0.0:8080` → HTTP 模式（绑定所有接口，端口 8080）
    pub fn from_env() -> Self {
        std::env::var("MCP_MODE")
            .ok()
            .map(|v| Self::parse(&v))
            .unwrap_or_default()
    }

    /// 解析配置字符串
    pub fn parse(s: &str) -> Self {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.first().map(|s| s.to_lowercase()).as_deref() {
            Some("stdio") => Self::Stdio,
            Some("http") => {
                // Format: http[:port] or http[:host:port]
                // When 3 parts: http:host:port (host may contain dots)
                // When 2 parts: http:port
                if parts.len() >= 3 {
                    // http:host:port
                    let host = parts[1].to_string();
                    let port = parts
                        .get(2)
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(3000);
                    Self::Http { port, host: Some(host) }
                } else {
                    // http or http:port
                    let port = parts
                        .get(1)
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(3000);
                    Self::Http { port, host: None }
                }
            }
            _ => Self::Stdio,
        }
    }
}

/// MCP Server 配置
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    /// 传输模式
    pub transport: McpTransport,
    /// 只读模式：禁用所有写操作工具（send_order, cancel_order, set_stop_loss 等）
    #[serde(default)]
    pub read_only: bool,
    /// 允许的工具模块（空 = 全部允许）
    /// 可选值："trading", "ui", "market", "account", "strategy", "risk", "backtest"
    #[serde(default)]
    pub allowed_modules: HashSet<String>,
}

impl McpConfig {
    /// 创建默认配置（STDIO 模式）
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建 STDIO 模式配置
    pub fn stdio() -> Self {
        Self {
            transport: McpTransport::Stdio,
            read_only: false,
            allowed_modules: HashSet::new(),
        }
    }

    /// 创建 HTTP 模式配置
    pub fn http(port: u16) -> Self {
        Self {
            transport: McpTransport::http(port),
            read_only: false,
            allowed_modules: HashSet::new(),
        }
    }

    /// 创建只读模式配置
    pub fn read_only() -> Self {
        Self {
            transport: McpTransport::Stdio,
            read_only: true,
            allowed_modules: HashSet::new(),
        }
    }

    /// 设置只读模式
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// 设置允许的工具模块
    pub fn with_allowed_modules(mut self, modules: HashSet<String>) -> Self {
        self.allowed_modules = modules;
        self
    }

    /// 检查某个模块是否被允许
    pub fn is_module_allowed(&self, module: &str) -> bool {
        self.allowed_modules.is_empty() || self.allowed_modules.contains(module)
    }

    /// 检查当前配置是否为只读模式
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// 从环境变量读取配置
    pub fn from_env() -> Self {
        let read_only = std::env::var("MCP_READ_ONLY")
            .ok()
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        let allowed_modules = std::env::var("MCP_ALLOWED_MODULES")
            .ok()
            .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();

        Self {
            transport: McpTransport::from_env(),
            read_only,
            allowed_modules,
        }
    }
}

/// UI 命令，由 MCP Server 发送给 UI 线程执行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UICommand {
    // ---- 前端操作 ----
    /// 切换当前交易标的
    SwitchSymbol { symbol: String },
    /// 切换 K 线周期
    SwitchInterval { interval: String },
    /// 添加技术指标到图表
    AddIndicator {
        indicator_type: String,
        period: Option<usize>,
    },
    /// 按索引移除技术指标
    RemoveIndicator { index: usize },
    /// 清除所有技术指标
    ClearIndicators,
    /// 导航到指定标签页
    NavigateTo { tab: String },
    /// 显示通知消息
    ShowNotification { message: String, level: String },

    // ---- 后端操作（通过 MainEngine 执行） ----
    /// 连接到交易所网关
    Connect {
        gateway_name: String,
        settings: serde_json::Value,
    },
    /// 订阅行情
    Subscribe {
        symbol: String,
        exchange: String,
        gateway_name: String,
    },
    /// 发送委托订单
    SendOrder {
        symbol: String,
        exchange: String,
        direction: String,
        order_type: String,
        volume: f64,
        price: Option<f64>,
        offset: Option<String>,
        gateway_name: String,
    },
    /// 撤销委托订单
    CancelOrder {
        order_id: String,
        symbol: String,
        exchange: String,
        gateway_name: String,
    },
}

/// UI 状态，由 UI 线程共享给 MCP Server 读取
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UIState {
    /// 当前选中标的（vt_symbol 格式，如 BTCUSDT.BINANCE）
    pub current_symbol: Option<String>,
    /// 当前 K 线周期
    pub current_interval: Option<String>,
    /// 当前活动标签页
    pub active_tab: String,
    /// 图表上已加载的指标列表
    pub chart_indicators: Vec<String>,
}

/// MCP→UI 命令通道发送端类型
pub type UICommandSender = mpsc::UnboundedSender<UICommand>;

/// MCP→UI 命令通道接收端类型
pub type UICommandReceiver = mpsc::UnboundedReceiver<UICommand>;

// ---- Sampling 配置 ----

/// MCP Sampling 配置参数
///
/// 控制 Server 向 Client 发起 LLM Sampling 请求时的默认行为，
/// 包括 max_tokens、temperature 和模型偏好。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// 最大生成 token 数（默认 1024）
    pub max_tokens: u32,
    /// 温度参数，0.0~1.0（默认 0.7）
    pub temperature: f32,
    /// 首选模型名称（可选）
    #[serde(default)]
    pub model_preference: Option<String>,
}

impl Default for SamplingConfig {
    fn default() -> Self {
        Self {
            max_tokens: 1024,
            temperature: 0.7,
            model_preference: None,
        }
    }
}

/// Sampling 请求审计日志记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingAuditEntry {
    /// 请求时间戳（ISO 8601）
    pub timestamp: String,
    /// 发起请求的工具名称
    pub tool_name: String,
    /// 输入消息数量
    pub message_count: usize,
    /// 使用的 max_tokens
    pub max_tokens: u32,
    /// 使用的 temperature
    pub temperature: Option<f32>,
    /// 使用的 system_prompt（截断到前 100 字符）
    pub system_prompt_preview: Option<String>,
}
