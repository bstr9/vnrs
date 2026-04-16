//! MCP Server 模块 — 为交易系统提供 MCP 协议接口
//!
//! 本模块实现了完整的 MCP Server，包含：
//! - **Tools**: 后端交易操作（connect / subscribe / send_order / cancel_order / query_history / list_contracts）
//!   前端 UI 操作（switch_symbol / switch_interval / add_indicator / remove_indicator / clear_indicators / navigate_to / show_notification）
//! - **Resources**: 实时交易数据和 UI 状态查询
//! - **Server**: TradingMcpServer 主服务器，支持 stdio 模式
//!
//! # 使用方式
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use trade_engine::trader::MainEngine;
//! use trade_engine::mcp::TradingMcpServer;
//!
//! let engine = Arc::new(MainEngine::new());
//! let (server, ui_rx) = TradingMcpServer::new(engine);
//! // 在单独的 tokio task 中启动 MCP server
//! tokio::spawn(async move {
//!     server.serve_stdio().await.unwrap();
//! });
//! ```

pub mod types;
pub mod tools;
pub mod resources;
pub mod server;

pub use server::TradingMcpServer;
pub use types::{UICommand, UICommandReceiver, UICommandSender, UIState};
