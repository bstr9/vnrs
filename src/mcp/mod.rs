//! MCP Server 模块 — 为交易系统提供 MCP 协议接口
//!
//! 本模块实现了完整的 MCP Server，包含：
//! - **Tools**: 后端交易操作（connect / subscribe / send_order / cancel_order / query_history / list_contracts / analyze_sentiment）
//!   前端 UI 操作（switch_symbol / switch_interval / add_indicator / remove_indicator / clear_indicators / navigate_to / show_notification）
//! - **Resources**: 实时交易数据和 UI 状态查询
//! - **Server**: TradingMcpServer 主服务器，支持 stdio 和 HTTP/SSE 两种传输模式
//! - **Sampling**: 支持 MCP Sampling，工具可向 Client 请求 LLM 推理
//!
//! # 使用方式
//!
//! ## STDIO 模式（本地客户端，如 Claude Desktop）
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use trade_engine::trader::MainEngine;
//! use trade_engine::mcp::TradingMcpServer;
//!
//! let engine = MainEngine::new();
//! let (server, ui_rx) = TradingMcpServer::new(Arc::new(engine));
//! tokio::spawn(async move {
//!     server.serve_stdio().await.unwrap();
//! });
//! ```
//!
//! ## HTTP/SSE 模式（远程 Web 客户端）
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use trade_engine::trader::MainEngine;
//! use trade_engine::mcp::TradingMcpServer;
//!
//! let engine = MainEngine::new();
//! let (server, ui_rx) = TradingMcpServer::new(Arc::new(engine));
//! tokio::spawn(async move {
//!     let addr = "127.0.0.1:3000".parse().unwrap();
//!     server.serve_http(addr).await.unwrap();
//! });
//! ```
//!
//! ## 使用 McpConfig 自动选择传输模式
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use trade_engine::trader::MainEngine;
//! use trade_engine::mcp::{TradingMcpServer, McpConfig};
//!
//! let engine = MainEngine::new();
//! let (server, ui_rx) = TradingMcpServer::new(Arc::new(engine));
//! let config = McpConfig::from_env(); // 读取 MCP_MODE 环境变量
//! tokio::spawn(async move {
//!     server.serve_with_config(&config).await.unwrap();
//! });
//! ```

pub mod types;
pub mod tools;
pub mod resources;
pub mod prompts;
pub mod server;

pub use server::TradingMcpServer;
pub use types::{McpConfig, McpTransport, SamplingConfig, SamplingAuditEntry, UICommand, UICommandReceiver, UICommandSender, UIState};
